import { randomBytes } from 'node:crypto'
import { dirname, join } from 'node:path'
import { chromium, expect, request, type FullConfig } from '@playwright/test'

/** Writes are allowed only on the fresh, loopback instance owned by run_staging.py. */
export default async function setup(config: FullConfig) {
    if (process.env.NOAR_E2E_WRITES !== '1') return
    const baseURL = config.projects[0].use.baseURL!
    const target = new URL(baseURL)
    if (process.env.NOAR_E2E_DISPOSABLE !== '1' || !['127.0.0.1', 'localhost', '[::1]'].includes(target.hostname)) {
        throw new Error('Authenticated tests require a fresh local instance owned by run_staging.py.')
    }
    const api = await request.newContext({ baseURL })
    try {
        const status = await api.get('/api/setup')
        if (!status.ok()) throw new Error(`Setup status: ${status.status()}`)
        const initial = await status.json()
        if (
            !initial.required ||
            !process.env.NOAR_E2E_STORAGE ||
            initial.settings?.storage !== process.env.NOAR_E2E_STORAGE
        ) {
            throw new Error('Refusing writes: staging must be uninitialized and use the runner-owned storage.')
        }
        const username = 'e2e-operator'
        const password = randomBytes(24).toString('hex')
        const completed = await api.post('/api/setup', {
            data: {
                ...initial.settings,
                shared: false,
                username,
                password,
                mail: 'operator@example.invalid',
                two_factor: false,
            },
        })
        if (!completed.ok()) throw new Error(`Setup failed: ${completed.status()} ${await completed.text()}`)
        if (!process.env.NOAR_E2E_STATE_PATH) throw new Error('Missing runner-owned session path.')
        const browser = await chromium.launch(config.projects[0].use.launchOptions)
        try {
            const context = await browser.newContext({ baseURL })
            const page = await context.newPage()
            await page.addInitScript(() => {
                if (!localStorage.getItem('language')) {
                    localStorage.setItem('language', 'en')
                }
            })
            await page.goto('/login')
            await page.locator('input[name="username"]').fill(username)
            await page.locator('input[name="password"]').fill(password)
            await page.getByRole('button', { name: 'Sign in', exact: true }).click()
            await expect(page).not.toHaveURL(/\/login(?:\?|$)/)
            // Verify first-run defaults before custom fixtures can hide a startup bug.
            // Seed additional artwork and a scene only after checking the stock logo.
            const token = await page.evaluate(() => localStorage.getItem('token'))
            const headers = { Authorization: `Bearer ${token}` }
            const loaded = await context.request.get('/api/playout/config/1', { headers })
            expect(loaded.ok()).toBeTruthy()
            const playout = await loaded.json()
            const assigned = await context.request.get('/api/graphics/1/channel', { headers })
            expect(assigned.ok()).toBeTruthy()
            const initialModels = await assigned.json()
            expect(initialModels).toHaveLength(1)
            const initialLibrary = await context.request.get('/api/graphics/1/templates', { headers })
            expect(await initialLibrary.json()).toHaveLength(1)
            const stock = initialModels.find((model: { kind: string }) => model.kind === 'logo')
            const image = stock.template.layers[0]
            expect((image.width * playout.output.width) / 100).toBeCloseTo(42, 4)
            expect((image.height * playout.output.height) / 100).toBeCloseTo(42, 4)
            expect(((100 - image.x - image.width) * playout.output.width) / 100).toBeCloseTo(12, 4)
            expect((image.y * playout.output.height) / 100).toBeCloseTo(12, 4)
            await page.goto('/graphics/onair?channel=1')
            const preview = page.getByRole('region', { name: 'Monitors', exact: true }).locator('figure').first()
            await expect(preview.locator('img')).toBeVisible()
            await expect
                .poll(() => preview.locator('img').evaluate((image) => (image as HTMLImageElement).naturalWidth))
                .toBe(42)
            await expect(
                page.getByText('The preview shows up when the playout is on air.', { exact: true }),
            ).toBeVisible()
            await expect(
                page.getByRole('heading', { name: 'Live graphics: logo and L-shape', exact: true }),
            ).toHaveCount(0)
            await page.screenshot({
                path: join(dirname(process.env.NOAR_E2E_STATE_PATH), 'fresh-onair-default.jpg'),
                type: 'jpeg',
                quality: 86,
            })
            await page.evaluate(() => {
                localStorage.setItem('language', 'pt-br')
                localStorage.setItem('theme', 'dark')
            })
            await page.goto('/graphics/templates?channel=1')
            await expect(page.locator('article')).toHaveCount(1)
            await expect(page.getByRole('heading', { name: 'Logo', exact: true })).toBeVisible()
            await expect(page.getByRole('heading', { name: 'Modelos', exact: true })).toBeVisible()
            await page.screenshot({
                path: join(dirname(process.env.NOAR_E2E_STATE_PATH), 'fresh-only-logo-pt-BR.jpg'),
                type: 'jpeg',
                quality: 88,
            })
            await page.setViewportSize({ width: 390, height: 844 })
            expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true)
            await page.screenshot({
                path: join(dirname(process.env.NOAR_E2E_STATE_PATH), 'fresh-only-logo-phone-pt-BR.jpg'),
                type: 'jpeg',
                quality: 88,
                fullPage: true,
            })
            await page.evaluate(() => localStorage.setItem('language', 'en'))

            Object.assign(playout.processing, {
                add_logo: true,
                logo: 'logo.png',
                logo_scale: '80:48',
                logo_position: 'W-w-12:12',
                lshape: 'logo.png',
                graphics_scenes: [{ name: 'E2E scene', logo: 'show', lshape: 'off' }],
            })
            const saved = await context.request.put('/api/playout/config/1', { headers, data: playout })
            expect(saved.ok(), await saved.text()).toBeTruthy()
            // Compatibility fixtures are explicitly authored after first-run assertions.
            const tickerCreated = await context.request.post('/api/graphics/1/templates', {
                headers,
                data: { name: 'Ticker', group: 'Channel graphics', duration: 0, always_on: false, layers: [] },
            })
            expect(tickerCreated.ok()).toBeTruthy()
            const ticker = await tickerCreated.json()
            expect(
                (
                    await context.request.put(`/api/graphics/1/templates/${ticker.id}/assignment`, {
                        headers,
                        data: { role: 'ticker' },
                    })
                ).ok(),
            ).toBeTruthy()
            const roleResponse = await context.request.get('/api/graphics/1/channel', { headers })

            for (const role of await roleResponse.json()) {
                if (role.kind === 'scene') {
                    continue
                }

                role.template.name =
                    role.kind === 'logo' ? 'Channel logo' : role.kind === 'lshape' ? 'Channel L-shape' : 'Ticker'
                role.template.group = 'Channel graphics'
                expect(
                    (
                        await context.request.put(`/api/graphics/1/templates/${role.template.id}`, {
                            headers,
                            data: role.template,
                        })
                    ).ok(),
                ).toBeTruthy()
            }

            await context.storageState({ path: process.env.NOAR_E2E_STATE_PATH })
            await context.close()
        } finally {
            await browser.close()
        }
    } finally {
        await api.dispose()
    }
}
