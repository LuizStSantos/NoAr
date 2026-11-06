import { randomBytes } from 'node:crypto'
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
            await page.addInitScript(() => localStorage.setItem('language', 'en'))
            await page.goto('/login')
            await page.locator('input[name="username"]').fill(username)
            await page.locator('input[name="password"]').fill(password)
            await page.getByRole('button', { name: 'Sign in', exact: true }).click()
            await expect(page).not.toHaveURL(/\/login(?:\?|$)/)
            // Seed the channel roles exercised by the graphics tests. The runner owns
            // both files; a fresh installation has no scene or readable default logo.
            const token = await page.evaluate(() => localStorage.getItem('token'))
            const headers = { Authorization: `Bearer ${token}` }
            const loaded = await context.request.get('/api/playout/config/1', { headers })
            expect(loaded.ok()).toBeTruthy()
            const playout = await loaded.json()
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
            await context.storageState({ path: process.env.NOAR_E2E_STATE_PATH })
            await context.close()
        } finally {
            await browser.close()
        }
    } finally {
        await api.dispose()
    }
}
