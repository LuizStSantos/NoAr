// Retake the README screenshots from a demo server built by build_demo.py.
//
//   node qa/screenshots/capture.cjs [--base http://127.0.0.1:5757] [--only player,report]
//
// English interface, dark theme, 1440x900. The pictures go to docs/images/.
// Run it against a disposable server only: it opens dialogs but saves nothing.
const path = require('node:path')
const { chromium } = require('playwright')

const args = process.argv.slice(2)
const option = (name, fallback) => {
    const index = args.indexOf(`--${name}`)
    return index >= 0 ? args[index + 1] : fallback
}
const base = option('base', 'http://127.0.0.1:5757')
const password = option('password', 'Example-2026')
const only = option('only', '').split(',').filter(Boolean)
const out = path.join(__dirname, '..', '..', 'docs', 'images')

const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms))

async function login(page) {
    await page.goto(`${base}/login`)
    await page.locator('input').first().fill('admin')
    await page.locator('input[type=password]').fill(password)
    await page.locator('button[type=submit]').click()
    await page.waitForFunction(() => !location.pathname.startsWith('/login'), null, { timeout: 15000 })
}

async function open(page, route, settle = 2500) {
    await page.goto(`${base}${route}`)
    await page.waitForLoadState('networkidle').catch(() => {})
    await pause(settle)
}

async function shot(page, name) {
    // The development server's Vue DevTools button is not part of NoAr.
    await page.addStyleTag({ content: '#__vue-devtools-container__ { display: none !important; }' })
    await page.mouse.move(1439, 899)
    await pause(400)
    await page.screenshot({ path: path.join(out, `${name}.png`) })
    console.log(`docs/images/${name}.png`)
}

const screens = {
    async login(page) {
        await page.context().clearCookies()
        await page.evaluate(() => {
            localStorage.removeItem('token')
            localStorage.removeItem('refresh')
        })
        await open(page, '/login')
        await shot(page, 'login')
        await login(page)
    },

    async player(page) {
        await open(page, '/player', 6000)
        await shot(page, 'player')
    },

    async scheduler(page) {
        await open(page, '/schedule', 3000)
        await shot(page, 'scheduler')
    },

    async storage(page) {
        await open(page, '/media', 2000)
        await page.getByText('Series', { exact: true }).first().click()
        await pause(800)
        await page.getByText('Kitchen Stories', { exact: true }).first().click()
        await pause(2000)
        await shot(page, 'storage')
    },

    async builder(page) {
        await open(page, '/playlist', 3000)
        await page.getByRole('button', { name: 'Open all' }).click()
        await pause(1500)
        // Two clips of the program on air, with the selection bar showing.
        const boxes = page.locator('main input[type=checkbox]:visible')
        await boxes.nth(1).check()
        await boxes.nth(2).check()
        await pause(1000)
        await shot(page, 'builder')
    },

    async 'program-folder'(page) {
        await open(page, '/playlist', 3000)
        const library = page.getByRole('complementary', { name: 'Library' })
        await library.getByText('Series', { exact: true }).click()
        await pause(1200)
        await library
            .getByRole('button', { name: /Kitchen Stories/ })
            .last()
            .click()
        await pause(1500)
        await shot(page, 'program-folder')
    },

    async 'program-editor'(page) {
        await open(page, '/media', 2000)
        await page.getByText('News', { exact: true }).first().click()
        await pause(1500)
        await page
            .getByRole('button', { name: /Morning News\.program/ })
            .first()
            .click()
        await pause(2500)
        await shot(page, 'program-editor')
    },

    async marks(page) {
        await open(page, '/media', 2000)
        await page.getByText('Documentaries', { exact: true }).first().click()
        await pause(800)
        await page.getByText('Oceans (2024)', { exact: true }).first().click()
        await pause(1500)
        await page.getByTitle('Trim, logo, breaks and L-shape in this video').first().click()
        await pause(1500)
        await page.locator('button', { hasText: 'SCTE-35 breaks' }).first().click()
        await pause(1500)
        // Inside the first break, where the timeline says so.
        await page.evaluate(() => {
            const video = document.querySelector('dialog video, [role=dialog] video, video')
            if (video) video.currentTime = 610
        })
        await pause(2500)
        await shot(page, 'marks')
    },

    async graphics(page) {
        await open(page, '/graphics/live', 4000)
        await shot(page, 'graphics')
    },

    async report(page) {
        await open(page, '/report', 4000)
        // What already aired, with actual times, and what is still to come.
        await page.getByRole('button', { name: 'Go to now' }).click()
        await pause(1500)
        await shot(page, 'report')
    },

    async settings(page) {
        await open(page, '/configure/output', 3000)
        await shot(page, 'settings')
    },
}

;(async () => {
    const browser = await chromium.launch()
    const context = await browser.newContext({
        viewport: { width: 1440, height: 900 },
        colorScheme: 'dark',
        locale: 'en-US',
        timezoneId: 'America/Sao_Paulo',
    })
    await context.addInitScript(() => {
        localStorage.setItem('language', 'en')
        localStorage.setItem('theme', 'dark')
    })
    const page = await context.newPage()
    await login(page)

    let failed = false
    for (const [name, take] of Object.entries(screens)) {
        if (only.length && !only.includes(name)) continue
        try {
            await take(page)
        } catch (error) {
            failed = true
            console.error(`${name}: ${error.message.split('\n')[0]}`)
            await page.screenshot({ path: path.join(out, `${name}.failed.png`) }).catch(() => {})
        }
    }
    await browser.close()
    process.exit(failed ? 1 : 0)
})()
