import { test, expect, type Page } from '@playwright/test'
import { reopenSaved } from './composer'

test.skip(process.env.NOAR_E2E_WRITES !== '1', 'Requires runner-owned disposable staging.')
test.use({ storageState: process.env.NOAR_E2E_STATE_PATH })

/** The names of the layer list, front first, as it shows them. */
function layerNames(page: Page) {
    return page.getByRole('complementary', { name: 'Layers', exact: true }).locator('li span.text-sm')
}

test('a layer is duplicated in front of itself, with its settings, and the copy is undone or saved', async ({
    page,
}) => {
    await page.addInitScript(() => localStorage.setItem('language', 'en'))
    await page.goto('/graphics/templates?channel=1')
    const token = await page.evaluate(() => localStorage.getItem('token'))
    const headers = { Authorization: `Bearer ${token}` }
    const created = await page.request.post('/api/graphics/1/templates', {
        headers,
        data: {
            name: `Duplicate layer ${Date.now()}`,
            group: 'Duplicate layer',
            duration: 8,
            always_on: false,
            during_lshape: 'wait',
            layers: [
                { id: 'picture', name: 'Picture', kind: 'video', visible: true, x: 5, y: 5, width: 60, height: 60 },
                {
                    id: 'band',
                    name: 'Band',
                    kind: 'box',
                    visible: true,
                    x: 0,
                    y: 80,
                    width: 100,
                    height: 12,
                    fill: '#123456',
                    radius: 2,
                },
                {
                    id: 'title',
                    name: 'Title',
                    kind: 'text',
                    visible: true,
                    x: 10,
                    y: 82,
                    width: 50,
                    height: 8,
                    text: '{name}',
                    font_size: 5,
                    bold: true,
                    color: '#ffcc00',
                },
            ],
        },
    })
    expect(created.ok(), await created.text()).toBeTruthy()
    const { id } = await created.json()

    try {
        await page.goto(`/graphics/composer?channel=1&template=${id}`)
        await expect(page.getByRole('button', { name: 'Save template', exact: true })).toBeEnabled()
        // The video layer shows under its kind's name.
        const video = 'Video in the corner'
        await expect(layerNames(page)).toHaveText(['Title', 'Band', video])

        // The template's one video layer cannot be copied.
        await expect(page.getByRole('button', { name: `Duplicate layer ${video}`, exact: true })).toBeDisabled()

        // From the list: the row's actions show on the selected layer, and
        // the copy comes in front of the band, selected.
        await page.getByRole('button', { name: 'Shape Band', exact: true }).click()
        await page.getByRole('button', { name: 'Duplicate layer Band', exact: true }).click()
        await expect(layerNames(page)).toHaveText(['Title', 'Band (copy)', 'Band', video])
        await expect(page.getByLabel('Layer name', { exact: true })).toHaveValue('Band (copy)')
        await expect(page.getByText('Layer “Band” duplicated.', { exact: false })).toBeVisible()
        // The preview draws it under the notice: the notice appearing as the
        // preview drew the new layer once left Chromium with no layout for the
        // whole preview column, while the editor's column was a query container.
        await expect(page.getByRole('button', { name: 'Select layer Band (copy)', exact: true })).toBeVisible()

        // Undo takes it away.
        await page.getByRole('button', { name: 'Undo', exact: true }).first().click()
        await expect(layerNames(page)).toHaveText(['Title', 'Band', video])

        // Ctrl+D copies the selected layer; typing in a field does not.
        await page.getByRole('button', { name: 'Select layer Title', exact: true }).click()
        await page.getByLabel('Layer name', { exact: true }).press('Control+d')
        await expect(layerNames(page)).toHaveText(['Title', 'Band', video])
        await page.getByRole('button', { name: 'Select layer Title', exact: true }).press('Control+d')
        await expect(layerNames(page)).toHaveText(['Title (copy)', 'Title', 'Band', video])

        // The properties panel duplicates the selected layer too: the copy of the copy.
        await page.getByRole('button', { name: 'Duplicate layer', exact: true }).click()
        await expect(layerNames(page)).toHaveText(['Title (copy) (copy)', 'Title (copy)', 'Title', 'Band', video])

        await page.getByRole('button', { name: 'Save template', exact: true }).click()
        const saved = page.waitForResponse(
            (response) => response.url().endsWith(`/templates/${id}`) && response.request().method() === 'PUT',
        )
        await page.getByRole('button', { name: 'Save', exact: true }).click()
        expect((await saved).ok()).toBeTruthy()
        await reopenSaved(page, id)

        const template = await (await page.request.get(`/api/graphics/1/templates/${id}`, { headers })).json()
        const layers: Record<string, unknown>[] = template.layers
        expect(layers.map((layer) => layer.name)).toEqual([
            'Picture',
            'Band',
            'Title',
            'Title (copy)',
            'Title (copy) (copy)',
        ])
        expect(new Set(layers.map((layer) => layer.id)).size).toBe(5)
        const [title, copy] = [layers[2]!, layers[3]!]
        for (const key of ['kind', 'x', 'y', 'width', 'height', 'text', 'font_size', 'bold', 'color']) {
            expect(copy[key], key).toEqual(title[key])
        }
        expect(template.fields).toEqual(['name'])
    } finally {
        await page.request.delete(`/api/graphics/1/templates/${id}`, { headers })
    }
})
