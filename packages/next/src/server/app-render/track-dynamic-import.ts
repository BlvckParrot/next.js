import { InvariantError } from '../../shared/lib/invariant-error'
import { isThenable } from '../../shared/lib/is-thenable'
import type { CacheSignal } from './cache-signal'
import {
  trackPendingAsyncImport,
  subscribeToPendingModules,
} from './track-module-loading.external'
import { workUnitAsyncStorage } from './work-unit-async-storage.external'

/**
 * in DynamicIO, `import(...)` will be transformed into `trackDynamicImport(import(...))`.
 * A dynamic import is essentially a cached async function, except it's cached by the module system.
 *
 * - If an `import()` happens in a component, we'll track the promise on the cacheSignal directly
 * - If it happens at the top level of a module, we don't have a cacheSignal, so we'll track it globally instead.
 *   then, when rendering, we can make the `cacheSignal wait for those via `trackPendingTopLevelModules`.
 * */
export function trackDynamicImport<TExports extends Record<string, any>>(
  modulePromise: Promise<TExports>
): Promise<TExports> {
  if (!isThenable(modulePromise)) {
    // We're expecting `import()` to always return a promise. If it's not, something's very wrong.
    throw new InvariantError('Expected the argument to be a promise')
  }

  const workUnitStore = workUnitAsyncStorage.getStore()
  let cacheSignal =
    workUnitStore && workUnitStore.type === 'prerender'
      ? workUnitStore.cacheSignal
      : null

  if (cacheSignal) {
    // we expect the caller to look like `trackDynamicImport(import(...))`.
    // if that's true, then we're in the same microtick (and thus we didn't begin the read too late)
    cacheSignal.trackRead(modulePromise)
  } else {
    // if we're not inside a render, track it globally.
    trackPendingAsyncImport(modulePromise)
  }
  return modulePromise
}

/**
 * A top-level dynamic import or a chunk load may reveal more caches,
 * so if we see one, we make the `CacheSignal` wait for it to complete.
 * (`trackDynamicImport` already tracks these if they happen in a component,
 *  but we also need to do handle imports that happen outside of render.)
 *
 * We're not using `waitForPendingModules`,
 * because we might start and finish multiple batches of module loads while waiting for caches,
 * and `waitForPendingModules` would resolve after the first batch.
 * Instead, the import/chunk-load tracking mechanism will notify the cache signal
 * of each import/chunk-load that happens, and we'll delay `cacheReady` until all of them are done.
 *
 * There's a potential race if the page does some imports at the top level with a tasky delay:
 *
 * ```tsx
 *   const modulePromise = createPromiseWithResolvers();
 *
 *   void (async () => {
 *     const id = await uncachedFetch(); // tasky
 *     return import(`./foo/${id}`);
 *   })().then(
 *     (mod) => modulePromise.resolve(mod),
 *     (err) => modulePromise.reject(err)
 *   );
 *
 *   export default async function Page() {
 *     const mod = await modulePromise
 *     ...
 *   }
 * ```
 * In that case, if the `CacheSignal` wasn't already waiting for any other caches when the `import()` is called,
 * It may have already resolved `cacheReady()`, so we'd miss this in the prospective render
 * and likely fail in the actual prerender.
 */
export function trackPendingTopLevelModules(cacheSignal: CacheSignal) {
  const unsubscribe = subscribeToPendingModules((promise) =>
    cacheSignal.trackRead(promise)
  )
  cacheSignal.cacheReady().then(unsubscribe)
}
