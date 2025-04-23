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
})
