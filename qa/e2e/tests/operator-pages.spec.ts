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

test('Settings › Activity lists what users did, with filters and a CSV', async ({ page }) => {
    const headers = await adminHeaders(page)
    const username = 'e2e-activity-user'
    const created = await page.request.post('/api/user', {
        headers,
        data: {
            username,
            password: randomBytes(18).toString('hex'),
            mail: 'activity@example.invalid',
            role_id: 3,
            channel_ids: [1],
            two_factor: false,
        },
    })
    expect(created.ok(), `create: ${created.status()}`).toBeTruthy()
    const users = await (await page.request.get('/api/users', { headers })).json()
    const id = users.find((user: { username: string }) => user.username === username).id
    await page.request.delete(`/api/user/${id}`, { headers })

    await page.goto('/configure/activity?channel=1')
    await expect(page.getByRole('heading', { name: 'Activity', level: 1 })).toBeVisible()
    const filters = page.getByRole('region', { name: 'Filters', exact: true })
    const table = page.getByRole('table', { name: 'Activity' })
    await filters.getByLabel('Type', { exact: true }).selectOption('user')
    await expect(table.getByRole('row').filter({ hasText: 'Deleted a user' }).first()).toBeVisible()
    await expect(
        table.getByRole('row').filter({ hasText: 'Created a user' }).filter({ hasText: username }),
    ).toBeVisible()

    // The search keeps the entries that name the user: created, then deleted.
    await filters.getByLabel('Search', { exact: true }).fill(username)
    await expect(table.locator('tbody tr')).toHaveCount(2)
    await expect(table.locator('tbody tr').nth(0)).toContainText('Deleted a user')
    await expect(table.locator('tbody tr').nth(0)).toContainText(username)
    await expect(table.locator('tbody tr').nth(1)).toContainText('Created a user')
    await expect(table.locator('tbody tr').nth(1)).toContainText('Done')

    const download = page.waitForEvent('download')
    await page.getByRole('button', { name: 'Export CSV', exact: true }).click()
    const file = await download
    expect(file.suggestedFilename()).toMatch(/^noar-activity-.*\.csv$/)
    const { readFile } = await import('node:fs/promises')
    expect(await readFile(await file.path(), 'utf8')).toContain(username)
})
