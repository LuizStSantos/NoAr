import { dirname, join } from 'node:path'
import { test, expect } from '@playwright/test'

test.skip(process.env.NOAR_E2E_WRITES !== '1', 'Requires runner-owned disposable staging.')
test.use({ storageState: process.env.NOAR_E2E_STATE_PATH })

test('Models, On air and Composer share a neutral checkerboard on desktop and phone', async ({ page }) => {
    await page.addInitScript(() => {
        localStorage.setItem('theme', 'dark')
        localStorage.setItem('language', 'pt-br')
    })
    await page.goto('/graphics/templates?channel=1')
    const token = await page.evaluate(() => localStorage.getItem('token'))
    const response = await page.request.get('/api/graphics/1/templates', {
        headers: { Authorization: `Bearer ${token}` },
    })
    expect(response.ok()).toBeTruthy()
    const templates = await response.json()
    const template = templates.find((item: { layers: { kind: string }[] }) =>
        item.layers.some((layer) => layer.kind === 'image'),
    )
    expect(template).toBeTruthy()

    for (const viewport of [
        { width: 1440, height: 1000 },
        { width: 390, height: 844 },
    ]) {
        await page.setViewportSize(viewport)

        for (const screen of ['templates', 'onair', 'composer']) {
            await page.goto(`/graphics/${screen}?channel=1&template=${template.id}`)
            const preview = page.locator('.graphics-preview').first()
            await expect(preview).toBeVisible()
            const style = await preview.evaluate((element) => {
                const style = getComputedStyle(element)

                return { color: style.backgroundColor, image: style.backgroundImage, size: style.backgroundSize }
            })
            expect(style.color).toBe('rgb(36, 39, 44)')
            expect(style.image).toContain('conic-gradient')
            expect(style.size).toContain('20px 20px')

            if (screen === 'composer') {
                expect(style.image.match(/linear-gradient/g)).toHaveLength(2)
                expect(style.size).toContain('10% 10%')
                await page.getByLabel('Margens de segurança', { exact: true }).check()
                await expect(page.locator('.composer-safe-area')).toHaveCount(2)
            } else {
                expect(style.image).not.toContain('linear-gradient')
            }

            if (screen === 'onair') {
                await expect(page.locator('[data-testid="graphics-program-monitor"] .graphics-preview')).toHaveCount(0)
            }

            expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true)
            await page.evaluate(() => window.scrollTo(0, 0))
            await page.screenshot({
                path: join(
                    dirname(process.env.NOAR_E2E_STATE_PATH!),
                    `checkerboard-${screen}-${viewport.width}-pt-BR.jpg`,
                ),
                type: 'jpeg',
                quality: 86,
                fullPage: viewport.width === 390,
            })
        }
    }
})
