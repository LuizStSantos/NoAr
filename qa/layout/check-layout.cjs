#!/usr/bin/env node
/**
 * Layout check for an isolated NoAr staging instance, at desktop or phone and
 * tablet widths. For each page it reports what a person would see go wrong:
 * the page scrolling sideways, elements past the edge of the screen, short
 * words split across lines, and the main menu on more than one line (desktop)
 * or the menu button missing (phone, tablet). It saves a screenshot of each
 * page and a JSON report.
 *
 *   NOAR_E2E_STAGING=1 NOAR_LAYOUT_USER=... NOAR_LAYOUT_PASSWORD=... \
 *     node qa/layout/check-layout.cjs --profile mobile --out /tmp/layout
 *
 * Options: --profile desktop|mobile, --widths 360,390, --routes /,/player,
 * --base http://127.0.0.1:8787, --out DIR. It refuses to run without
 * NOAR_E2E_STAGING=1, and against anything but a loopback host unless
 * NOAR_E2E_ALLOW_REMOTE=1. It only reads pages: no clicks that save, no writes.
 */
const fs = require('fs')
const path = require('path')

const PROFILES = {
    desktop: [1280, 1366, 1440, 1920],
    mobile: [320, 360, 390, 820, 1024],
}

const ROUTES = [
    '/',
    '/player',
    '/playlist',
    '/schedule',
    '/graphics/live',
    '/graphics/logo',
    '/graphics/lshape',
    '/graphics/subtitles',
    '/graphics/scenes',
    '/graphics/api',
    '/media',
    '/report',
    '/configure/general',
    '/configure/logging',
    '/configure/output',
    '/configure/ingest',
    '/configure/playlist',
    '/configure/audio',
    '/configure/recording',
    '/configure/breaks',
    '/configure/alerts',
    '/configure/advanced',
    '/configure/users',
]

/** Below this width the main menu is behind the menu button (HeaderMenu, Tailwind `xl`). */
const MENU_BUTTON_BELOW = 1280

function option(name, fallback) {
    const index = process.argv.indexOf(`--${name}`)

    return index > 0 && process.argv[index + 1] ? process.argv[index + 1] : fallback
}

function refuse(message) {
    console.error(`check-layout: ${message}`)
    process.exit(2)
}

if (process.env.NOAR_E2E_STAGING !== '1') {
    refuse('set NOAR_E2E_STAGING=1, and point it at a disposable staging NoAr only')
}

const base = option('base', process.env.NOAR_LAYOUT_BASE_URL || 'http://127.0.0.1:8787').replace(/\/$/, '')
const host = new URL(base).hostname

if (!['127.0.0.1', 'localhost', '::1', '[::1]'].includes(host) && process.env.NOAR_E2E_ALLOW_REMOTE !== '1') {
    refuse(`${host} is not a loopback host; set NOAR_E2E_ALLOW_REMOTE=1 only for a reviewed disposable staging host`)
}

const profile = option('profile', 'desktop')

if (!PROFILES[profile]) {
    refuse(`unknown profile ${profile}: use desktop or mobile`)
}

const widths = option('widths', PROFILES[profile].join(','))
    .split(',')
    .map(Number)
    .filter((width) => width > 0)
const routes = option('routes', ROUTES.join(',')).split(',').filter(Boolean)
const out = option('out', path.join(process.cwd(), 'qa/layout/results', profile))

async function login(page) {
    const user = process.env.NOAR_LAYOUT_USER
    const password = process.env.NOAR_LAYOUT_PASSWORD

    if (!user || !password) {
        refuse('set NOAR_LAYOUT_USER and NOAR_LAYOUT_PASSWORD (a test-only staging account)')
    }

    await page.goto(`${base}/login`)
    await page.fill('input[autocomplete=username]', user)
    await page.fill('input[autocomplete=current-password]', password)
    await page.keyboard.press('Enter')
    await page.waitForURL((url) => !url.pathname.startsWith('/login'), { timeout: 20000 })
}

/** What goes wrong on the page as it is now. Runs in the browser. */
function inspect(menuButtonBelow) {
    const viewport = document.documentElement.clientWidth
    const label = (element) =>
        `${element.tagName.toLowerCase()}.${(element.getAttribute('class') || '').split(' ').slice(0, 3).join('.')}`
    const visible = (element) => {
        const style = getComputedStyle(element)

        return style.display !== 'none' && style.visibility !== 'hidden' && element.getBoundingClientRect().width > 0
    }
    const inScroller = (element) => {
        for (let parent = element.parentElement; parent; parent = parent.parentElement) {
            if (getComputedStyle(parent).overflowX !== 'visible') {
                return true
            }
        }

        return false
    }

    const outside = []

    for (const element of document.querySelectorAll('body *')) {
        if (!visible(element) || inScroller(element)) {
            continue
        }

        const box = element.getBoundingClientRect()

        if (box.right > viewport + 1 || box.left < -1) {
            outside.push(`${label(element)} "${(element.innerText || '').trim().slice(0, 40)}"`)
        }
    }

    // A text with more lines than words had a word split in two.
    const split = []
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT)

    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
        const text = node.textContent.trim()

        if (!text || text.length > 60 || !node.parentElement || !visible(node.parentElement)) {
            continue
        }

        const range = document.createRange()
        range.selectNodeContents(node)
        const lines = new Set(
            [...range.getClientRects()].filter((rect) => rect.width > 0).map((rect) => Math.round(rect.top)),
        )

        if (lines.size > text.split(/\s+/).length && !/[-/]/.test(text)) {
            split.push(`"${text.slice(0, 30)}" in ${label(node.parentElement)}`)
        }
    }

    const nav = document.querySelector('#main-nav')
    const links = nav ? [...nav.querySelectorAll('a')].filter(visible) : []
    const menuButton = document.querySelector('button[aria-controls="main-nav"]')

    return {
        pageWidth: document.documentElement.scrollWidth,
        viewport,
        outside: [...new Set(outside)].slice(0, 10),
        split: [...new Set(split)].slice(0, 10),
        menuRows: new Set(links.map((link) => Math.round(link.getBoundingClientRect().top))).size,
        menuButton: Boolean(menuButton && visible(menuButton)),
        expectMenuButton: viewport < menuButtonBelow,
    }
}

;(async () => {
    const { chromium, devices } = require('playwright')
    const browser = await chromium.launch()
    const report = { base, profile, widths, pages: [] }

    fs.mkdirSync(out, { recursive: true })

    for (const width of widths) {
        const phone = width < 500
        const context = await browser.newContext(
            phone ? { ...devices['Pixel 7'], viewport: { width, height: 800 } } : { viewport: { width, height: 900 } },
        )
        const page = await context.newPage()
        const errors = []

        page.on('pageerror', (error) => errors.push(String(error).slice(0, 200)))
        await login(page)

        for (const route of routes) {
            errors.length = 0
            await page.goto(`${base}${route}`)
            await page.waitForTimeout(2500)

            const found = await page.evaluate(inspect, MENU_BUTTON_BELOW)
            const problems = []

            if (found.pageWidth > found.viewport + 1)
                problems.push(`page scrolls sideways (${found.pageWidth}/${found.viewport} px)`)
            if (found.outside.length) problems.push(`past the edge: ${found.outside.join('; ')}`)
            if (found.split.length) problems.push(`words split: ${found.split.join('; ')}`)
            if (!found.expectMenuButton && found.menuRows > 1) problems.push(`menu on ${found.menuRows} lines`)
            if (found.expectMenuButton && !found.menuButton) problems.push('menu button missing')
            if (!found.expectMenuButton && found.menuButton) problems.push('menu button shown on desktop')
            if (errors.length) problems.push(`page errors: ${errors.join(' | ')}`)

            const shot = path.join(out, `${width}${route.replace(/\//g, '_') || '_home'}.png`)
            await page.screenshot({ path: shot, fullPage: true })
            report.pages.push({ width, route, problems, screenshot: shot })
            console.log(
                `${problems.length ? 'FAIL' : 'ok  '} ${width} ${route}${problems.length ? `\n     ${problems.join('\n     ')}` : ''}`,
            )
        }

        await context.close()
    }

    await browser.close()
    fs.writeFileSync(path.join(out, 'report.json'), JSON.stringify(report, null, 2))

    const failed = report.pages.filter((entry) => entry.problems.length).length
    console.log(
        `${failed} of ${report.pages.length} page views with problems; report in ${path.join(out, 'report.json')}`,
    )
    process.exit(failed ? 1 : 0)
})().catch((error) => {
    console.error(error)
    process.exit(2)
})
