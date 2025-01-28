use std::iter;

use indexmap::{indexset, IndexSet};
use lazy_static::lazy_static;
use serde::Deserialize;
use swc_core::{
    atoms::Atom,
    common::{comments::Comments, util::take::Take, Mark, Span, SyntaxContext},
    ecma::{
        ast::*,
        utils::{prepend_stmts, private_ident, quote_ident, quote_str, StmtOrModuleItem},
        visit::{noop_visit_mut_type, visit_mut_pass, VisitMut, VisitMutWith},
    },
    quote,
};

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Config {}

pub fn track_dynamic_imports<C: Comments>(
    unresolved_mark: Mark,
    comments: C,
) -> impl VisitMut + Pass {
    visit_mut_pass(ImportReplacer::new(unresolved_mark, comments))
}

struct ImportReplacer<C> {
    comments: C,
    unresolved_ctxt: SyntaxContext,
    track_dynamic_import_local_ident: Ident,
    track_async_function_local_ident: Ident,
    has_dynamic_import: bool,
    identifiers_to_instrument: IndexSet<Atom>,
}

impl<C> ImportReplacer<C>
where
    C: Comments,
{
    pub fn new(unresolved_mark: Mark, comments: C) -> Self {
        ImportReplacer {
            comments,
            unresolved_ctxt: SyntaxContext::empty().apply_mark(unresolved_mark),
            track_dynamic_import_local_ident: private_ident!("$$trackDynamicImport__"),
            track_async_function_local_ident: private_ident!("$$trackAsyncFunction__"),
            has_dynamic_import: false,
            identifiers_to_instrument: IndexSet::new(),
        }
    }
}

lazy_static! {
    static ref GLOBALS_TO_INSTRUMENT: IndexSet<Atom> = indexset! {
        "__turbopack_load__".into(),
        "__webpack_load__".into(),
        "__webpack_require__".into(),
        "__turbopack_require__".into(),
    };
}

impl<C> VisitMut for ImportReplacer<C>
where
    C: Comments,
{
    noop_visit_mut_type!(); // TODO: what does this do?

    fn visit_mut_program(&mut self, program: &mut Program) {
        program.visit_mut_children_with(self);
        // if we wrapped a dynamic import while visiting the children, we need to import the wrapper

        // import()

        if self.has_dynamic_import {
            match program {
                Program::Module(module) => {
                    prepend_stmts(
                        &mut module.body,
                        iter::once(quote!(
                            "import { trackDynamicImport as $wrapper_fn } from \
                             'private-next-rsc-track-dynamic-import'"
                                as ModuleItem,
                            wrapper_fn = self.track_dynamic_import_local_ident.clone()
                        )),
                    );
                }
                Program::Script(script) => {
                    // CJS modules can still use `import()`. for CJS, we have to inject the helper
                    // using `require` instead of `import` to avoid accidentally turning them
                    // into ESM modules.
                    prepend_stmts(
                        &mut script.body,
                        iter::once(quote!(
                            "const { trackDynamicImport: $wrapper_fn } = \
                             $require('private-next-rsc-track-dynamic-import')"
                                as Stmt,
                            wrapper_fn = self.track_dynamic_import_local_ident.clone(),
                            // the builtin `require` is considered an unresolved identifier.
                            // we have to match that, or it won't be recognized as
                            // a proper `require()` call.
                            require = quote_ident!(self.unresolved_ctxt, "require")
                        )),
                    );
                }
            }
        }

        // bundler globals

        let mut stmts: Vec<ModuleItem> = vec![];

        let mut added_track_async_function = false;
        for name in &self.identifiers_to_instrument {
            if !added_track_async_function {
                stmts.push(match program {
                    Program::Module(..) => {
                        quote!(
                            "import { trackAsyncFunction as $wrapper_fn } from \
                             'private-next-rsc-track-dynamic-import'"
                                as ModuleItem,
                            wrapper_fn = self.track_async_function_local_ident.clone()
                        )
                    }
                    Program::Script(..) => {
                        quote!(
                            "const { trackAsyncFunction: $wrapper_fn } = \
                             $require('private-next-rsc-track-dynamic-import')"
                                as ModuleItem,
                            wrapper_fn = self.track_async_function_local_ident.clone(),
                            // the builtin `require` is considered an unresolved identifier.
                            // we have to match that, or it won't be recognized as
                            // a proper `require()` call.
                            require = quote_ident!(self.unresolved_ctxt, "require")
                        )
                    }
                });
                added_track_async_function = true
            }

            let name = name.as_str();
            let name_ident: Ident = quote_ident!(self.unresolved_ctxt, name).into();

            let replacement_expr = {
                let expr_span = Span::dummy_with_cmt();
                let mut expr: Expr = quote!(
                    "$wrapper_fn($name_string, $name)" as Expr,
                    wrapper_fn = self.track_async_function_local_ident.clone(),
                    name_string: Expr = quote_str!(name).into(),
                    name = name_ident.clone(),
                );

                // this call doesn't have any side effects, so add `/*#__PURE__*/`
                expr.set_span(expr_span);
                self.comments.add_pure_comment(expr_span.lo);
                expr
            };

            stmts.push(quote!(
                "if (typeof $name === 'function') {\
                    $name = $replacement_expr;\
                }" as ModuleItem,
                name = name_ident.clone(),
                replacement_expr: Expr = replacement_expr,
            ));
        }

        match program {
            Program::Module(module) => {
                prepend_stmts(&mut module.body, stmts.drain(..));
            }
            Program::Script(script) => {
                // CJS modules can still use `import()`. for CJS, we have to inject the helper
                // using `require` instead of `import` to avoid accidentally turning them
                // into ESM modules.
                prepend_stmts(
                    &mut script.body,
                    stmts
                        .into_iter()
                        .filter_map(|item| item.into_stmt().ok())
                        .collect::<Vec<_>>()
                        .drain(..),
                );
            }
        }
    }

    fn visit_mut_expr(&mut self, expr: &mut Expr) {
        expr.visit_mut_children_with(self);

        // before: `import(...)`
        // after:  `$$trackDynamicImport__(import(...))`

        if let Expr::Call(CallExpr {
            callee: Callee::Import(_),
            ..
        }) = expr
        {
            self.has_dynamic_import = true;
            let mut replacement_expr = quote!(
                "$wrapper_fn($expr)" as Expr,
                wrapper_fn = self.track_dynamic_import_local_ident.clone(),
                expr: Expr = expr.take()
            );
            // this call doesn't have any side effects, so add `/*#__PURE__*/`
            let replacement_expr_span = Span::dummy_with_cmt();
            replacement_expr.set_span(replacement_expr_span);
            self.comments.add_pure_comment(replacement_expr_span.lo);
            *expr = replacement_expr
        }
    }

    fn visit_mut_ident(&mut self, ident: &mut Ident) {
        // find references to bundler globals like `__turbopack_load__`
        //
        // "globals" like this use the unresolved syntax context
        // https://rustdoc.swc.rs/swc_core/ecma/transforms/base/fn.resolver.html#unresolved_mark
        // if it's not unresolved, then there's a local redefinition which we don't want to touch
        // TODO: we should replace this reference with a reference to our wrapper instead
        if ident.ctxt == self.unresolved_ctxt && GLOBALS_TO_INSTRUMENT.contains(&ident.sym) {
            self.identifiers_to_instrument.insert(ident.sym.clone());
        }
    }
}
