// Capture actual README screens from the owned Example TV demo in dark mode.
const fs = require('node:fs')
const path = require('node:path')
const crypto = require('node:crypto')
const { execFileSync } = require('node:child_process')
const { chromium, expect } = require('../e2e/node_modules/@playwright/test')

const args = process.argv.slice(2)
function option(name, fallback) {
    const index = args.indexOf(`--${name}`)

    return index >= 0 ? args[index + 1] : fallback
}

const base = option('base', 'http://127.0.0.1:5757')
const apiBase = option('api', 'http://127.0.0.1:8787')
const passwordFile = option('password-file', '')
const password = passwordFile ? fs.readFileSync(passwordFile, 'utf8').trim() : option('password', 'Example-2026')
const manifestFile = option('manifest', '')
const only = option('only', '').split(',').filter(Boolean)
const out = path.join(__dirname, '..', '..', 'docs', 'images')
const evidenceFile = option('evidence', path.join(out, 'readme-captures.json'))
const target = new URL(base)
const apiTarget = new URL(apiBase)

if (
    process.env.NOAR_E2E_STAGING !== '1' ||
    process.env.NOAR_E2E_DISPOSABLE !== '1' ||
    !['127.0.0.1', 'localhost', '[::1]'].includes(target.hostname) ||
    !['http:', 'https:'].includes(target.protocol) ||
    !['127.0.0.1', 'localhost', '[::1]'].includes(apiTarget.hostname) ||
    !['http:', 'https:'].includes(apiTarget.protocol)
) {
    throw new Error('Explicit loopback disposable staging opt-in is required')
}

if (!manifestFile || !fs.existsSync(path.join(path.dirname(manifestFile), '.qa-readme-owned'))) {
    throw new Error('Use the manifest of an owned README demo installation')
}

const manifest = JSON.parse(fs.readFileSync(manifestFile, 'utf8'))
const evidence = {
    theme: 'dark',
    language: 'en-US',
    viewport: { width: 1440, height: 1000 },
    captures: [],
    video_checks: [],
    page_errors: [],
    application_revision: execFileSync('git', ['rev-parse', 'HEAD'], {
        cwd: path.join(__dirname, '..', '..'),
        encoding: 'utf8',
    }).trim(),
}
// Record working-source hashes as captures can precede their final commit.
evidence.application_source_sha256 = Object.fromEntries(
    [
        'frontend/src/components/graphics/TemplatePreview.vue',
        'frontend/src/components/graphics/GraphicsLibrary.vue',
        'frontend/src/components/graphics/GraphicsOnAir.vue',
        'frontend/src/components/graphics/composer/ComposerCanvas.vue',
    ].map((file) => [
        file,
        crypto
            .createHash('sha256')
            .update(fs.readFileSync(path.join(__dirname, '..', '..', file)))
            .digest('hex'),
    ]),
)
const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms))

async function login(page) {
    await page.goto(`${base}/login`)
    await page.locator('input[name=username]').fill('admin')
    await page.locator('input[type=password]').fill(password)
    await page.getByRole('button', { name: 'Sign in', exact: true }).click()
    await expect(page).not.toHaveURL(/\/login(?:\?|$)/)
}

async function open(page, route, heading) {
    await page.goto(`${base}${route}`)
    await expect(page.locator('html')).toHaveAttribute('data-theme', 'ffp-dark')

    if (heading) {
        await expect(page.getByRole('heading', { name: heading, exact: true }).first()).toBeVisible()
    }

    await page
        .locator('.loading-spinner:visible')
        .first()
        .waitFor({ state: 'hidden', timeout: 15000 })
        .catch(() => {})
    await pause(700)
}

async function shot(page, name) {
    await expect(page.locator('html')).toHaveAttribute('data-theme', 'ffp-dark')
    await page.evaluate(() => window.scrollTo(0, 0))
    await page.mouse.move(1439, 999)
    await pause(350)
    const geometry = await page.evaluate(() => ({
        theme: document.documentElement.getAttribute('data-theme'),
        viewport: innerWidth,
        viewport_height: innerHeight,
        device_scale: devicePixelRatio,
        scroll: document.documentElement.scrollWidth,
        prefers_dark: matchMedia('(prefers-color-scheme: dark)').matches,
    }))
    expect(geometry.scroll, `${name}: document overflow`).toBeLessThanOrEqual(geometry.viewport)
    let previewBackground

    if (['graphics', 'composer', 'onair'].includes(name)) {
        previewBackground = await page
            .locator('.graphics-preview')
            .first()
            .evaluate((element) => {
                const style = getComputedStyle(element)

                return { color: style.backgroundColor, image: style.backgroundImage, size: style.backgroundSize }
            })
        expect(previewBackground.color).toBe('rgb(36, 39, 44)')
        expect(previewBackground.image).toContain('conic-gradient')
        expect(previewBackground.size).toContain('20px 20px')

        if (name === 'composer') {
            expect(previewBackground.image.match(/linear-gradient/g)).toHaveLength(2)
        } else {
            expect(previewBackground.image).not.toContain('linear-gradient')
        }
    }

    await page.screenshot({
        path: path.join(out, `${name}.png`),
        animations: 'disabled',
        fullPage: false,
    })
    const contents = fs.readFileSync(path.join(out, `${name}.png`))
    expect(contents.readUInt32BE(16), `${name}: image width`).toBe(evidence.viewport.width)
    expect(contents.readUInt32BE(20), `${name}: image height`).toBe(evidence.viewport.height)
    evidence.captures.push({
        file: `${name}.png`,
        width: contents.readUInt32BE(16),
        height: contents.readUInt32BE(20),
        sha256: crypto.createHash('sha256').update(contents).digest('hex'),
        ...(previewBackground ? { preview_background: previewBackground } : {}),
        ...geometry,
    })
    console.log(`PASS dark capture: docs/images/${name}.png`)
}

async function waitForActualVideo(page, screen) {
    const video = page.locator('video').first()
    await expect(video).toBeVisible()
    await expect.poll(() => video.evaluate((element) => element.videoWidth), { timeout: 25000 }).toBeGreaterThan(0)
    const first = await video.evaluate((element) => element.currentTime)
    await expect.poll(() => video.evaluate((element) => element.currentTime), { timeout: 10000 }).toBeGreaterThan(first)
    evidence.video_checks.push({
        screen,
        previous_time: first,
        ...(await video.evaluate((element) => ({
            width: element.videoWidth,
            height: element.videoHeight,
            time: element.currentTime,
        }))),
    })
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
        await open(page, '/player')
        await waitForActualVideo(page, 'player')
        await shot(page, 'player')
    },
    async scheduler(page) {
        await open(page, '/schedule', 'Scheduler')
        await expect(page.getByText('Sponsor window', { exact: true }).first()).toBeVisible()
        await shot(page, 'scheduler')
    },
    async storage(page) {
        await open(page, '/media?path=Series%2FKitchen%20Stories', 'Storage')
        await expect(page.getByText('Kitchen Stories S01E01.mp4', { exact: true }).first()).toBeVisible()
        await shot(page, 'storage')
    },
    async builder(page) {
        await open(page, '/playlist')
        const playlist = page.getByRole('region', { name: 'Playlist', exact: true })
        await playlist.getByRole('button', { name: 'Open all', exact: true }).click()
        const boxes = playlist.locator('input[type=checkbox]:visible')
        await boxes.nth(1).check()
        await boxes.nth(2).check()
        await shot(page, 'builder')
    },
    async 'program-folder'(page) {
        await open(page, '/playlist')
        const library = page.getByRole('complementary', { name: 'Library' })
        await library.getByText('Series', { exact: true }).click()
        await library
            .getByRole('button', { name: /Kitchen Stories/ })
            .last()
            .click()
        await expect(page.getByRole('dialog')).toBeVisible()
        await shot(page, 'program-folder')
    },
    async 'program-editor'(page) {
        await open(page, '/media?path=News', 'Storage')
        await page
            .getByRole('button', { name: /Morning News\.program/ })
            .first()
            .click()
        await expect(page.getByRole('dialog')).toBeVisible()
        await shot(page, 'program-editor')
    },
    async marks(page) {
        await open(page, '/media?path=Documentaries%2FOceans%20(2024)', 'Storage')
        await page
            .getByRole('button', { name: 'Trim, logo, breaks and L-shape in this video: Oceans.mp4', exact: true })
            .click()
        await expect(page.getByRole('dialog')).toBeVisible()
        await page.getByRole('radio', { name: /^SCTE-35 breaks/ }).click()
        await page.locator('[role=dialog] video').evaluate((video) => {
            video.currentTime = 610
        })
        await pause(1000)
        await shot(page, 'marks')
    },
    async graphics(page) {
        await open(page, '/graphics/templates', 'Templates')
        await expect(page.getByText('Presenter', { exact: true }).first()).toBeVisible()
        await expect(page.getByText('Market update', { exact: true }).first()).toBeVisible()
        await shot(page, 'graphics')
    },
    async composer(page) {
        await open(page, `/graphics/composer?template=${manifest.templates.presenter}`)
        await expect(page.getByLabel('Template name', { exact: true })).toHaveValue('Presenter')
        await page
            .getByRole('complementary', { name: 'Layers', exact: true })
            .getByText('Presenter Name', { exact: true })
            .click()
        await page.getByLabel('Safety margins', { exact: true }).check()
        await shot(page, 'composer')
    },
    async onair(page) {
        await open(page, '/graphics/onair', 'On air')
        await page
            .getByRole('button', { name: /Presenter/ })
            .last()
            .click()
        await page.getByLabel('{name}', { exact: true }).fill('Rita Souza')
        await page.getByLabel('{role}', { exact: true }).fill('Host · Example TV')
        await waitForActualVideo(page, 'onair')
        await shot(page, 'onair')
    },
    async 'data-sources'(page) {
        await open(page, '/graphics/sources', 'Data sources')
        await expect(page.getByText('Market snapshot', { exact: true })).toBeVisible()
        await expect(page.getByRole('cell', { name: 'USD', exact: true })).toBeVisible()
        await shot(page, 'data-sources')
    },
    async report(page) {
        await open(page, '/report', 'Report')
        await page.getByRole('button', { name: 'Go to now', exact: true }).click()
        await shot(page, 'report')
    },
    async settings(page) {
        await open(page, '/configure/output')
        await expect(page.getByText('HLS', { exact: true }).first()).toBeVisible()
        await shot(page, 'settings')
    },
}

async function main() {
    for (const name of only) {
        if (!Object.hasOwn(screens, name)) {
            throw new Error(`Unknown capture: ${name}`)
        }
    }

    const browser = await chromium.launch({
        executablePath: option('chromium', process.env.NOAR_E2E_CHROMIUM),
        args: ['--no-sandbox', '--autoplay-policy=no-user-gesture-required'],
    })
    const context = await browser.newContext({
        viewport: evidence.viewport,
        deviceScaleFactor: 1,
        colorScheme: 'dark',
        locale: evidence.language,
        timezoneId: 'America/Sao_Paulo',
    })
    await context.addInitScript(() => {
        localStorage.setItem('language', 'en')
        localStorage.setItem('theme', 'dark')
    })
    const page = await context.newPage()
    page.on('pageerror', (error) => evidence.page_errors.push(error.message))
    let failed = false

    try {
        await login(page)
        const token = await page.evaluate(() => localStorage.getItem('token'))
        const headers = { Authorization: `Bearer ${token}` }
        const response = await context.request.get(`${base}/api/channel/1`, { headers })
        expect(response.ok()).toBeTruthy()
        const channel = await response.json()
        expect(channel.name).toBe('Example TV')
        expect(path.resolve(channel.storage)).toBe(path.resolve(manifest.storage))

        for (const [name, take] of Object.entries(screens)) {
            if (only.length && !only.includes(name)) {
                continue
            }

            try {
                await take(page)
            } catch (error) {
                failed = true
                console.error(`${name}: ${error.message.split('\n')[0]}`)
                await page
                    .screenshot({ path: path.join(path.dirname(manifestFile), `${name}.failed.png`) })
                    .catch(() => {})
            }
        }

        const health = await context.request.get(new URL('/health', apiBase).href)
        evidence.health = await health.json()
        expect(evidence.health.status).toBe('ok')
        expect(evidence.page_errors).toEqual([])
        evidence.status = failed ? 'FAIL' : 'PASS'
        fs.writeFileSync(evidenceFile, JSON.stringify(evidence, null, 2) + '\n')
    } finally {
        await browser.close()
    }

    process.exit(failed ? 1 : 0)
}

main().catch((error) => {
    console.error(error)
    process.exit(1)
})
