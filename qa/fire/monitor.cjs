/* Optional post-run browser proof; uses only an existing, owned fire fixture. */
const fs = require('node:fs')
const path = require('node:path')
const { chromium, expect } = require('../e2e/node_modules/@playwright/test')
const { waitForProgram } = require('./browser.cjs')

async function main() {
    const root = path.resolve(process.argv[2] || '')

    if (!fs.existsSync(path.join(root, '.qa-fire-owned'))) {
        throw new Error('An owned disposable fixture is required')
    }

    const auth = JSON.parse(fs.readFileSync(path.join(root, 'auth.json')))
    const plan = JSON.parse(fs.readFileSync(path.join(root, 'ready.json')))
    const browser = await chromium.launch({
        executablePath: '/usr/bin/chromium',
        args: ['--no-sandbox', '--autoplay-policy=no-user-gesture-required'],
    })
    const page = await browser.newPage({
        baseURL: 'http://127.0.0.1:5757',
        viewport: { width: 1440, height: 1000 },
        timezoneId: 'UTC',
    })
    const result = { video: [], page_errors: [] }
    let headers
    let event
    page.on('pageerror', (error) => result.page_errors.push(String(error)))

    async function api(method, endpoint, data) {
        const response = await page.request.fetch(endpoint, { method, headers, data })
        expect(response.ok(), `${endpoint}: ${response.status()} ${await response.text()}`).toBeTruthy()
        const text = await response.text()

        try {
            return JSON.parse(text)
        } catch {
            return text
        }
    }

    try {
        await page.addInitScript(() => {
            if (!localStorage.getItem('language')) {
                localStorage.setItem('language', 'en')
                localStorage.setItem('theme', 'dark')
            }
        })
        await page.goto('/login')
        await page.locator('input[name="username"]').fill(auth.username)
        await page.locator('input[name="password"]').fill(auth.password)
        await page.getByRole('button', { name: 'Sign in', exact: true }).click()
        await expect(page).not.toHaveURL(/\/login/)
        headers = { Authorization: `Bearer ${await page.evaluate(() => localStorage.getItem('token'))}` }
        const channel = await api('GET', '/api/channel/1')

        if (channel.storage !== path.join(root, 'media')) {
            throw new Error('The backend does not belong to the owned fixture')
        }

        await api('POST', '/api/control/1/process', { command: 'start' })
        const begin = new Date(Date.now() + 5000)
        const end = new Date(begin.getTime() + 120000)
        event = await api('POST', '/api/schedule/1', {
            kind: 'live',
            title: 'Browser monitor proof',
            sources: ['Lives/Fire UDP/signal.live'],
            start_date: begin.toISOString().slice(0, 10),
            start_time: begin.toISOString().slice(11, 19),
            end_time: end.toISOString().slice(11, 19),
            entry_mode: 'interrupt',
            after_mode: 'clock',
            retry_seconds: 120,
        })
        await expect
            .poll(async () => (await api('GET', '/api/schedule/1/status')).active?.event_id, { timeout: 30000 })
            .toBe(event.id)
        await api('POST', '/api/control/1/lshape', { action: 'on' })
        await page.goto(`/graphics/onair?channel=1&template=${plan.gc_id}`)
        result.text = `BROWSER ${begin.toISOString().slice(11, 19)}`
        await page.getByLabel('{name}', { exact: true }).fill(result.text)
        await page.getByLabel('{role}', { exact: true }).fill('UDP live')
        const taking = page.waitForResponse(
            (response) => response.url().endsWith('/api/graphics/1/take') && response.request().method() === 'POST',
        )
        await page.getByRole('button', { name: /^put on air$/i }).click()
        const taken = await taking
        expect(taken.ok(), await taken.text()).toBeTruthy()
        result.take = await taken.json()
        await page.evaluate(() => localStorage.setItem('language', 'pt-br'))
        await page.reload()
        await waitForProgram(page)
        // Allow the newly taken text to traverse the HLS live-edge buffer.
        await page.waitForTimeout(10000)
        result.video.push({ screen: 'desktop', ...(await waitForProgram(page)) })
        await page.screenshot({
            path: path.join(root, 'captures/fire-onair-verified-pt-BR.jpg'),
            type: 'jpeg',
            quality: 88,
        })
        await page.setViewportSize({ width: 390, height: 844 })
        result.video.push({ screen: 'phone', ...(await waitForProgram(page)) })
        await page.screenshot({
            path: path.join(root, 'captures/fire-onair-verified-mobile-pt-BR.jpg'),
            type: 'jpeg',
            quality: 88,
            fullPage: true,
        })
        result.media = await api('GET', '/api/control/1/media/current')
        expect(result.media.media.source).toBe('udp://127.0.0.1:5004')
        await page.setViewportSize({ width: 1440, height: 1000 })
        await page.goto('/schedule')
        await expect(page.getByRole('row').filter({ hasText: 'Browser monitor proof' })).toBeVisible()
        await page.screenshot({ path: path.join(root, 'captures/fire-scheduler-pt-BR.jpg'), type: 'jpeg', quality: 88 })
        await page.goto('/graphics/templates?channel=1')
        const templates = await api('GET', '/api/graphics/1/templates')
        result.templates = templates.length
        await expect(page.locator('article')).toHaveCount(templates.length)
        await page.screenshot({ path: path.join(root, 'captures/fire-models-pt-BR.jpg'), type: 'jpeg', quality: 88 })
        expect(result.page_errors).toEqual([])
        result.status = 'PASS'
    } catch (error) {
        result.status = 'FAIL'
        result.error = String(error)
        throw error
    } finally {
        if (headers) {
            await api('POST', '/api/graphics/1/out', {}).catch(() => {})
            await api('POST', '/api/control/1/lshape', { action: 'off' }).catch(() => {})
            await api('POST', '/api/control/1/process', { command: 'stop' }).catch(() => {})

            if (event) {
                await api('DELETE', `/api/schedule/1/${event.id}`).catch(() => {})
            }
        }

        fs.writeFileSync(path.join(root, 'browser-monitor-proof.json'), JSON.stringify(result, null, 2))
        await browser.close()
    }
}

main().catch((error) => {
    console.error(error)
    process.exitCode = 1
})
