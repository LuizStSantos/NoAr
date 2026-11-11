import { randomBytes } from 'node:crypto'
import { test, expect, type Page } from '@playwright/test'

test.skip(process.env.NOAR_E2E_WRITES !== '1', 'Run with run_staging.py for disposable authenticated tests.')
test.use({ storageState: process.env.NOAR_E2E_STATE_PATH })

async function adminHeaders(page: Page) {
    await page.goto('/')
    const token = await page.evaluate(() => localStorage.getItem('token'))

    return { Authorization: `Bearer ${token}` }
}

function pathOf(page: Page) {
    return new URL(page.url()).pathname
}

test('an operator follows the pages a global admin gives them without signing in again', async ({ page, browser }) => {
    test.setTimeout(90_000)
    const headers = await adminHeaders(page)
    const username = 'e2e-limited-operator'
    const password = randomBytes(18).toString('hex')
    const created = await page.request.post('/api/user', {
        headers,
        data: {
            username,
            password,
            mail: 'limited@example.invalid',
            role_id: 3,
            channel_ids: [1],
            two_factor: false,
            areas: ['player', 'schedule'],
        },
    })
    expect(created.ok(), `create: ${created.status()}`).toBeTruthy()
    const users = await (await page.request.get('/api/users', { headers })).json()
    const id = users.find((user: { username: string }) => user.username === username).id
    const setAreas = async (areas: string[] | null) => {
        const response = await page.request.put(`/api/user/${id}`, {
            headers,
            data: { username, channel_ids: [1], areas },
        })
        expect(response.ok(), `areas: ${response.status()}`).toBeTruthy()
    }

    // A browser of its own, without the admin's session.
    const context = await browser.newContext({
        baseURL: process.env.NOAR_E2E_BASE_URL,
        storageState: { cookies: [], origins: [] },
    })
    try {
        const operator = await context.newPage()
        await operator.addInitScript(() => localStorage.setItem('language', 'en'))
        await operator.goto('/login')
        await operator.locator('input[name="username"]').fill(username)
        await operator.locator('input[name="password"]').fill(password)
        await operator.getByRole('button', { name: 'Sign in', exact: true }).click()
        await expect(operator).not.toHaveURL(/\/login(?:\?|$)/)
        const menu = operator.getByRole('navigation', { name: 'Main', exact: true })
        await expect(menu.getByRole('link', { name: 'Scheduler', exact: true })).toBeVisible()
        await expect(menu.getByRole('link', { name: 'Report', exact: true })).toHaveCount(0)

        await operator.goto('/schedule?channel=1')
        expect(pathOf(operator)).toBe('/schedule')

        // The Scheduler is taken away while it is open: its next refresh
        // renews the token, the menu drops it and the page closes.
        await setAreas(['player'])
        await expect.poll(() => pathOf(operator), { timeout: 20_000 }).toBe('/')
        await expect(menu.getByRole('link', { name: 'Scheduler', exact: true })).toHaveCount(0)
        await expect(operator).not.toHaveURL(/\/login(?:\?|$)/)

        // Every page back: the next page opened brings the menu back.
        await setAreas(null)
        await menu.getByRole('link', { name: 'Player', exact: true }).click()
        await expect(menu.getByRole('link', { name: 'Report', exact: true })).toBeVisible({ timeout: 20_000 })
        await expect(menu.getByRole('link', { name: 'Scheduler', exact: true })).toBeVisible()
    } finally {
        await context.close()
        await page.request.delete(`/api/user/${id}`, { headers })
    }
})
