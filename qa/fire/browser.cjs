/* Browser operations against run.py's explicitly owned disposable installation. */
const fs = require('node:fs')
const path = require('node:path')
const { chromium, expect } = require('../e2e/node_modules/@playwright/test')

async function waitForProgram(page) {
    const video = page.locator('video').first()
    await expect(video).toBeVisible()
    await expect.poll(() => video.evaluate((element) => element.videoWidth), { timeout: 30000 }).toBeGreaterThan(0)
    return video.evaluate(async (element) => {
        async function presentedFrame() {
            return new Promise((resolve, reject) => {
                const timeout = setTimeout(
                    () => reject(new Error('Program did not present another video frame')),
                    10000,
                )
                element.requestVideoFrameCallback((_, metadata) => {
                    clearTimeout(timeout)
                    resolve(metadata)
                })
            })
        }

        const first = await presentedFrame()
        const second = await presentedFrame()

        if (second.mediaTime <= first.mediaTime || second.presentedFrames <= first.presentedFrames) {
            throw new Error('Program video frames did not advance')
        }

        return {
            width: element.videoWidth,
            height: element.videoHeight,
            previousTime: first.mediaTime,
            currentTime: second.mediaTime,
            previousFrames: first.presentedFrames,
            presentedFrames: second.presentedFrames,
        }
    })
}

async function main() {
    const root = path.resolve(process.argv[2] || '')

    if (!fs.existsSync(path.join(root, '.qa-fire-owned'))) {
        throw new Error('Refusing browser writes without the owned fixture marker')
    }

    const provisionDeadline = Date.now() + 120000

    while (!fs.existsSync(path.join(root, 'ready.json'))) {
        if (Date.now() > provisionDeadline || fs.existsSync(path.join(root, 'report.json'))) {
            throw new Error('Native fixture failed or did not become ready within two minutes')
        }

        await new Promise((resolve) => setTimeout(resolve, 1000))
    }

    const plan = JSON.parse(fs.readFileSync(path.join(root, 'ready.json')))
    const credentials = JSON.parse(fs.readFileSync(path.join(root, 'auth.json')))
    const browser = await chromium.launch({
        executablePath: '/usr/bin/chromium',
        args: ['--no-sandbox', '--autoplay-policy=no-user-gesture-required'],
    })
    const page = await browser.newPage({
        baseURL: 'http://127.0.0.1:5757',
        viewport: { width: 1440, height: 1000 },
        timezoneId: 'UTC',
    })
    const receipts = []
    const errors = []
    page.on('pageerror', (error) => errors.push(String(error)))

    try {
        await page.addInitScript(() => {
            if (!localStorage.getItem('language')) {
                localStorage.setItem('language', 'en')
                localStorage.setItem('theme', 'dark')
            }
        })
        await page.goto('/login')
        await page.locator('input[name="username"]').fill(credentials.username)
        await page.locator('input[name="password"]').fill(credentials.password)
        await page.getByRole('button', { name: 'Sign in', exact: true }).click()
        await expect(page).not.toHaveURL(/\/login/)

        const token = await page.evaluate(() => localStorage.getItem('token'))
        const config = await page.request.get('/api/channel/1', { headers: { Authorization: `Bearer ${token}` } })
        const settings = await config.json()

        if (settings.storage !== path.join(root, 'media')) {
            throw new Error('The browser backend does not use the runner-owned storage')
        }

        if (!fs.existsSync(path.join(root, 'ui-ready.json'))) {
            await page.goto('/schedule')
            await page.getByRole('button', { name: 'New live', exact: true }).first().click()
            let dialog = page.getByRole('dialog')
            await dialog.getByLabel('Name', { exact: true }).fill('Fire SRT')
            await dialog.getByRole('button', { name: 'SRT', exact: true }).click()
            await dialog.getByRole('textbox', { name: 'Signal address', exact: true }).fill(plan.srt_url)
            await dialog.locator('input[type="date"]').fill(plan.date)
            await dialog.locator('input[type="time"]').nth(0).fill(plan.srt_start)
            await dialog.locator('input[type="time"]').nth(1).fill(plan.srt_end)
            await page.screenshot({ path: path.join(root, 'captures/fire-live-form.jpg'), type: 'jpeg' })
            let saving = page.waitForResponse(
                (response) => response.url().endsWith('/api/schedule/1') && response.request().method() === 'POST',
            )
            await dialog.getByRole('button', { name: 'Save and schedule', exact: true }).click()
            await page.screenshot({ path: path.join(root, 'captures/fire-live-submitted.jpg'), type: 'jpeg' })
            let response = await saving
            expect(response.ok(), await response.text()).toBeTruthy()
            const live = await response.json()
            await expect(dialog).toBeHidden()

            await page.getByRole('button', { name: 'Schedule graphics', exact: true }).first().click()
            dialog = page.getByRole('dialog')
            await dialog.getByLabel('Graphics template', { exact: true }).selectOption(String(plan.notice_id))
            await dialog.getByLabel('Name', { exact: true }).fill('Fire UI scheduled notice')
            await dialog.locator('input[type="date"]').fill(plan.date)
            await dialog.locator('input[type="time"]').nth(0).fill(plan.notice_start)
            await dialog.locator('input[type="time"]').nth(1).fill(plan.notice_end)
            saving = page.waitForResponse(
                (result) => result.url().endsWith('/api/schedule/1/graphics') && result.request().method() === 'POST',
            )
            await dialog.getByRole('button', { name: 'Schedule', exact: true }).click()
            response = await saving
            expect(response.ok(), await response.text()).toBeTruthy()
            const graphic = await response.json()
            await expect(dialog).toBeHidden()
            fs.writeFileSync(
                path.join(root, 'ui-ready.json'),
                JSON.stringify({ live_event: live.id, graphics_event: graphic.id }),
            )
            await page.screenshot({ path: path.join(root, 'captures/fire-scheduler.jpg'), type: 'jpeg', quality: 88 })
        }

        async function until(second) {
            const delay = plan.epoch * 1000 + second * 1000 - Date.now()
            if (delay < -5000) {
                throw new Error(`Missed browser operation at ${second}s`)
            }

            if (delay > 0) {
                await page.waitForTimeout(delay)
            }
        }

        await until(50)
        await page.goto('/graphics/templates?channel=1')
        const toggle = page.getByRole('switch', { name: 'Fire logo', exact: true })
        await expect(toggle).toBeChecked()
        await toggle.click()
        await expect(toggle).not.toBeChecked()
        receipts.push({ action: 'logo-hidden-in-browser', at: Date.now() / 1000 - plan.epoch })
        await until(58)
        await toggle.click()
        await expect(toggle).toBeChecked()
        receipts.push({ action: 'logo-shown-in-browser', at: Date.now() / 1000 - plan.epoch })
        await page.screenshot({ path: path.join(root, 'captures/fire-models.jpg'), type: 'jpeg', quality: 88 })

        await until(130)
        await page.goto(`/graphics/onair?channel=1&template=${plan.gc_id}`)
        await page.getByLabel('{name}', { exact: true }).fill('FIRE TEST')
        await page.getByLabel('{role}', { exact: true }).fill('SRT live')
        saving = page.waitForResponse(
            (result) => result.url().endsWith('/api/graphics/1/take') && result.request().method() === 'POST',
        )
        await page.getByRole('button', { name: /^put on air$/i }).click()
        response = await saving
        expect(response.ok(), await response.text()).toBeTruthy()
        receipts.push({
            action: 'gc-taken-in-browser',
            at: Date.now() / 1000 - plan.epoch,
            take: await response.json(),
        })
        await until(145)
        await page.evaluate(() => localStorage.setItem('language', 'pt-br'))
        await page.reload()
        await waitForProgram(page)
        await page.screenshot({ path: path.join(root, 'captures/fire-onair-pt-BR.jpg'), type: 'jpeg', quality: 88 })
        await page.setViewportSize({ width: 390, height: 844 })
        await waitForProgram(page)
        await page.screenshot({
            path: path.join(root, 'captures/fire-onair-mobile-pt-BR.jpg'),
            type: 'jpeg',
            quality: 88,
            fullPage: true,
        })
    } finally {
        fs.writeFileSync(path.join(root, 'ui-actions.json'), JSON.stringify({ receipts, errors }, null, 2))
        await browser.close()
    }
}

module.exports = { waitForProgram }

if (require.main === module) {
    main().catch((error) => {
        console.error(error)
        process.exitCode = 1
    })
}
