import { resolve } from 'node:path'
import { test, expect, type Page } from '@playwright/test'
import { expectBackToTemplates, openTab, reopenSaved } from './composer'

test.skip(process.env.NOAR_E2E_WRITES !== '1', 'Requires runner-owned disposable staging.')
test.use({ storageState: process.env.NOAR_E2E_STATE_PATH })

async function fixture(page: Page, layers?: object[]) {
    await page.addInitScript(() => localStorage.setItem('language', 'en'))
    await page.goto('/graphics/templates?channel=1')
    const token = await page.evaluate(() => localStorage.getItem('token'))
    const headers = { Authorization: `Bearer ${token}` }
    const response = await page.request.post('/api/graphics/1/templates', {
        headers,
        data: {
            name: `Editor safety ${Date.now()}`,
            group: 'Editor safety',
            duration: 8,
            always_on: false,
            during_lshape: 'wait',
            layers: layers ?? [
                {
                    id: 'artwork',
                    name: 'Artwork',
                    kind: 'image',
                    visible: true,
                    x: 0,
                    y: 0,
                    width: 100,
                    height: 100,
                    image: '00-assets/logo.png',
                },
            ],
        },
    })
    expect(response.ok(), await response.text()).toBeTruthy()

    return { template: await response.json(), headers }
}

/** Exercise an actual reused-component route update while an authoring request is delayed. */
async function openDocument(page: Page, id: number) {
    await page.evaluate(async (template) => {
        const root = document.querySelector('#app') as unknown as {
            __vue_app__: { config: { globalProperties: { $router: { push: (location: object) => Promise<unknown> } } } }
        }
        await root.__vue_app__.config.globalProperties.$router.push({
            name: 'graphics-composer',
            query: { channel: 1, template },
        })
    }, id)
}

test('a rejected new template leaves no model behind and the corrected draft can be saved', async ({ page }) => {
    await page.addInitScript(() => localStorage.setItem('language', 'en'))
    await page.goto('/graphics/composer?channel=1')
    const token = await page.evaluate(() => localStorage.getItem('token'))
    const headers = { Authorization: `Bearer ${token}` }
    const name = `Atomic new model ${Date.now()}`
    let id: number | undefined
    try {
        await page.getByLabel('Template name', { exact: true }).fill(name)
        await page.getByRole('button', { name: 'Add layer', exact: true }).click()
        await page
            .getByRole('group', { name: 'Layer type', exact: true })
            .getByRole('button', { name: 'Text', exact: true })
            .click()
        await page.getByRole('button', { name: 'Save template', exact: true }).click()
        // The server refuses the first save; nothing may be created and the draft stays open.
        await page.route('**/api/graphics/1/templates', (route) =>
            route.request().method() === 'POST'
                ? route.fulfill({ status: 400, contentType: 'text/plain', body: 'Rejected draft' })
                : route.continue(),
        )
        const rejected = page.waitForResponse(
            (response) => response.url().endsWith('/templates') && response.request().method() === 'POST',
        )
        await page.getByRole('button', { name: 'Save', exact: true }).click()
        expect((await rejected).ok()).toBe(false)
        await expect(page.getByRole('alert')).toBeVisible()
        const models = await (await page.request.get('/api/graphics/1/templates', { headers })).json()
        expect(models.some((model: { name: string }) => model.name === name)).toBe(false)
        expect(new URL(page.url()).searchParams.has('template')).toBe(false)
        await page.unroute('**/api/graphics/1/templates')
        const saved = page.waitForResponse(
            (response) => response.url().endsWith('/templates') && response.request().method() === 'POST',
        )
        await page.getByRole('button', { name: 'Save', exact: true }).click()
        const response = await saved
        expect(response.ok()).toBeTruthy()
        id = (await response.json()).id
        await reopenSaved(page, id!)
        await expect(page.getByLabel('Layer name', { exact: true })).toBeEnabled()
    } finally {
        if (id !== undefined) await page.request.delete(`/api/graphics/1/templates/${id}`, { headers })
    }
})

test('a delayed font failure cannot keep a new document locked or publish its old error', async ({ page }) => {
    const layer = {
        id: 'shared',
        name: 'Text',
        kind: 'text',
        text: 'Text',
        visible: true,
        x: 10,
        y: 10,
        width: 40,
        height: 20,
    }
    const first = await fixture(page, [layer])
    const second = await fixture(page, [layer])
    let release!: () => void
    let started!: () => void
    const gate = new Promise<void>((resolve) => {
        release = resolve
    })
    const requested = new Promise<void>((resolve) => {
        started = resolve
    })
    await page.route('**/font-assets?filename=*', async (route) => {
        if (route.request().method() !== 'POST') return route.continue()
        started()
        await gate
        await route.fulfill({ status: 503, contentType: 'text/plain', body: 'Abandoned font upload' })
    })
    try {
        await page.goto(`/graphics/composer?channel=1&template=${first.template.id}`)
        await openTab(page, 'Style')
        await page
            .getByLabel('Upload font', { exact: true })
            .setInputFiles(resolve(__dirname, '../../../tests_assets/fonts/graphics.ttf'))
        await requested
        await openDocument(page, second.template.id)
        await expect(page.getByLabel('Template name', { exact: true })).toHaveValue(second.template.name)
        await expect(page.getByLabel('Layer name', { exact: true })).toBeEnabled()
        const completed = page.waitForResponse(
            (response) => response.url().includes('/font-assets?filename=') && response.request().method() === 'POST',
        )
        release()
        await completed
        await expect(page.getByRole('alert')).toHaveCount(0)
        await expect(page.getByRole('combobox', { name: 'Font', exact: true })).toHaveValue('')
    } finally {
        release()
        for (const model of [first, second])
            await page.request.delete(`/api/graphics/1/templates/${model.template.id}`, { headers: model.headers })
    }
})

test('an abandoned image upload cannot unlock a newer document load or upload into it', async ({ page }) => {
    const first = await fixture(page)
    const second = await fixture(page)
    let releaseFolder!: () => void
    let releaseDocument!: () => void
    let started!: () => void
    const folderGate = new Promise<void>((resolve) => {
        releaseFolder = resolve
    })
    const documentGate = new Promise<void>((resolve) => {
        releaseDocument = resolve
    })
    const requested = new Promise<void>((resolve) => {
        started = resolve
    })
    let uploads = 0
    await page.route('**/api/file/1/create-folder', async (route) => {
        started()
        await folderGate
        await route.fulfill({ status: 204 })
    })
    await page.route(`**/api/graphics/1/templates/${second.template.id}`, async (route) => {
        if (route.request().method() !== 'GET') return route.continue()
        const response = await route.fetch()
        await documentGate
        await route.fulfill({ response })
    })
    await page.route('**/api/file/1/upload?path=Graphics', async (route) => {
        uploads += 1
        await route.fulfill({ status: 204 })
    })
    try {
        await page.goto(`/graphics/composer?channel=1&template=${first.template.id}`)
        await openTab(page, 'Content')
        await page
            .locator('input[type="file"][accept*="image"]')
            .setInputFiles(resolve(__dirname, '../../../tests_assets/storage/logo.png'))
        await requested
        await openDocument(page, second.template.id)
        await expect(page.getByLabel('Layer name', { exact: true })).toBeDisabled()
        const completed = page.waitForResponse((response) => response.url().endsWith('/create-folder'))
        releaseFolder()
        await completed
        await expect(page.getByLabel('Layer name', { exact: true })).toBeDisabled()
        expect(uploads).toBe(0)
        releaseDocument()
        await expect(page.getByLabel('Template name', { exact: true })).toHaveValue(second.template.name)
        await expect(page.getByLabel('Layer name', { exact: true })).toBeEnabled()
    } finally {
        releaseFolder()
        releaseDocument()
        for (const model of [first, second])
            await page.request.delete(`/api/graphics/1/templates/${model.template.id}`, { headers: model.headers })
    }
})

test('the first save returns to the templates and the new model opens editable', async ({ page }) => {
    await page.addInitScript(() => localStorage.setItem('language', 'en'))
    await page.goto('/graphics/composer?channel=1')
    const token = await page.evaluate(() => localStorage.getItem('token'))
    const headers = { Authorization: `Bearer ${token}` }
    let id: number | undefined
    try {
        await page.getByLabel('Template name', { exact: true }).fill(`First-save safety ${Date.now()}`)
        await page.getByRole('button', { name: 'Add layer', exact: true }).click()
        await page
            .getByRole('group', { name: 'Layer type', exact: true })
            .getByRole('button', { name: 'Text', exact: true })
            .click()
        await page.getByRole('button', { name: 'Save template', exact: true }).click()
        const created = page.waitForResponse(
            (response) => response.url().endsWith('/templates') && response.request().method() === 'POST',
        )
        await page.getByRole('button', { name: 'Save', exact: true }).click()
        const response = await created
        expect(response.ok()).toBeTruthy()
        id = (await response.json()).id
        await reopenSaved(page, id!)
        await expect(page.getByLabel('Layer name', { exact: true })).toBeEnabled()
        await expect(page.getByLabel('Template name', { exact: true })).toBeEnabled()
        await page.getByLabel('Layer name', { exact: true }).fill('Edited after first save')
        await page.getByLabel('Layer name', { exact: true }).press('Tab')
        await page.getByRole('button', { name: 'Save template', exact: true }).click()
        const updated = page.waitForResponse(
            (result) => result.url().endsWith(`/templates/${id}`) && result.request().method() === 'PUT',
        )
        await page.getByRole('button', { name: 'Save', exact: true }).click()
        expect((await updated).ok()).toBeTruthy()
        await reopenSaved(page, id!)
        await expect(page.getByLabel('Layer name', { exact: true })).toHaveValue('Edited after first save')
        await expect(page.getByLabel('Layer name', { exact: true })).toBeEnabled()
    } finally {
        if (id !== undefined) await page.request.delete(`/api/graphics/1/templates/${id}`, { headers })
    }
})

test('cancelled channel changes preserve the dirty document before configuration initialization', async ({ page }) => {
    const { template, headers } = await fixture(page)
    let destinationRequests = 0
    await page.route('**/api/channels', async (route) => {
        const response = await route.fetch()
        const channels = await response.json()
        await route.fulfill({
            response,
            json: [...channels, { ...channels[0], id: 987654, name: 'Guard destination' }],
        })
    })
    page.on('request', (request) => {
        if (request.url().includes('/api/playout/config/987654')) destinationRequests += 1
    })
    try {
        await page.goto(`/graphics/composer?channel=1&template=${template.id}`)
        await page.getByLabel('Template name', { exact: true }).fill('Unsaved document')
        page.once('dialog', (dialog) => dialog.dismiss())
        await page.getByRole('combobox', { name: 'Channel', exact: true }).selectOption('987654')
        await expect(page.getByLabel('Template name', { exact: true })).toHaveValue('Unsaved document')
        await expect(page.getByRole('combobox', { name: 'Channel', exact: true })).toHaveValue('1')
        expect(new URL(page.url()).searchParams.get('channel')).toBe('1')
        expect(destinationRequests).toBe(0)
    } finally {
        await page.request.delete(`/api/graphics/1/templates/${template.id}`, { headers })
    }
})

test('save disables native editing and a late response cannot navigate back after leaving Composer', async ({
    page,
}) => {
    const { template, headers } = await fixture(page)
    let release!: () => void
    const gate = new Promise<void>((resolve) => {
        release = resolve
    })
    let received!: () => void
    const requested = new Promise<void>((resolve) => {
        received = resolve
    })
    await page.route(`**/api/graphics/1/templates/${template.id}`, async (route) => {
        if (route.request().method() !== 'PUT') return route.continue()
        const response = await route.fetch()
        received()
        await gate
        await route.fulfill({ response })
    })
    try {
        await page.goto(`/graphics/composer?channel=1&template=${template.id}`)
        await page.getByLabel('Layer name', { exact: true }).fill('Submitted layer')
        await page.getByLabel('Layer name', { exact: true }).press('Tab')
        await page.getByRole('button', { name: 'Save template', exact: true }).click()
        await page.getByRole('button', { name: 'Save', exact: true }).click()
        await requested
        await expect(page.getByLabel('Layer name', { exact: true })).toBeDisabled()
        await expect(page.getByLabel('X', { exact: true })).toBeDisabled()
        for (const input of await page.getByLabel('Template name', { exact: true }).all()) {
            await expect(input).toBeDisabled()
        }
        page.once('dialog', (dialog) => dialog.dismiss())
        await page.getByRole('link', { name: 'Player', exact: true }).click()
        await expect(page).toHaveURL(/\/graphics\/composer/)
        await expect(page.getByLabel('Layer name', { exact: true })).toBeDisabled()
        page.once('dialog', (dialog) => dialog.accept())
        await page.getByRole('link', { name: 'Player', exact: true }).click()
        await expect(page).toHaveURL(/\/player/)
        const completed = page.waitForResponse(
            (response) => response.url().endsWith(`/templates/${template.id}`) && response.request().method() === 'PUT',
        )
        release()
        await completed
        await expect(page).toHaveURL(/\/player/)
    } finally {
        release()
        await page.request.delete(`/api/graphics/1/templates/${template.id}`, { headers })
    }
})

test('full-frame canvas targets stay visible and support keyboard resizing and selection', async ({ page }) => {
    const { template, headers } = await fixture(page)
    try {
        for (const width of [1440, 390]) {
            await page.setViewportSize({ width, height: 1000 })
            await page.goto(`/graphics/composer?channel=1&template=${template.id}`)
            const canvas = page.locator('.composer-canvas')
            await expect(page.getByRole('button', { name: 'Select layer Artwork', exact: true })).toHaveAttribute(
                'aria-pressed',
                'true',
            )
            const frame = await canvas.boundingBox()
            expect(frame).toBeTruthy()
            for (const name of ['Resize width', 'Resize height', 'Resize width and height']) {
                const bounds = await page.getByRole('button', { name, exact: true }).boundingBox()
                expect(bounds).toBeTruthy()
                expect(bounds!.width).toBeGreaterThanOrEqual(24)
                expect(bounds!.x).toBeGreaterThanOrEqual(frame!.x)
                expect(bounds!.y).toBeGreaterThanOrEqual(frame!.y)
                expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(frame!.x + frame!.width + 1)
                expect(bounds!.y + bounds!.height).toBeLessThanOrEqual(frame!.y + frame!.height + 1)
            }
            await page.getByRole('button', { name: 'Resize width', exact: true }).press('ArrowLeft')
            await expect(page.getByLabel('Width', { exact: true })).toHaveValue('99')
            await page.getByRole('button', { name: 'Resize height', exact: true }).press('Shift+ArrowUp')
            await expect(page.getByLabel('Height', { exact: true })).toHaveValue('90')
            await page.getByRole('button', { name: 'Undo', exact: true }).click()
            await page.getByRole('button', { name: 'Undo', exact: true }).click()
            const tiny = {
                ...template,
                layers: template.layers.map((layer: object) => ({ ...layer, x: 96, y: 2, width: 3, height: 6 })),
            }
            expect(
                (await page.request.put(`/api/graphics/1/templates/${template.id}`, { headers, data: tiny })).ok(),
            ).toBeTruthy()
            await page.reload()
            for (const name of ['Resize width', 'Resize height', 'Resize width and height']) {
                const handle = page.getByRole('button', { name, exact: true })
                await handle.scrollIntoViewIfNeeded()
                await expect
                    .poll(() =>
                        handle.evaluate((button) => {
                            const bounds = button.getBoundingClientRect()
                            const hit = document.elementFromPoint(
                                bounds.x + bounds.width / 2,
                                bounds.y + bounds.height / 2,
                            )
                            return hit === button || button.contains(hit)
                        }),
                    )
                    .toBe(true)
            }
            expect(
                (await page.request.put(`/api/graphics/1/templates/${template.id}`, { headers, data: template })).ok(),
            ).toBeTruthy()
        }
    } finally {
        await page.request.delete(`/api/graphics/1/templates/${template.id}`, { headers })
    }
})

test('library refreshes temporary badges and waiting takes can be cancelled without an active graphic', async ({
    page,
}) => {
    const { template, headers } = await fixture(page)
    let aired: object[] = []
    await page.route('**/api/graphics/1/on-air', (route) => route.fulfill({ json: aired }))
    try {
        await page.goto('/graphics/templates?channel=1')
        const card = page
            .locator('article')
            .filter({ has: page.getByRole('heading', { name: template.name, exact: true }) })
        await expect(card).toBeVisible()
        aired = [{ take_id: 999, template_id: template.id, name: template.name, since: 0, leaving: false }]
        await expect(card.getByText('ON AIR', { exact: true })).toBeVisible({ timeout: 6000 })
        aired = []
        await expect(card.getByText('ON AIR', { exact: true })).toHaveCount(0, { timeout: 6000 })
        const filter = page.getByRole('button', { name: 'Editor safety', exact: true })
        await filter.click()
        await expect(filter).toHaveAttribute('aria-pressed', 'true')
        await expect(filter).toHaveClass(/border-accent/)

        await page.route('**/api/graphics/1/take', (route) => route.fulfill({ json: { state: 'waiting' } }))
        let channelOut = false
        await page.route('**/api/graphics/1/out', async (route) => {
            expect(route.request().postDataJSON()).toEqual({})
            channelOut = true
            await route.fulfill({ status: 204 })
        })
        await page.goto(`/graphics/onair?channel=1&template=${template.id}`)
        await page.getByRole('button', { name: /^put on air$/i }).click()
        const allOut = page.getByRole('button', { name: 'Take all out', exact: true })
        await expect(allOut).toBeEnabled()
        await allOut.click()
        await expect.poll(() => channelOut).toBe(true)
        await expect(page.getByRole('button', { name: /^take out$/i })).toBeDisabled()
    } finally {
        await page.request.delete(`/api/graphics/1/templates/${template.id}`, { headers })
    }
})

test('shared placeholders discard former-layer mappings when remapped or returned to manual', async ({ page }) => {
    await page.addInitScript(() => localStorage.setItem('language', 'en'))
    await page.goto('/graphics/templates?channel=1')
    const token = await page.evaluate(() => localStorage.getItem('token'))
    const headers = { Authorization: `Bearer ${token}` }
    const sourceResponse = await page.request.post('/api/graphics/1/sources', {
        headers,
        data: { name: `Mapping safety ${Date.now()}`, kind: 'csv', interval: 5, document: 'a,b\n1,2' },
    })
    expect(sourceResponse.ok(), await sourceResponse.text()).toBeTruthy()
    const source = await sourceResponse.json()
    const { template } = await fixture(
        page,
        ['first', 'second'].map((id) => ({
            id,
            name: id,
            kind: 'text',
            text: '{price}',
            visible: true,
            x: 10,
            y: 10,
            width: 40,
            height: 20,
            data_fields: { price: { source_id: source.id, column: 'a', row: 0 } },
        })),
    )
    try {
        await page.goto(`/graphics/composer?channel=1&template=${template.id}`)
        await openTab(page, 'Style')
        await page.getByRole('combobox', { name: 'Font', exact: true }).focus()
        await page.keyboard.press('Tab')
        const fontInput = page.getByLabel('Upload font', { exact: true })
        await expect(fontInput).toBeFocused()
        await expect(fontInput.locator('..')).toHaveCSS('outline-width', '2px')
        await openTab(page, 'Content')
        await page.getByLabel('Text', { exact: true }).fill('Literal text')
        await page.getByLabel('Text', { exact: true }).press('Tab')
        await page.getByRole('button', { name: 'Save template', exact: true }).click()
        await page.getByRole('combobox', { name: 'Column for price', exact: true }).selectOption('b')
        await page.getByRole('button', { name: 'Save', exact: true }).click()
        await expect(page.getByRole('button', { name: 'Save', exact: true })).toHaveCount(0)
        const remapped = await (await page.request.get(`/api/graphics/1/templates/${template.id}`, { headers })).json()
        expect(remapped.layers[0].data_fields ?? {}).toEqual({})
        expect(remapped.layers[1].data_fields.price.column).toBe('b')
        await reopenSaved(page, template.id)

        await page.getByRole('button', { name: 'Save template', exact: true }).click()
        await page.getByRole('combobox', { name: 'Source for price', exact: true }).selectOption('')
        await page.getByRole('button', { name: 'Save', exact: true }).click()
        await expect(page.getByRole('button', { name: 'Save', exact: true })).toHaveCount(0)
        const manual = await (await page.request.get(`/api/graphics/1/templates/${template.id}`, { headers })).json()
        expect(
            manual.layers.every((layer: { data_fields?: object }) => Object.keys(layer.data_fields ?? {}).length === 0),
        ).toBe(true)
    } finally {
        await page.request.delete(`/api/graphics/1/templates/${template.id}`, { headers })
        await page.request.delete(`/api/graphics/1/sources/${source.id}`, { headers })
    }
})
