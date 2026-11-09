import { test, expect } from '@playwright/test'

test.skip(process.env.NOAR_E2E_WRITES !== '1', 'Requires runner-owned disposable staging.')
test.use({ storageState: process.env.NOAR_E2E_STATE_PATH })

test('converted display names follow the locale while saves retain raw defaults and custom names', async ({ page }) => {
    test.setTimeout(90_000)
    await page.goto('/graphics/templates?channel=1')
    const token = await page.evaluate(() => localStorage.getItem('token'))
    const headers = { Authorization: `Bearer ${token}` }
    const suffix = ` (${Date.now()})`
    const created: number[] = []
    const image = {
        id: 'image',
        name: 'Image',
        kind: 'image',
        visible: true,
        x: 0,
        y: 0,
        width: 40,
        height: 20,
        image: '00-assets/logo.png',
        animation_in: 'none',
        animation_out: 'none',
    }
    const picture = {
        ...image,
        id: 'picture',
        name: 'Picture',
        kind: 'video',
        image: null,
        width: 75,
        height: 75,
    }

    async function create(name: string, layers: object[], group = 'Channel graphics') {
        const response = await page.request.post('/api/graphics/1/templates', {
            headers,
            data: { name, group, duration: 0, always_on: false, layers },
        })
        expect(response.ok(), await response.text()).toBeTruthy()
        const template = await response.json()
        created.push(template.id)

        return template
    }

    try {
        const lshape = await create('lshape' + suffix, [image, picture])
        await create('logo' + suffix, [image])
        await create('ticker' + suffix, [])
        const custom = await create('Custom Image' + suffix, [
            { ...image, id: 'custom-image' },
            { ...picture, id: 'custom-picture' },
        ])
        const customRoleName = `lshape (${Date.now() + 1})`
        await create(customRoleName, [image], 'Custom group')

        for (const locale of [
            {
                code: 'pt-br',
                names: ['L-shape do canal', 'Logo do canal', 'Letreiro'],
                template: 'Nome do modelo',
                layer: 'Nome da camada',
                image: 'Imagem',
                picture: 'Vídeo no canto',
                select: 'Selecionar a camada',
            },
            {
                code: 'en',
                names: ['Channel L-shape', 'Channel logo', 'Ticker'],
                template: 'Template name',
                layer: 'Layer name',
                image: 'Image',
                picture: 'Video in the corner',
                select: 'Select layer',
            },
        ]) {
            await page.evaluate((language) => localStorage.setItem('language', language), locale.code)
            await page.goto('/graphics/templates?channel=1')

            for (const name of locale.names) {
                await expect(page.getByRole('heading', { name: name + suffix, exact: true })).toBeVisible()
            }

            await expect(page.getByRole('heading', { name: custom.name, exact: true })).toBeVisible()
            await expect(page.getByRole('heading', { name: customRoleName, exact: true })).toBeVisible()
            await page.goto(`/graphics/onair?channel=1&template=${lshape.id}`)
            await expect(page.locator('figcaption').first()).toContainText(locale.names[0]! + suffix)
            await page.goto(`/graphics/composer?channel=1&template=${lshape.id}`)
            await expect(page.getByLabel(locale.template, { exact: true })).toHaveValue(locale.names[0]! + suffix)
            await expect(page.getByLabel(locale.layer, { exact: true })).toHaveValue(locale.image)
            await page.getByRole('button', { name: `${locale.select} ${locale.picture}`, exact: true }).click()
            await expect(page.getByLabel(locale.layer, { exact: true })).toHaveValue(locale.picture)

            await page.goto(`/graphics/composer?channel=1&template=${custom.id}`)
            await expect(page.getByLabel(locale.template, { exact: true })).toHaveValue(custom.name)
            await expect(page.getByLabel(locale.layer, { exact: true })).toHaveValue('Image')
            await page.getByRole('button', { name: `${locale.select} Picture`, exact: true }).click()
            await expect(page.getByLabel(locale.layer, { exact: true })).toHaveValue('Picture')
        }

        await page.evaluate(() => localStorage.setItem('language', 'pt-br'))
        await page.goto(`/graphics/composer?channel=1&template=${lshape.id}`)
        await page.getByLabel('X', { exact: true }).fill('1')
        await page.getByLabel('X', { exact: true }).press('Tab')
        await page.getByRole('button', { name: 'Salvar modelo', exact: true }).click()
        const saved = page.waitForResponse(
            (response) => response.url().endsWith(`/templates/${lshape.id}`) && response.request().method() === 'PUT',
        )
        await page.getByRole('button', { name: 'Salvar', exact: true }).click()
        expect((await saved).ok()).toBeTruthy()
        await expect(page.getByRole('button', { name: 'Salvar', exact: true })).toHaveCount(0)
        const stored = await page.request.get(`/api/graphics/1/templates/${lshape.id}`, { headers })
        const raw = await stored.json()
        expect(raw.name).toBe('lshape' + suffix)
        expect(raw.layers.map((layer: { id: string; name: string }) => [layer.id, layer.name])).toEqual([
            ['image', 'Image'],
            ['picture', 'Picture'],
        ])
        expect(raw.layers[0].x).toBe(1)

        await page.getByLabel('Nome do modelo', { exact: true }).fill('My custom model' + suffix)
        await page.getByLabel('Nome da camada', { exact: true }).fill('My custom layer')
        await page.getByLabel('Nome da camada', { exact: true }).press('Tab')
        await page.getByRole('button', { name: 'Salvar modelo', exact: true }).click()
        const renamed = page.waitForResponse(
            (response) => response.url().endsWith(`/templates/${lshape.id}`) && response.request().method() === 'PUT',
        )
        await page.getByRole('button', { name: 'Salvar', exact: true }).click()
        expect((await renamed).ok()).toBeTruthy()
        await expect(page.getByRole('button', { name: 'Salvar', exact: true })).toHaveCount(0)
        await page.evaluate(() => localStorage.setItem('language', 'en'))
        await page.reload()
        await expect(page.getByLabel('Template name', { exact: true })).toHaveValue('My custom model' + suffix)
        await expect(page.getByLabel('Layer name', { exact: true })).toHaveValue('My custom layer')
    } finally {
        for (const id of created) {
            expect((await page.request.delete(`/api/graphics/1/templates/${id}`, { headers })).ok()).toBeTruthy()
        }

        await page.evaluate(() => localStorage.setItem('language', 'en'))
    }
})
