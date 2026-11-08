import { dirname, join } from 'node:path'
import { test, expect, type Page } from '@playwright/test'

test.skip(process.env.NOAR_E2E_WRITES !== '1', 'Requires runner-owned disposable staging.')
test.use({ storageState: process.env.NOAR_E2E_STATE_PATH })

async function headers(page: Page) {
    return { Authorization: `Bearer ${await page.evaluate(() => localStorage.getItem('token'))}` }
}

test('Composer saves scrolling data fields, previews both directions, and edits them on a phone', async ({ page }) => {
    test.setTimeout(90000)
    page.setDefaultTimeout(7000)
    await page.goto('/graphics/composer?channel=1')
    const auth = await headers(page)
    const createdSource = await page.request.post('/api/graphics/1/sources', {
        headers: auth,
        data: { name: 'Scroll market data', kind: 'csv', interval: 30, document: 'symbol,price\nUSD,5.10\nEUR,6.20' },
    })
    expect(createdSource.ok()).toBeTruthy()
    const source = await createdSource.json()
    let templateId: number | undefined

    try {
        await page.reload()
        await page.getByRole('button', { name: 'Add layer', exact: true }).click()
        await page
            .getByRole('group', { name: 'Layer type', exact: true })
            .getByRole('button', { name: 'Text', exact: true })
            .click()
        const set = async (name: string, value: string) => {
            await expect(page.getByLabel(name, { exact: true })).toBeVisible()
            await page.getByLabel(name, { exact: true }).fill(value)
            await page.getByLabel(name, { exact: true }).press('Tab')
        }
        await set('Text', 'Market report: {symbol} {price} · São Paulo · Headlines for your channel')
        await set('Width', '90')
        await set('Y', '80')
        await set('Fill (empty = transparent)', '#102030')
        await page.getByRole('combobox', { name: 'Text movement', exact: true }).selectOption('left')
        await set('Speed (output px/s)', '80')
        await set('Repeat gap (output px)', '40')
        await page.getByRole('button', { name: 'Undo', exact: true }).first().click()
        await expect(page.getByLabel('Repeat gap (output px)', { exact: true })).toHaveValue('80')
        await set('Repeat gap (output px)', '40')
        const position = async (time: string) => {
            await page.getByLabel('Preview time', { exact: true }).fill(time)
            const text = page.locator('[data-scroll-text]').first()
            await expect(text).toBeAttached()

            return text.evaluate((element) => new DOMMatrix(getComputedStyle(element).transform).m41)
        }
        const left1 = await position('1')
        const left2 = await position('1.5')
        expect(left2).toBeLessThan(left1)
        await page.getByRole('button', { name: 'Back to editing', exact: true }).click()
        await page.getByRole('combobox', { name: 'Text movement', exact: true }).selectOption('right')
        const right1 = await position('1')
        const right2 = await position('1.5')
        expect(right2).toBeGreaterThan(right1)
        await page.getByRole('button', { name: 'Back to editing', exact: true }).click()
        await page.getByRole('combobox', { name: 'Text movement', exact: true }).selectOption('left')
        await set('Template name', 'Scrolling market report')
        await page.getByRole('button', { name: 'Save template', exact: true }).click()
        for (const field of ['symbol', 'price']) {
            await page
                .getByRole('combobox', { name: `Source for ${field}`, exact: true })
                .selectOption(String(source.id))
            await page.getByRole('combobox', { name: `Column for ${field}`, exact: true }).selectOption(field)
        }
        const saved = page.waitForResponse(
            (response) =>
                response.url().endsWith('/api/graphics/1/templates') && response.request().method() === 'POST',
        )
        await page.getByRole('button', { name: 'Save', exact: true }).click()
        const response = await saved
        expect(response.ok()).toBeTruthy()
        const template = await response.json()
        templateId = template.id
        await expect(page).toHaveURL(new RegExp(`template=${template.id}(?:&|$)`))
        expect(template.layers[0].scroll).toEqual({ direction: 'left', speed: 80, gap: 40 })
        expect(template.layers[0].data_fields.price).toMatchObject({ source_id: source.id, column: 'price', row: 0 })
        await page.reload()
        await expect(page.getByRole('combobox', { name: 'Text movement', exact: true })).toHaveValue('left')
        await expect(page.getByLabel('Speed (output px/s)', { exact: true })).toHaveValue('80')
        await expect(page.locator('[data-scroll-text]').first()).toContainText('USD 5.10')
        await page.evaluate(() => localStorage.setItem('theme', 'dark'))
        await page.reload()
        await page.evaluate(() => localStorage.setItem('language', 'pt-br'))
        await page.reload()
        await expect(page.getByRole('combobox', { name: 'Movimento do texto', exact: true })).toHaveValue('left')
        await expect(page.locator('html')).toHaveAttribute('data-theme', 'ffp-dark')
        await page.getByRole('group', { name: 'Texto em movimento', exact: true }).scrollIntoViewIfNeeded()
        await page.evaluate(() => window.scrollTo(0, 0))
        await page.screenshot({
            path: join(dirname(process.env.NOAR_E2E_STATE_PATH!), 'scroll-composer-pt-BR.jpg'),
            type: 'jpeg',
            quality: 86,
            fullPage: true,
        })
        await page.setViewportSize({ width: 390, height: 844 })
        expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true)
        await page.getByRole('combobox', { name: 'Movimento do texto', exact: true }).selectOption('right')
        await expect(page.getByLabel('Velocidade (px/s da saída)', { exact: true })).toHaveValue('80')
        await page.getByRole('group', { name: 'Texto em movimento', exact: true }).scrollIntoViewIfNeeded()
        await page.evaluate(() => window.scrollTo(0, 0))
        await page.screenshot({
            path: join(dirname(process.env.NOAR_E2E_STATE_PATH!), 'scroll-composer-phone-pt-BR.jpg'),
            type: 'jpeg',
            quality: 86,
            fullPage: true,
        })
        await page.getByRole('combobox', { name: 'Movimento do texto', exact: true }).selectOption('left')
        await page.evaluate(() => localStorage.setItem('language', 'en'))
        await page.reload()
        await page.setViewportSize({ width: 1440, height: 1000 })
        await page.goto(`/graphics/onair?channel=1&template=${template.id}`)
        await page.getByRole('combobox', { name: 'Record from Scroll market data', exact: true }).selectOption('1')
        await expect(page.locator('[data-scroll-text]').first()).toContainText('EUR 6.20')
        const before = await page.locator('[data-scroll-text]').first().getAttribute('style')
        await expect.poll(() => page.locator('[data-scroll-text]').first().getAttribute('style')).not.toBe(before)
        await page.getByLabel('{price}', { exact: true }).fill('MANUAL')
        await expect(page.locator('[data-scroll-text]').first()).toContainText('EUR MANUAL')
        const invalid = await page.request.put(`/api/graphics/1/templates/${template.id}`, {
            headers: auth,
            data: {
                ...template,
                layers: [{ ...template.layers[0], scroll: { direction: 'left', speed: 0, gap: 40 } }],
            },
        })
        expect(invalid.status()).toBe(400)
    } finally {
        if (templateId) {
            expect(
                (await page.request.delete(`/api/graphics/1/templates/${templateId}`, { headers: auth })).ok(),
            ).toBeTruthy()
        }

        expect((await page.request.delete(`/api/graphics/1/sources/${source.id}`, { headers: auth })).ok()).toBeTruthy()
    }
})
