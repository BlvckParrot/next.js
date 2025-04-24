import { nextTestSetup } from 'e2e-utils'

describe('async imports in dynamicIO', () => {
  const { next } = nextTestSetup({
    files: __dirname,
  })

  describe('inside a component', () => {
    it('import of a sync module', async () => {
      const browser = await next.browser('/inside-component/sync-module')
      expect(await browser.elementByCss('body').text()).toBe('hello')
    })

    it('import of module with top-level-await', async () => {
      const browser = await next.browser('/inside-component/async-module')
      expect(await browser.elementByCss('body').text()).toBe('hello')
    })
  })

  describe('unawaited at the top level', () => {
    it('import of a sync module', async () => {
      const browser = await next.browser('/top-level/sync-module')
      expect(await browser.elementByCss('body').text()).toBe('hello')
    })

    it('import of module with top-level-await', async () => {
      const browser = await next.browser('/top-level/async-module')
      expect(await browser.elementByCss('body').text()).toBe('hello')
    })
  })

  describe('dynamic import inside node_modules', () => {
    it('import of a sync module', async () => {
      const browser = await next.browser('/from-node-modules/sync-module')
      expect(await browser.elementByCss('body').text()).toBe('hello')
    })

    it('import of module with top-level-await', async () => {
      const browser = await next.browser('/from-node-modules/async-module')
      expect(await browser.elementByCss('body').text()).toBe('hello')
    })
  })

  // TODO:
  // - depending on a shared async module (rn they're all isolated)
  // - imports inside an external
  // - imports in client modules
  // - imports in a prerendered GET handler
  // likely to fail:
  // - unawaited import with a tasky delay (and nothing else to delay `cacheSignal`)
  // - TLA in a client component that is not a segment and is only imported from server components (so it's missed by `warmAllModulesInTree`)
})
