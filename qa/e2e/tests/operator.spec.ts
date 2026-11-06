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
    const nextDayLoad = page.waitForResponse(
        (response) => response.url().includes('/api/playlist/1?date=') && response.request().method() === 'GET',
    )
    await playlist.getByRole('button', { name: 'Next day', exact: true }).click()
    await nextDayLoad
    await expect(playlist.locator('.loading-spinner')).toHaveCount(0)
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

test('assigned channel models use dedicated controls and layered Composer', async ({ page }) => {
    await page.goto('/graphics/templates?channel=1')
    const channel = page.getByRole('region', { name: 'Channel graphics', exact: true })
    await expect(channel.getByRole('heading', { name: 'Channel logo', exact: true })).toBeVisible()

    for (const kind of ['logo', 'ticker', 'lshape']) {
        const card = channel.locator(`article[data-kind="${kind}"]`)
        await expect(card).not.toContainText('No fields')
        await expect(card).not.toContainText('With L-shape:')
        await expect(card.getByRole('link', { name: 'Put on air', exact: true })).toHaveCount(0)
        await expect(card.getByRole('link', { name: 'Edit', exact: true })).toHaveAttribute('href', /composer/)
    }

    const models = await api(page, '/api/graphics/1/channel')
    const originalName = models.find((view: { kind: string }) => view.kind === 'ticker').template.name
    await channel.locator('article[data-kind="ticker"]').getByRole('link', { name: 'Edit', exact: true }).click()
    await expect(page).toHaveURL(/graphics\/composer.*template=/)
    const text = `E2E channel bulletin ${Date.now()}`
    const add = page.getByRole('button', { name: 'Add layer', exact: true })
    await add.click()
    await page.getByRole('group', { name: 'Layer type' }).getByRole('button', { name: 'Text', exact: true }).click()
    await page.getByLabel('Text', { exact: true }).fill(text)
    await page.getByLabel('Always on air', { exact: true }).check()
    await page.getByRole('button', { name: 'Save template', exact: true }).click()
    await page.getByRole('button', { name: 'Save', exact: true }).click()
    await expect(page.getByRole('status').filter({ hasText: 'saved' })).toBeVisible()
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
    await page.getByLabel('Text', { exact: true }).fill(`${text} updated`)
    await page.getByRole('button', { name: 'Save template', exact: true }).click()
    await page.getByRole('button', { name: 'Save', exact: true }).click()
    await expect(page.getByRole('status').filter({ hasText: 'saved' })).toBeVisible()
    await page.goto('/graphics/templates?channel=1')
    await expect(ticker).not.toBeChecked()
    const saved = await api(page, '/api/graphics/1/channel')
    expect(saved.find((view: { kind: string }) => view.kind === 'ticker').template.name).toBe(originalName)
    await page.setViewportSize({ width: 390, height: 844 })
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true)
})

test('channel graphics editors replace legacy navigation and retain bookmarked links', async ({ page }) => {
    await page.goto('/graphics/templates?channel=1')
    const navigation = page.getByRole('navigation', { name: 'Graphics', exact: true })
    await expect(navigation.locator('a')).toHaveCount(5)
    await expect(
        navigation.locator('a[href*="/graphics/logo"], a[href*="/graphics/lshape"], a[href*="/graphics/scenes"]'),
    ).toHaveCount(0)

    for (const kind of ['logo', 'lshape', 'ticker', 'scene']) {
        const edit = page
            .locator(`article[data-kind="${kind}"]`)
            .first()
            .getByRole('link', { name: 'Edit', exact: true })
        await expect(edit).toHaveAttribute(
            'href',
            kind === 'scene' ? /graphics\/channel\/scene/ : /graphics\/composer.*template=/,
        )
        await edit.click()
        if (kind === 'scene') {
            await expect(page.getByTestId('channel-graphics-editor')).toBeVisible()
        } else {
            await expect(page.getByLabel('Template name', { exact: true })).toBeVisible()
        }
        await page
            .getByRole('link', { name: kind === 'scene' ? 'Back to templates' : 'See templates', exact: true })
            .click()
        await expect(page).toHaveURL(/graphics\/templates\?channel=1/)
    }

    for (const [legacy, kind] of [
        ['logo', 'logo'],
        ['logo#ticker', 'ticker'],
        ['lshape', 'lshape'],
        ['scenes', 'scene'],
    ]) {
        const [path, hash] = legacy!.split('#')
        await page.goto(`/graphics/${path}?channel=1${hash ? `#${hash}` : ''}`)
        if (kind === 'scene') {
            await expect(page).toHaveURL(/graphics\/channel\/scene/)
            await expect(page.getByTestId('channel-graphics-editor')).toBeVisible()
        } else {
            await expect(page).toHaveURL(/graphics\/composer.*template=/)
            await expect(page.getByLabel('Template name', { exact: true })).toBeVisible()
        }
    }

    await page.setViewportSize({ width: 390, height: 844 })
    const selector = page.getByLabel('Page', { exact: true })
    await expect(selector.locator('option')).toHaveCount(5)
    await expect(selector.locator('option[value="logo"], option[value="lshape"], option[value="scenes"]')).toHaveCount(
        0,
    )
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true)
})

test('the library can create the first scene after legacy menu removal', async ({ page, request }) => {
    await page.goto('/graphics/templates?channel=1')
    const token = await page.evaluate(() => localStorage.getItem('token'))
    const headers = { Authorization: `Bearer ${token}` }
    const config = await api(page, '/api/playout/config/1')
    const scenes = config.processing.graphics_scenes

    try {
        config.processing.graphics_scenes = []
        expect((await page.request.put('/api/playout/config/1', { headers, data: config })).ok()).toBeTruthy()
        await page.reload()
        const section = page.getByRole('region', { name: 'Scenes', exact: true })
        await expect(section).toHaveCount(0)
        await page.getByRole('link', { name: 'Manage scenes', exact: true }).click()
        await page.getByRole('button', { name: 'New scene', exact: true }).click()
        const name = `E2E first scene ${Date.now()}`
        await page.getByLabel('Name', { exact: true }).fill(name)
        const response = page.waitForResponse(
            (item) => item.url().endsWith('/api/playout/config/1') && item.request().method() === 'PUT',
        )
        await page.getByRole('button', { name: 'Save', exact: true }).click()
        expect((await response).ok()).toBeTruthy()
        await page.getByRole('link', { name: 'Back to templates', exact: true }).click()
        await expect(section.locator('article[data-kind="scene"]')).toContainText(name)
        const applied = await page.request.post('/api/control/1/scene', { headers, data: { name } })
        expect(applied.ok()).toBeTruthy()
    } finally {
        const response = await request.get('/api/playout/config/1', { headers })
        expect(response.ok()).toBeTruthy()
        const current = await response.json()
        current.processing.graphics_scenes = scenes
        expect((await request.put('/api/playout/config/1', { headers, data: current })).ok()).toBeTruthy()
    }
})

test('the XMLTV logo tab saves its independent guide icon without changing layers', async ({ page }) => {
    await page.goto('/graphics/templates?channel=1')
    const roles = await api(page, '/api/graphics/1/channel')
    const logo = roles.find((view: { kind: string }) => view.kind === 'logo').template
    const config = await api(page, '/api/playout/config/1')
    const original = config.processing.guide_logo
    await page.getByRole('link', { name: 'XMLTV logo', exact: true }).click()
    await expect(page).toHaveURL(/graphics\/xmltv\?channel=1/)
    const field = page.getByLabel('Logo in the guide (XMLTV)', { exact: true })
    await expect(field).toHaveValue(original)
    const save = async (path: string) => {
        await field.fill(path)
        const response = page.waitForResponse(
            (item) => item.url().endsWith('/api/playout/config/1') && item.request().method() === 'PUT',
        )
        await page.getByRole('button', { name: 'Save guide icon', exact: true }).click()
        expect((await response).ok()).toBeTruthy()
        await expect(page.getByRole('status', { name: '' }).filter({ hasText: 'Guide icon saved.' })).toBeVisible()
    }

    try {
        await save('guide-logo.png')
        await page.reload()
        await expect(field).toHaveValue('guide-logo.png')
        const xml = await page.request.get('/xmltv/1/guide.xml?days=1')
        expect(xml.ok()).toBeTruthy()
        expect(await xml.text()).toContain('/guide-logo.png')
        const saved = await api(page, `/api/graphics/1/templates/${logo.id}`)
        expect(saved.layers).toEqual(logo.layers)
    } finally {
        if (original !== 'guide-logo.png') {
            await save(original)
        }
    }
})

test('the API tutorial uses actual model IDs, separates tokens and keys, and stays read-only', async ({ page }) => {
    await page.goto('/graphics/api?channel=1')
    const token = await page.evaluate(() => localStorage.getItem('token'))
    const created = await page.request.post('/api/graphics/1/templates', {
        headers: { Authorization: `Bearer ${token}` },
        data: {
            name: `API tutorial ${Date.now()}`,
            group: 'API examples',
            layers: [
                {
                    id: 'title',
                    name: 'Title',
                    kind: 'text',
                    text: 'Topic: {topic}',
                    x: 5,
                    y: 70,
                    width: 90,
                    height: 12,
                },
            ],
        },
    })
    expect(created.ok()).toBeTruthy()
    const ordinary = await created.json()
    const models = await api(page, '/api/graphics/1/channel')
    let graphicsWrites = 0
    page.on('request', (request) => {
        if (request.url().includes('/api/graphics/') && request.method() !== 'GET') {
            graphicsWrites += 1
        }
    })
    await page.reload()
    const authenticated = page.getByRole('region', { name: 'Authenticated API for models and ticker', exact: true })
    await expect(authenticated).toContainText('The graphics key below only authorizes')
    await expect(authenticated).toContainText('Authorization: Bearer $TOKEN')
    const selected = page.getByRole('combobox', { name: 'Channel model for these examples', exact: true })

    for (const kind of ['logo', 'ticker', 'lshape']) {
        const model = models.find((item: { kind: string }) => item.kind === kind)
        await selected.selectOption(String(model.template.id))
        await expect(authenticated).toContainText(`/api/graphics/1/channel/${model.template.id}`)
        await expect(authenticated).toContainText('"on":true')
        await expect(authenticated).toContainText('"on":false')
        await expect(authenticated.getByText('Save Always on air through the API', { exact: true })).toHaveCount(
            kind === 'lshape' ? 0 : 1,
        )
    }

    const temporary = page.getByRole('region', { name: 'Temporary templates: take and out', exact: true })
    await page
        .getByRole('combobox', { name: 'Temporary template for these examples', exact: true })
        .selectOption(String(ordinary.id))
    await expect(temporary).toContainText(`"template_id":${ordinary.id}`)
    await expect(temporary).toContainText('"topic":"Example value"')
    await expect(temporary).toContainText('/api/graphics/1/take')
    await expect(temporary).toContainText('/api/graphics/1/out')
    await expect(temporary).toContainText('returns 409')
    await expect(page.getByRole('region', { name: 'Channel models and layers', exact: true })).toContainText(
        'XMLTV logo',
    )
    await expect(
        page
            .getByRole('region', { name: 'Channel models and layers', exact: true })
            .getByRole('link', { name: 'See templates', exact: true }),
    ).toHaveAttribute('href', '/graphics/templates?channel=1')
    await page.setViewportSize({ width: 390, height: 844 })
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true)
    expect(graphicsWrites).toBe(0)

    await page.route('**/api/graphics/1/channel', (route) =>
        route.fulfill({ status: 503, body: 'Tutorial unavailable' }),
    )
    await page.reload()
    await expect(authenticated.getByRole('alert')).toBeVisible()
    await expect(selected).toHaveCount(0)
    await page.unroute('**/api/graphics/1/channel')
    await authenticated.getByRole('button', { name: 'Retry', exact: true }).click()
    await expect(selected).toBeVisible()
    expect(graphicsWrites).toBe(0)
})

test('Composer adds every layer when the secure UUID browser API is unavailable', async ({ page }) => {
    await page.addInitScript(() =>
        Object.defineProperty(crypto, 'randomUUID', { configurable: true, value: undefined }),
    )
    const errors: string[] = []
    page.on('pageerror', (error) => errors.push(error.message))
    await page.goto('/graphics/composer?channel=1')
    expect(await page.evaluate(() => typeof crypto.randomUUID)).toBe('undefined')

    for (const kind of ['Image', 'Text', 'Shape', 'Video in the corner']) {
        await page.getByRole('button', { name: 'Add layer', exact: true }).click()
        await page
            .getByRole('group', { name: 'Layer type', exact: true })
            .getByRole('button', { name: kind, exact: true })
            .click()
        await expect(page.getByLabel('Layer name', { exact: true })).toHaveValue(kind)

        if (kind === 'Image') {
            await page.getByLabel('Image path or field', { exact: true }).fill('00-assets/logo.png')
        }
    }

    const name = `HTTP layers ${Date.now()}`
    await page.getByLabel('Template name', { exact: true }).fill(name)
    await page.getByRole('button', { name: 'Save template', exact: true }).click()
    const response = page.waitForResponse(
        (item) => item.url().endsWith('/api/graphics/1/templates') && item.request().method() === 'POST',
    )
    await page.getByRole('button', { name: 'Save', exact: true }).click()
    const saved = await response
    expect(saved.ok()).toBeTruthy()
    const template = await saved.json()
    expect(template.layers).toHaveLength(4)
    expect(new Set(template.layers.map((layer: { id: string }) => layer.id)).size).toBe(4)
    await expect(page).toHaveURL(new RegExp(`template=${template.id}(?:&|$)`))
    await page.reload()
    await expect(page.getByLabel('Template name', { exact: true })).toHaveValue(name)
    expect(errors).toEqual([])
})

test('the channel logo can explicitly restore its natural size and corner with undo', async ({ page, request }) => {
    await page.goto('/graphics/templates?channel=1')
    const views = await api(page, '/api/graphics/1/channel')
    const logo = views.find((view: { kind: string }) => view.kind === 'logo').template
    const original = structuredClone(logo)
    const token = await page.evaluate(() => localStorage.getItem('token'))
    const headers = { Authorization: `Bearer ${token}` }
    const image = logo.layers.find((layer: { id: string }) => layer.id === logo.control_layer_id)
    Object.assign(image, { x: 0, y: 0, width: 100, height: 100 })
    expect((await request.put(`/api/graphics/1/templates/${logo.id}`, { headers, data: logo })).ok()).toBeTruthy()

    try {
        await page.goto(`/graphics/composer?channel=1&template=${logo.id}`)
        const restore = page.getByRole('button', { name: 'Restore image size and top-right position', exact: true })
        await restore.click()
        await expect(page.getByLabel('Width', { exact: true })).not.toHaveValue('100')
        await page.getByRole('button', { name: 'Undo', exact: true }).first().click()
        await expect(page.getByLabel('Width', { exact: true })).toHaveValue('100')
        await restore.click()
        await expect(page.getByLabel('Width', { exact: true })).not.toHaveValue('100')
        await page.getByRole('button', { name: 'Save template', exact: true }).click()
        await page.getByRole('button', { name: 'Save', exact: true }).click()
        await expect(page.getByRole('status').filter({ hasText: 'saved' })).toBeVisible()
        const saved = await api(page, `/api/graphics/1/templates/${logo.id}`)
        const config = await api(page, '/api/playout/config/1')
        const result = saved.layers.find((layer: { id: string }) => layer.id === saved.control_layer_id)
        expect((result.width * config.output.width) / 100).toBeCloseTo(42, 4)
        expect((result.height * config.output.height) / 100).toBeCloseTo(42, 4)
        expect(((100 - result.x - result.width) * config.output.width) / 100).toBeCloseTo(12, 4)
        expect((result.y * config.output.height) / 100).toBeCloseTo(12, 4)
        await page.getByRole('button', { name: 'Add layer', exact: true }).click()
        await expect(
            page
                .getByRole('group', { name: 'Layer type', exact: true })
                .getByRole('button', { name: 'Video in the corner', exact: true }),
        ).toBeDisabled()
    } finally {
        expect(
            (await request.put(`/api/graphics/1/templates/${logo.id}`, { headers, data: original })).ok(),
        ).toBeTruthy()
    }
})

test('On air uses permanent model controls and the Player only reports graphics state', async ({ page }) => {
    await page.goto('/graphics/onair?channel=1')
    const permanent = page.getByRole('region', { name: 'Always on air', exact: true })
    await expect(permanent).toBeVisible()
    await expect(page.getByRole('heading', { name: 'Live graphics: logo and L-shape', exact: true })).toHaveCount(0)
    await expect(page.getByRole('heading', { name: 'Quick scenes', exact: true })).toHaveCount(0)
    await expect(page.getByRole('button', { name: 'Swap for…', exact: true })).toHaveCount(0)
    const logo = permanent.getByRole('button').filter({ hasText: 'Channel logo' })
    const wasOn = await logo.getAttribute('aria-pressed')
    await logo.click()
    await expect(logo).toHaveAttribute('aria-pressed', wasOn === 'true' ? 'false' : 'true')
    await logo.click()
    await expect(logo).toHaveAttribute('aria-pressed', wasOn!)
    const views = await api(page, '/api/graphics/1/channel')
    const lshape = views.find((view: { kind: string }) => view.kind === 'lshape')
    await page
        .getByRole('complementary', { name: 'Templates', exact: true })
        .getByRole('button')
        .filter({ hasText: 'Channel L-shape' })
        .click()
    const response = page.waitForResponse(
        (item) =>
            item.url().endsWith(`/api/graphics/1/channel/${lshape.template.id}`) && item.request().method() === 'POST',
    )
    await page.getByRole('button', { name: 'Enter L-shape', exact: true }).click()
    expect((await response).ok()).toBeTruthy()
    await page.getByRole('button', { name: 'Return to full screen', exact: true }).click()
    await page.setViewportSize({ width: 390, height: 844 })
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true)
    await page.goto('/player?channel=1')
    const graphics = page.getByRole('region', { name: 'Graphics', exact: true })
    await expect(graphics.getByRole('link', { name: 'Open Graphics', exact: true })).toHaveAttribute(
        'href',
        '/graphics/onair?channel=1',
    )
    await expect(graphics.getByRole('button')).toHaveCount(0)
    await expect(graphics.getByRole('switch')).toHaveCount(0)
})

test('free template names and groups support permanent compositions and deletion', async ({ page }) => {
    await page.goto('/graphics/composer?channel=1')
    await page.getByRole('button', { name: 'Add layer', exact: true }).click()
    await page.getByRole('group', { name: 'Layer type' }).getByRole('button', { name: 'Text', exact: true }).click()
    await page.getByLabel('Text', { exact: true }).fill('Programme information')
    await page.getByLabel('Always on air', { exact: true }).check()
    const name = `Programme graphic ${Date.now()}`
    await page.getByLabel('Template name', { exact: true }).fill(name)
    await page.getByRole('button', { name: 'Save template', exact: true }).click()
    await page.getByLabel('Group', { exact: true }).fill('My programme')
    await expect(page.getByLabel('Control assignment (optional)', { exact: true })).toHaveValue('')
    const posted = page.waitForResponse(
        (response) => response.url().endsWith('/api/graphics/1/templates') && response.request().method() === 'POST',
    )
    await page.getByRole('button', { name: 'Save', exact: true }).click()
    expect((await posted).ok()).toBeTruthy()
    await expect(page.getByRole('status').filter({ hasText: 'saved' })).toBeVisible()
    await page.goto('/graphics/templates?channel=1')
    const group = page.getByRole('region', { name: 'My programme', exact: true })
    const card = group.locator('article').filter({ hasText: name })
    await expect(card.getByRole('heading', { name, exact: true })).toBeVisible()
    await expect(card.getByRole('switch', { name, exact: true })).toBeChecked()
    await card.getByRole('switch', { name, exact: true }).click()
    await expect(card.getByRole('switch', { name, exact: true })).not.toBeChecked()
    await card.getByRole('button', { name: 'Delete template', exact: true }).click()
    const dialog = page.getByRole('dialog', { name: 'Delete this template?', exact: true })
    await expect(dialog).toContainText('Storage images and fonts will be preserved.')
    await dialog.getByRole('button', { name: 'Delete template', exact: true }).click()
    await expect(card).toHaveCount(0)
    await page.reload()
    await expect(page.getByRole('region', { name: 'My programme', exact: true })).toHaveCount(0)
})

test('Composer authors shape primitives and a live server-time clock', async ({ page }) => {
    await page.goto('/graphics/composer?channel=1')
    const add = async (kind: string) => {
        await page.getByRole('button', { name: 'Add layer', exact: true }).click()
        await page
            .getByRole('group', { name: 'Layer type', exact: true })
            .getByRole('button', { name: kind, exact: true })
            .click()
    }
    for (const shape of ['rectangle', 'rounded', 'ellipse', 'line']) {
        await add('Shape')
        await page.getByLabel('Shape type', { exact: true }).selectOption(shape)
        await page.getByLabel('Layer name', { exact: true }).fill(shape)
        await page.getByLabel('Layer name', { exact: true }).press('Tab')
        await page.getByLabel('Stroke colour', { exact: true }).fill('#ffffff')
        await page.getByLabel('Stroke colour', { exact: true }).press('Tab')
        await page.getByLabel('Stroke width (%)', { exact: true }).fill('0.5')
        await page.getByLabel('Stroke width (%)', { exact: true }).press('Tab')
    }
    await add('Clock')
    await page.getByLabel('Clock format', { exact: true }).selectOption('HH:mm:ss')
    await page.getByLabel('Clock timezone', { exact: true }).fill('UTC')
    await page.getByLabel('Clock timezone', { exact: true }).press('Tab')
    const clock = page
        .locator('main span')
        .filter({ hasText: /\d{2}:\d{2}:\d{2}/ })
        .first()
    await expect(clock).toBeVisible()
    const first = await clock.textContent()
    await expect.poll(() => clock.textContent()).not.toBe(first)
    await page.getByLabel('Always on air', { exact: true }).check()
    const name = `Shapes and clock ${Date.now()}`
    await page.getByLabel('Template name', { exact: true }).fill(name)
    await page.getByRole('button', { name: 'Save template', exact: true }).click()
    const saving = page.waitForResponse(
        (r) => r.url().endsWith('/api/graphics/1/templates') && r.request().method() === 'POST',
    )
    await page.getByRole('button', { name: 'Save', exact: true }).click()
    const saved = await saving
    expect(saved.ok()).toBeTruthy()
    const template = await saved.json()
    expect(template.layers.map((layer: { shape: string }) => layer.shape).slice(0, 4)).toEqual([
        'rectangle',
        'rounded',
        'ellipse',
        'line',
    ])
    expect(template.layers[4]).toMatchObject({ kind: 'clock', clock_timezone: 'UTC', clock_format: 'HH:mm:ss' })
    await expect(page).toHaveURL(new RegExp(`template=${template.id}(?:&|$)`))
    await page.reload()
    await expect(page.getByLabel('Template name', { exact: true })).toHaveValue(name)
    await page.setViewportSize({ width: 390, height: 844 })
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true)
})
