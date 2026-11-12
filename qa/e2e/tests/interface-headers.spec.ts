import { test, expect } from '@playwright/test'

// The security headers exist only where the backend serves the interface
// itself, as the Docker image does (run_staging.py --embedded). Vite, in the
// default run, sends none.
test.skip(process.env.NOAR_E2E_EMBEDDED !== '1', 'Run with run_staging.py --embedded: Vite sends no security headers.')

test('the interface is served with its security headers', async ({ request }) => {
  const page = await request.get('/login')
  expect(page.status()).toBe(200)
  const headers = page.headers()
  expect(headers['x-content-type-options']).toBe('nosniff')
  expect(headers['referrer-policy']).toBe('same-origin')
  const policy = headers['content-security-policy']
  expect(policy).toContain("script-src 'self';")
  expect(policy).toContain("object-src 'none'")
  expect(policy).not.toContain('unsafe-eval')
  // Open unless NOAR_FRAME_ANCESTORS restricts it.
  expect(policy).not.toContain('frame-ancestors')

  // Scripts and styles say what they are; the policy is for pages only.
  const script = /src="(\/assets\/[^"]+\.js)"/.exec(await page.text())
  expect(script).not.toBeNull()
  const asset = await request.get(script![1])
  expect(asset.headers()['x-content-type-options']).toBe('nosniff')
  expect(asset.headers()['content-type']).toMatch(/javascript/)
  expect(asset.headers()['content-security-policy']).toBeUndefined()
  const api = await request.get('/api/setup')
  expect(api.headers()['content-security-policy']).toBeUndefined()
})

test('a page refuses a script that is not one of its own and still starts its workers', async ({ page }) => {
  await page.goto('/login')
  await expect(page.locator('input[name="password"]')).toBeVisible()

  // What a script slipped into the page would try. Code run by
  // page.evaluate itself goes through DevTools, which the policy does not
  // govern, so eval is checked through the header in the test above; what it
  // adds to the page is governed.
  const result = await page.evaluate(async () => {
    const marks = window as unknown as { __inline?: boolean; __data?: boolean; __handler?: boolean }

    const inline = document.createElement('script')
    inline.textContent = 'window.__inline = true'
    document.head.appendChild(inline)

    const fromData = document.createElement('script')
    fromData.src = 'data:text/javascript,window.__data=true'
    const data = new Promise<string>((resolve) => {
      fromData.onload = () => resolve('loaded')
      fromData.onerror = () => resolve('refused')
      setTimeout(() => resolve('no answer'), 3000)
    })
    document.head.appendChild(fromData)

    const image = document.createElement('img')
    image.setAttribute('onerror', 'window.__handler = true')
    const failed = new Promise<void>((resolve) => image.addEventListener('error', () => resolve(), { once: true }))
    image.src = 'data:,not-an-image'
    document.body.appendChild(image)
    await failed

    // What the players need: a worker made from a blob.
    const source = URL.createObjectURL(new Blob(['postMessage("ready")'], { type: 'text/javascript' }))
    const worker = await new Promise<string>((resolve) => {
      try {
        const made = new Worker(source)
        made.onmessage = (event) => resolve(String(event.data))
        made.onerror = () => resolve('refused')
      } catch {
        resolve('refused')
      }
      setTimeout(() => resolve('no answer'), 3000)
    })

    return {
      inline: marks.__inline === true,
      data: await data,
      dataRan: marks.__data === true,
      handler: marks.__handler === true,
      worker,
    }
  })

  expect(result).toEqual({ inline: false, data: 'refused', dataRan: false, handler: false, worker: 'ready' })
})

test.describe('signed in', () => {
  test.skip(process.env.NOAR_E2E_WRITES !== '1', 'Run with run_staging.py for disposable authenticated tests.')
  test.use({ storageState: process.env.NOAR_E2E_STATE_PATH })

  test('no page of the interface breaks its content security policy', async ({ page }) => {
    test.setTimeout(120_000)
    const violations: string[] = []
    page.on('console', (message) => {
      if (/content security policy|CSP violation/i.test(message.text())) {
        violations.push(message.text())
      }
    })
    await page.addInitScript(() => {
      document.addEventListener('securitypolicyviolation', (event) => {
        console.error(`CSP violation: ${event.violatedDirective} blocked ${event.blockedURI || 'inline'} on ${location.pathname}`)
      })
    })

    const routes = [
      '/player',
      '/playlist',
      '/schedule',
      '/graphics/onair',
      '/graphics/templates',
      '/graphics/composer',
      '/graphics/sources',
      '/media',
      '/report',
      '/configure/general',
      '/configure/logging',
      '/configure/output',
      '/configure/users',
    ]

    for (const route of routes) {
      await page.goto(`${route}?channel=1`)
      await expect(page).not.toHaveURL(/\/login(?:\?|$)/)
      await expect(page.getByRole('navigation', { name: 'Main', exact: true })).toBeVisible()
      // Lazy parts of the page (players, editors) load after the first paint.
      await page.waitForTimeout(1500)
    }

    expect(violations).toEqual([])
  })
})
