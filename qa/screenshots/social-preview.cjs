// Render the GitHub social preview (1280x640) from social-preview.html,
// using the Player screenshot of the README gallery. Upload the result in the
// repository's Settings › General › Social preview.
const path = require('node:path')
const { chromium } = require('../e2e/node_modules/@playwright/test')

const args = process.argv.slice(2)
function option(name, fallback) {
    const index = args.indexOf(`--${name}`)

    return index >= 0 ? args[index + 1] : fallback
}

const out = option('out', path.join(__dirname, '..', '..', 'docs', 'images', 'social-preview.png'))

async function main() {
    const browser = await chromium.launch({
        executablePath: option('chromium', process.env.NOAR_E2E_CHROMIUM),
        args: ['--no-sandbox', '--allow-file-access-from-files'],
    })

    try {
        const page = await browser.newPage({ viewport: { width: 1280, height: 640 }, deviceScaleFactor: 1 })
        const failed = []
        page.on('requestfailed', (request) => failed.push(request.url()))
        await page.goto(`file://${path.join(__dirname, 'social-preview.html')}`)
        // Load every declared face, so a missing font file fails instead of
        // falling back silently to another typeface.
        const fonts = await page.evaluate(async () => {
            await Promise.allSettled([...document.fonts].map((font) => font.load()))
            return [...document.fonts].map((font) => `${font.family} ${font.weight}: ${font.status}`)
        })
        const image = await page.locator('.shot img').evaluate((img) => img.complete && img.naturalWidth)

        if (failed.length || image !== 1440 || fonts.some((font) => !font.endsWith('loaded'))) {
            throw new Error(
                `Missing assets: ${failed.join(', ') || 'no failed request'}; screenshot width ${image}; ` +
                    `fonts ${fonts.join(', ')}. Run npm ci first so node_modules has the IBM Plex fonts.`,
            )
        }

        // GitHub's repo card template: what matters stays 80 px inside every edge.
        const outside = await page.evaluate(() =>
            [...document.querySelectorAll('.brand, .tag, .desc, .chips, .foot, .shot, .onair')]
                .map((el) => [el.className, el.getBoundingClientRect()])
                .filter(([, box]) => box.left < 80 || box.top < 80 || box.right > 1200 || box.bottom > 560)
                .map(
                    ([name, box]) =>
                        `${name} at x ${Math.round(box.left)}-${Math.round(box.right)}, y ${Math.round(box.top)}-${Math.round(box.bottom)}`,
                ),
        )

        if (outside.length) {
            throw new Error(`Outside GitHub's safe area (x 80-1200, y 80-560): ${outside.join('; ')}`)
        }

        await page.screenshot({ path: out })
        console.log(`PASS social preview: ${path.relative(process.cwd(), out)} (1280x640, inside the safe area)`)
    } finally {
        await browser.close()
    }
}

main().catch((error) => {
    console.error(error.message)
    process.exit(1)
})
