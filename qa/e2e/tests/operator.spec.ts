import { test, expect, type Page } from '@playwright/test'

test.skip(process.env.NOAR_E2E_WRITES !== '1', 'Run with run_staging.py for disposable authenticated tests.')
test.use({ storageState: process.env.NOAR_E2E_STATE_PATH })

async function api(page: Page, path: string) {
    // Use the token actually issued to this browser; do not bypass authentication.
    const token = await page.evaluate(() => localStorage.getItem('token'))
    const response = await page.request.get(path, { headers: { Authorization: `Bearer ${token}` } })
    expect(response.ok(), `${path}: ${response.status()}`).toBeTruthy()
    return response.json()
}

test('playlist edits, undo/redo and save survive a full reload', async ({ page }) => {
    await page.goto('/playlist')
    const playlist = page.getByRole('region', { name: 'Playlist', exact: true })
    const library = page.getByRole('complementary', { name: 'Library' })
    await library.getByRole('button', { name: 'Add e2e-clip.mp4 to the playlist', exact: true }).click()
    await playlist.getByRole('button', { name: 'Edit', exact: true }).click()
    await playlist.getByLabel('Title in the guide (optional)', { exact: true }).fill('E2E edited programme')
    await playlist.getByRole('textbox', { name: 'In', exact: true }).fill('00:00:01')
    await playlist.getByRole('textbox', { name: 'Out', exact: true }).fill('00:00:04')
    await playlist.getByRole('button', { name: 'Apply', exact: true }).click()
    await playlist.getByRole('button', { name: 'Undo', exact: true }).click()
    await expect(playlist.getByText('E2E edited programme', { exact: true })).toHaveCount(0)
    await playlist.getByRole('button', { name: 'Redo', exact: true }).click()
    await expect(playlist.getByText('E2E edited programme', { exact: true })).toBeVisible()
    // A failed save must keep the draft dirty so the operator can retry.
    await page.route('**/api/playlist/1', async (route) => {
        if (route.request().method() === 'POST') {
            await route.fulfill({ status: 503, contentType: 'text/plain', body: 'E2E save temporarily unavailable' })
        } else {
            await route.continue()
        }
    })
    await playlist.getByRole('button', { name: 'Save', exact: true }).click()
    await expect(page.getByRole('alert')).toContainText('E2E save temporarily unavailable')
    await expect(playlist.getByRole('button', { name: 'Save', exact: true })).toBeEnabled()
    await expect(playlist.getByText('E2E edited programme', { exact: true })).toBeVisible()
    await page.unroute('**/api/playlist/1')
    const saved = page.waitForResponse((r) => r.url().endsWith('/api/playlist/1') && r.request().method() === 'POST')
    await playlist.getByRole('button', { name: 'Save', exact: true }).click()
    expect((await saved).ok()).toBeTruthy()
    const date = await playlist.locator('input[type="date"]').inputValue()
    const persisted = await api(page, `/api/playlist/1?date=${date}`)
    expect(persisted.program).toHaveLength(1)
    expect(persisted.program[0]).toMatchObject({ title: 'E2E edited programme', in: 1, out: 4 })
    await page.reload()
    await expect(playlist.getByText('E2E edited programme', { exact: true })).toBeVisible()
    await expect(playlist.getByRole('button', { name: 'Save', exact: true })).toBeDisabled()
})

test('schedule create, edit and delete persist through the real API and reload', async ({ page }) => {
    await page.goto('/schedule')
    await page.getByRole('button', { name: 'Schedule material', exact: true }).click()
    const dialog = page.getByRole('dialog')
    await expect(dialog).toBeVisible()
    await dialog.getByRole('button', { name: /e2e-clip\.mp4/ }).click()
    await dialog.getByRole('textbox', { name: /^Name(?: |$)/ }).fill('E2E scheduled programme')
    await dialog.locator('input[type="time"]').fill('23:30:00')
    const createdResponse = page.waitForResponse(
        (r) => r.url().endsWith('/api/schedule/1') && r.request().method() === 'POST',
    )
    await dialog.getByRole('button', { name: 'Schedule', exact: true }).click()
    const response = await createdResponse
    expect(response.ok()).toBeTruthy()
    const event = await response.json()
    expect(event).toMatchObject({ title: 'E2E scheduled programme', start_time: '23:30:00' })
    await expect(dialog).toHaveCount(0)
    await page.reload()
    const events = page.getByRole('region', { name: "Today's events" })
    let row = events.getByRole('row').filter({ hasText: 'E2E scheduled programme' })
    await expect(row).toBeVisible()
    await row.getByRole('button', { name: 'Edit', exact: true }).click()
    await dialog.getByRole('textbox', { name: /^Name(?: |$)/ }).fill('E2E revised programme')
    await dialog.locator('input[type="time"]').fill('23:35:00')
    await dialog.getByRole('button', { name: 'Every day', exact: true }).click()
    const updated = page.waitForResponse(
        (r) => r.url().endsWith(`/api/schedule/1/${event.id}`) && r.request().method() === 'PUT',
    )
    await dialog.getByRole('button', { name: 'Schedule', exact: true }).click()
    expect((await updated).ok()).toBeTruthy()
    const persisted = await api(page, '/api/schedule/1/events')
    expect(persisted.find((item: { id: number }) => item.id === event.id)).toMatchObject({
        title: 'E2E revised programme',
        start_time: '23:35:00',
        repeat_days: 127,
    })
    const tomorrow = new Date(`${event.start_date}T12:00:00Z`)
    tomorrow.setUTCDate(tomorrow.getUTCDate() + 1)
    const nextDay = await api(page, `/api/schedule/1?date=${tomorrow.toISOString().slice(0, 10)}`)
    expect(nextDay.occurrences.some((item: { event: { id: number } }) => item.event.id === event.id)).toBe(true)
    await page.reload()
    row = events.getByRole('row').filter({ hasText: 'E2E revised programme' })
    await row.getByRole('button', { name: 'Remove', exact: true }).click()
    const removed = page.waitForResponse(
        (r) => r.url().endsWith(`/api/schedule/1/${event.id}`) && r.request().method() === 'DELETE',
    )
    await page.getByRole('dialog').getByRole('button', { name: 'Remove', exact: true }).click()
    expect((await removed).ok()).toBeTruthy()
    await page.reload()
    await expect(events.getByText('E2E revised programme', { exact: true })).toHaveCount(0)
    expect((await api(page, '/api/schedule/1/events')).some((item: { id: number }) => item.id === event.id)).toBe(false)
})

test('a failed playlist load cannot overwrite the saved day', async ({ page }) => {
    await page.goto('/playlist')
    const playlist = page.getByRole('region', { name: 'Playlist', exact: true })
    await playlist.getByRole('button', { name: 'Next day', exact: true }).click()
    await page
        .getByRole('complementary', { name: 'Library' })
        .getByRole('button', { name: 'Add e2e-clip.mp4 to the playlist', exact: true })
        .click()
    const saved = page.waitForResponse((r) => r.url().endsWith('/api/playlist/1') && r.request().method() === 'POST')
    await playlist.getByRole('button', { name: 'Save', exact: true }).click()
    expect((await saved).ok()).toBeTruthy()
    const date = await playlist.locator('input[type="date"]').inputValue()
    const before = await api(page, `/api/playlist/1?date=${date}`)
    let writes = 0
    page.on('request', (req) => {
        if (req.url().endsWith('/api/playlist/1') && req.method() === 'POST') writes += 1
    })
    await page.route('**/api/playlist/1?*', (route) =>
        route.fulfill({
            status: 503,
            contentType: 'text/plain',
            body: 'E2E load temporarily unavailable',
        }),
    )
    // Switching away and back reloads the saved day through the failing GET.
    await playlist.getByRole('button', { name: 'Previous day', exact: true }).click()
    await expect(page.getByRole('alert')).toContainText('E2E load temporarily unavailable')
    const failedLoad = page.waitForResponse(
        (r) => r.url().includes(`/api/playlist/1?date=${date}`) && r.status() === 503,
    )
    await playlist.getByRole('button', { name: 'Next day', exact: true }).click()
    await failedLoad
    await expect(playlist.locator('.loading-spinner')).toHaveCount(0)
    await expect(playlist.getByRole('button', { name: 'Save', exact: true })).toBeDisabled()
    // Adding a new draft item must still leave saving disabled after the load error.
    await page
        .getByRole('complementary', { name: 'Library' })
        .getByRole('button', { name: 'Add e2e-clip.mp4 to the playlist', exact: true })
        .click()
    await expect(playlist.getByText('e2e-clip.mp4', { exact: true })).toBeVisible()
    await expect(playlist.getByRole('button', { name: 'Save', exact: true })).toBeDisabled()
    expect(writes).toBe(0)
    await page.unroute('**/api/playlist/1?*')
    const after = await api(page, `/api/playlist/1?date=${date}`)
    expect(after.program).toEqual(before.program)
})

test('schedule modal moves, traps and restores keyboard focus and closes on Escape', async ({ page }) => {
    await page.goto('/schedule')
    const opener = page.getByRole('button', { name: 'Schedule material', exact: true })
    await opener.click()
    const dialog = page.getByRole('dialog')
    await expect(dialog).toBeFocused()
    await page.keyboard.press('Shift+Tab')
    expect(await dialog.evaluate((el) => el.contains(document.activeElement))).toBe(true)
    await page.keyboard.press('Tab')
    expect(await dialog.evaluate((el) => el.contains(document.activeElement))).toBe(true)
    await page.keyboard.press('Escape')
    await expect(dialog).toHaveCount(0)
    await expect(opener).toBeFocused()
})

test('a scheduled event is stored for the backend restart check', async ({ page }) => {
    await page.goto('/schedule')
    await page.getByRole('button', { name: 'Schedule material', exact: true }).click()
    const dialog = page.getByRole('dialog')
    await dialog.getByRole('button', { name: /e2e-clip\.mp4/ }).click()
    await dialog.getByRole('textbox', { name: /^Name(?: |$)/ }).fill('E2E restart persistence')
    await dialog.locator('input[type="time"]').fill('23:45:00')
    const saved = page.waitForResponse((r) => r.url().endsWith('/api/schedule/1') && r.request().method() === 'POST')
    await dialog.getByRole('button', { name: 'Schedule', exact: true }).click()
    expect((await saved).ok()).toBeTruthy()
    await expect(dialog).toHaveCount(0)
})

test('graphics Composer saves layers, fields, artwork and animation through the real API', async ({ page }) => {
    test.setTimeout(90_000)
    await page.setViewportSize({ width: 1600, height: 1100 })
    await page.goto('/graphics/templates?channel=1')
    await page.getByRole('link', { name: '+ New template', exact: true }).click()
    const add = async (kind: string) => {
        await page.getByRole('button', { name: 'Add layer', exact: true }).click()
        await page.getByRole('group', { name: 'Layer type' }).getByRole('button', { name: kind, exact: true }).click()
    }
    const set = async (name: string, value: string) => {
        const input = page.getByLabel(name, { exact: true })
        await input.fill(value)
        await input.press('Tab')
    }
    await add('Shape')
    await add('Text')
    await set('Text', 'Guest: {name} — {role}')
    const font = page.getByRole('combobox', { name: 'Font', exact: true })
    const family = await font.locator('option').nth(1).getAttribute('value')
    expect(family).toBeTruthy()
    await font.selectOption(family!)
    await set('Colour', '#123456')
    await page.getByLabel('Bold', { exact: true }).check()
    await page.getByRole('combobox', { name: 'Alignment', exact: true }).selectOption('center')
    await page.getByLabel('Proportional', { exact: true }).check()
    await set('Width', '30')
    await expect(page.getByLabel('Height', { exact: true })).toHaveValue('9')
    await set('X', '99')
    await expect(page.getByLabel('X', { exact: true })).toHaveValue('70')
    await expect(page.getByLabel('Width', { exact: true })).toHaveValue('30')
    await page.getByRole('button', { name: 'px', exact: true }).click()
    const width = Number(await page.getByLabel('Width', { exact: true }).inputValue())
    const geometry = await api(page, '/api/playout/config/1')
    expect(width).toBeCloseTo(geometry.output.width * 0.3, 1)
    await page.getByRole('button', { name: '%', exact: true }).click()
    await page.getByRole('button', { name: 'Hide Text', exact: true }).click()
    await expect(page.getByRole('button', { name: 'Select layer Text', exact: true })).toHaveCount(0)
    await page.getByRole('button', { name: 'Show Text', exact: true }).click()
    await page.getByRole('button', { name: 'Delete layer Text', exact: true }).click()
    await page.getByRole('button', { name: 'Undo', exact: true }).last().click()
    await expect(page.getByRole('button', { name: 'Select layer Text', exact: true })).toBeVisible()
    await add('Image')
    const imageName = `composer-${Date.now()}.png`
    await page.getByLabel('Upload to Graphics/', { exact: true }).setInputFiles({
        name: imageName.replace('composer-', 'com?poser:-'),
        mimeType: 'image/png',
        buffer: Buffer.from(
            'iVBORw0KGgoAAAANSUhEUgAAAKAAAABaCAIAAACwpMoFAAAACXBIWXMAAAABAAAAAQBPJcTWAAABH0lEQVR4nO3OAQkAMAzAsPnXcZ3jKkphFCIgs29ymD8Iyh8E5Q+C8gdB+YOg/EFQ/iAofxCUPwjKHwTlD4LyB0H5g6D8QVD+ICh/EJQ/CMofBOUPgvIHQfmDoPxBUP4gKH8QlD8Iyh8E5Q+C8gdB+YOg/EFQ/iAofxCUPwjKHwTlD4LyB0H5g6D8QVD+ICh/EJQ/CMofBOUPgvIHQfmDoPxBUP4gKH8QlD8Iyh8E5Q+C8gdB+YOg/EFQ/iAofxCUPwjKHwTlD4LyB0H5g6D8QVD+ICh/EJQ/CMofBOUPgvIHQfmDoPxBUP4gKH8QlD8Iyh8E5Q+C8gdB+YOg/EFQ/iAofxCUPwjKHwTlD4LyB0H5g6D8QVD+ICh/EJQ/COoD2Wvd1kzRakIAAAAASUVORK5CYII=',
            'base64',
        ),
    })
    await expect(page.getByLabel('Image path or field', { exact: true })).toHaveValue(`Graphics/${imageName}`)
    await expect
        .poll(() =>
            page
                .locator('main img')
                .first()
                .evaluate((image) => (image as HTMLImageElement).naturalWidth),
        )
        .toBe(160)
    const firstImageUrl = await page.locator('main img').first().getAttribute('src')
    const originalBytes = await (await page.request.get(firstImageUrl!)).body()
    const changedImage = await page.evaluate(() => {
        const canvas = document.createElement('canvas')
        canvas.width = 160
        canvas.height = 90
        const context = canvas.getContext('2d')!
        context.fillStyle = '#2040ff'
        context.fillRect(0, 0, canvas.width, canvas.height)
        return canvas.toDataURL('image/png').split(',')[1]!
    })
    await page.getByLabel('Upload to Graphics/', { exact: true }).setInputFiles({
        name: imageName.replace('composer-', 'com?poser:-'),
        mimeType: 'image/png',
        buffer: Buffer.from(changedImage, 'base64'),
    })
    const renamedPath = `Graphics/${imageName.slice(0, -4)}-2.png`
    await expect(page.getByLabel('Image path or field', { exact: true })).toHaveValue(renamedPath)
    const currentImage = page.locator('main img').first()
    await expect.poll(async () => (await currentImage.getAttribute('src')) !== firstImageUrl).toBe(true)
    const renamedBytes = await (await page.request.get((await currentImage.getAttribute('src'))!)).body()
    expect(renamedBytes.equals(originalBytes)).toBe(false)
    expect((await (await page.request.get(firstImageUrl!)).body()).equals(originalBytes)).toBe(true)

    await add('Video in the corner')
    await page.getByRole('button', { name: 'Add layer', exact: true }).click()
    await expect(
        page
            .getByRole('group', { name: 'Layer type' })
            .getByRole('button', { name: 'Video in the corner', exact: true }),
    ).toBeDisabled()
    await page.getByRole('button', { name: 'Add layer', exact: true }).click()
    await page.getByRole('combobox', { name: 'Entry', exact: true }).selectOption('scale')
    await page.getByRole('combobox', { name: 'Exit', exact: true }).selectOption('wipe')
    await set('Duration (ms)', '800')
    await set('Delay (ms)', '400')
    await page.getByRole('button', { name: 'Play entry', exact: true }).click()
    await expect(page.getByRole('button', { name: 'Stop preview', exact: true })).toBeVisible()
    await page.getByRole('button', { name: 'Stop preview', exact: true }).click()
    await page.getByLabel('Preview time', { exact: true }).fill('0.2')
    await expect(page.getByRole('button', { name: 'Back to editing', exact: true })).toBeVisible()
    await page.getByRole('button', { name: 'Back to editing', exact: true }).click()
    const name = `Composer regression ${Date.now()}`
    await page.getByLabel('Template name', { exact: true }).fill(name)
    await page.getByRole('button', { name: 'Save template', exact: true }).click()
    await page.getByLabel('Group', { exact: true }).fill('Regression tests')
    await expect(page.getByRole('paragraph').filter({ hasText: '{name} {role}' })).toBeVisible()
    const created = page.waitForResponse(
        (response) => response.url().endsWith('/api/graphics/1/templates') && response.request().method() === 'POST',
    )
    await page.getByRole('button', { name: 'Save', exact: true }).click()
    const response = await created
    expect(response.ok()).toBeTruthy()
    const template = await response.json()
    expect(template).toMatchObject({ name, group: 'Regression tests', fields: ['name', 'role'] })
    expect(template.layers).toHaveLength(4)
    expect(template.layers[1]).toMatchObject({
        font_family: family,
        color: '#123456',
        bold: true,
        align: 'center',
        width: 30,
        height: 9,
    })
    expect(template.layers[3]).toMatchObject({
        kind: 'video',
        animation_in: 'scale',
        animation_out: 'wipe',
        animation_ms: 800,
        delay_ms: 400,
    })
    await expect(page).toHaveURL(new RegExp(`template=${template.id}(?:&|$)`))
    await page.reload()
    await expect(page.getByLabel('Template name', { exact: true })).toHaveValue(name)
    await page.getByRole('link', { name: 'See templates', exact: true }).click()
    const card = page.getByRole('article').filter({ hasText: name })
    await card.getByRole('link', { name: 'Edit', exact: true }).click()
    await expect(page.getByLabel('Template name', { exact: true })).toHaveValue(name)
    await page.getByRole('link', { name: 'Open in On air', exact: true }).click()
    await page.getByRole('link', { name: 'Edit', exact: true }).click()
    await expect(page).toHaveURL(new RegExp(`template=${template.id}(?:&|$)`))
})

test('graphics Composer keeps unsaved layers after conflicts and a failed update', async ({ page }) => {
    await page.goto('/graphics/composer?channel=1')
    const token = await page.evaluate(() => localStorage.getItem('token'))
    const name = `Composer conflict ${Date.now()}`
    const seed = await page.request.post('/api/graphics/1/templates', {
        headers: { Authorization: `Bearer ${token}` },
        data: { name, group: 'Regression tests', layers: [] },
    })
    expect(seed.ok()).toBeTruthy()
    await page.getByRole('button', { name: 'Add layer', exact: true }).click()
    await page.getByRole('button', { name: 'Shape', exact: true }).click()
    await page.getByLabel('Template name', { exact: true }).fill(name)
    await page.getByRole('button', { name: 'Save template', exact: true }).click()
    await page.getByRole('button', { name: 'Save', exact: true }).click()
    await expect(page.getByRole('alert')).toContainText('That name is already taken')
    await expect(page.getByRole('button', { name: 'Select layer Shape', exact: true })).toBeVisible()
    await page.getByLabel('Template name', { exact: true }).last().fill(`${name} revised`)
    const created = page.waitForResponse(
        (response) => response.url().endsWith('/api/graphics/1/templates') && response.request().method() === 'POST',
    )
    await page.getByRole('button', { name: 'Save', exact: true }).click()
    const template = await (await created).json()
    await expect(page).toHaveURL(new RegExp(`template=${template.id}(?:&|$)`))
    await page.getByLabel('Layer name', { exact: true }).fill('Unsaved shape')
    await page.getByLabel('Layer name', { exact: true }).press('Tab')
    await page.route(`**/api/graphics/1/templates/${template.id}`, (route) =>
        route.request().method() === 'PUT'
            ? route.fulfill({ status: 503, contentType: 'text/plain', body: 'Temporary save failure' })
            : route.continue(),
    )
    await page.getByRole('button', { name: 'Save template', exact: true }).click()
    await page.getByRole('button', { name: 'Save', exact: true }).click()
    await expect(page.getByRole('alert')).toContainText('Temporary save failure')
    await expect(page.getByLabel('Layer name', { exact: true })).toHaveValue('Unsaved shape')
    await expect(page.getByRole('button', { name: 'Save', exact: true })).toBeEnabled()
    await page.unroute(`**/api/graphics/1/templates/${template.id}`)
    await page.getByRole('button', { name: 'Save', exact: true }).click()
    await expect(page.getByRole('status').filter({ hasText: 'saved' })).toBeVisible()
    await page.reload()
    await expect(page.getByLabel('Layer name', { exact: true })).toHaveValue('Unsaved shape')
})

test('graphics Composer cannot overwrite a template after a failed load', async ({ page }) => {
    await page.goto('/graphics/templates?channel=1')
    const templates = await api(page, '/api/graphics/1/templates')
    const template = templates[0]
    expect(template).toBeTruthy()
    await page.route(`**/api/graphics/1/templates/${template.id}`, (route) =>
        route.fulfill({ status: 503, contentType: 'text/plain', body: 'Temporary load failure' }),
    )
    await page.goto(`/graphics/composer?channel=1&template=${template.id}`)
    await expect(page.getByRole('alert')).toContainText('Temporary load failure')
    await expect(page.getByRole('button', { name: 'Save template', exact: true })).toBeDisabled()
    await page.unroute(`**/api/graphics/1/templates/${template.id}`)
    await page.getByRole('button', { name: 'Retry', exact: true }).click()
    await expect(page.getByLabel('Template name', { exact: true })).toHaveValue(template.name)
    await expect(page.getByRole('button', { name: 'Save template', exact: true })).toBeEnabled()
})

test('assigned channel models use dedicated controls and legacy editors', async ({ page }) => {
    await page.goto('/graphics/templates?channel=1')
    const channel = page.getByRole('region', { name: 'Channel graphics', exact: true })
    await expect(channel.getByRole('heading', { name: 'Channel logo', exact: true })).toBeVisible()

    for (const kind of ['logo', 'ticker', 'lshape']) {
        const card = channel.locator(`article[data-kind="${kind}"]`)
        await expect(card).not.toContainText('No fields')
        await expect(card).not.toContainText('With L-shape:')
        await expect(card.getByRole('link', { name: 'Put on air', exact: true })).toHaveCount(0)
        await expect(card.getByRole('link', { name: 'Edit', exact: true })).not.toHaveAttribute('href', /composer/)
    }

    const models = await api(page, '/api/graphics/1/channel')
    const originalName = models.find((view: { kind: string }) => view.kind === 'ticker').template.name
    await channel.locator('article[data-kind="ticker"]').getByRole('link', { name: 'Edit', exact: true }).click()
    await expect(page).toHaveURL(/graphics\/logo.*#ticker/)
    const text = `E2E channel bulletin ${Date.now()}`
    await page.getByLabel('Ticker text', { exact: true }).fill(text)
    await page.getByRole('button', { name: 'Save ticker', exact: true }).click()
    await expect(page.getByRole('button', { name: 'Save ticker', exact: true })).toBeDisabled()
    await page.goto('/graphics/templates?channel=1')
    const ticker = page.getByRole('switch', { name: 'Ticker', exact: true })

    if (await ticker.isChecked()) {
        await ticker.click()
    }

    await expect(ticker).not.toBeChecked()
    await ticker.click()
    await expect(ticker).toBeChecked()
    await ticker.click()
    await expect(ticker).not.toBeChecked()
    await channel.locator('article[data-kind="ticker"]').getByRole('link', { name: 'Edit', exact: true }).click()
    await page.getByLabel('Ticker text', { exact: true }).fill(`${text} updated`)
    await page.getByRole('button', { name: 'Save ticker', exact: true }).click()
    await expect(page.getByRole('button', { name: 'Save ticker', exact: true })).toBeDisabled()
    await page.goto('/graphics/templates?channel=1')
    await expect(ticker).not.toBeChecked()
    const saved = await api(page, '/api/graphics/1/channel')
    expect(saved.find((view: { kind: string }) => view.kind === 'ticker').template.name).toBe(originalName)
    await page.setViewportSize({ width: 390, height: 844 })
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true)
})
