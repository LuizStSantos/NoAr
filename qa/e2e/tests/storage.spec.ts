import { readFile } from 'node:fs/promises'
import { resolve } from 'node:path'
import { test, expect, type Locator, type Page } from '@playwright/test'

test.skip(process.env.NOAR_E2E_WRITES !== '1', 'Run against an owned disposable installation.')
test.use({ storageState: process.env.NOAR_E2E_STATE_PATH })

async function expectOriginalDownload(page: Page, button: Locator, name: string, original: Buffer) {
    const started = page.waitForEvent('download')
    await button.click()
    const download = await started
    expect(download.suggestedFilename()).toBe(name)
    const saved = await download.path()
    expect(saved).not.toBeNull()
    expect(await readFile(saved!)).toEqual(original)
}

test('Storage downloads original video, image and WebVTT files and recovers from denied access tickets', async ({
    page,
}) => {
    const stamp = Date.now()
    const videoName = `download-${stamp}.mp4`
    const imageName = `download-${stamp}.png`
    const subtitleName = `download captions café-${stamp}.vtt`
    const video = await readFile(resolve('../../tests_assets/storage/media_mix/ad.mp4'))
    const image = await readFile(resolve('../../tests_assets/storage/logo.png'))
    const subtitle = Buffer.from('WEBVTT\n\n00:00:01.000 --> 00:00:04.000\nSample caption with UTF-8: café.\n')
    await page.goto('/media')
    const token = await page.evaluate(() => localStorage.getItem('token'))
    const headers = { Authorization: `Bearer ${token}` }
    const currentConfig = await page.request.get('/api/playout/config/1', { headers })
    expect(currentConfig.ok()).toBeTruthy()
    const config = await currentConfig.json()
    const convertUploads = config.storage.convert_uploads
    config.storage.convert_uploads = false
    expect((await page.request.put('/api/playout/config/1', { headers, data: config })).ok()).toBeTruthy()
    const upload = page.locator('input[type=file]')
    await expect(upload).toHaveAttribute('accept', /\.vtt(?:,|$)/)
    await upload.setInputFiles([
        { name: videoName, mimeType: 'video/mp4', buffer: video },
        { name: imageName, mimeType: 'image/png', buffer: image },
        { name: subtitleName, mimeType: 'text/vtt', buffer: subtitle },
    ])
    await page.getByRole('button', { name: 'All files', exact: true }).click()
    const videoButton = page.getByRole('button', { name: `Download ${videoName}`, exact: true })
    const subtitleButton = page.getByRole('button', { name: `Download ${subtitleName}`, exact: true })
    await expect(videoButton).toBeVisible()
    await expect(subtitleButton).toBeVisible()
    await expectOriginalDownload(page, videoButton, videoName, video)
    await expectOriginalDownload(page, subtitleButton, subtitleName, subtitle)
    await page.getByRole('button', { name: /^Images(?:\s|$)/ }).click()
    await expectOriginalDownload(
        page,
        page.getByRole('button', { name: `Download ${imageName}`, exact: true }),
        imageName,
        image,
    )
    await page
        .getByRole('button', { name: `View ${imageName}`, exact: true })
        .first()
        .click()
    const viewer = page.getByRole('dialog')
    await expectOriginalDownload(page, viewer.getByRole('button', { name: 'Download', exact: true }), imageName, image)
    await viewer.getByRole('button', { name: 'Close', exact: true }).click()
    await page.getByRole('button', { name: 'All files', exact: true }).click()
    await page.setViewportSize({ width: 390, height: 844 })
    await expectOriginalDownload(page, subtitleButton, subtitleName, subtitle)
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true)
    await page.route('**/api/file/1/access-token', (route) =>
        route.fulfill({ status: 503, body: 'Download temporarily unavailable' }),
    )
    await videoButton.click()
    await expect(page.getByText(/Download temporarily unavailable/)).toBeVisible()
    await expect(videoButton).toBeEnabled()
    await page.unroute('**/api/file/1/access-token')
    await expectOriginalDownload(page, videoButton, videoName, video)
    for (const name of [videoName, imageName, subtitleName]) {
        const removed = await page.request.post('/api/file/1/remove', {
            headers: { Authorization: `Bearer ${token}` },
            data: { source: `/${name}`, recursive: false },
        })
        expect(removed.ok()).toBeTruthy()
    }
    config.storage.convert_uploads = convertUploads
    expect((await page.request.put('/api/playout/config/1', { headers, data: config })).ok()).toBeTruthy()
})

test('Storage previews the graphics cued in a video over its player at their times', async ({ page }) => {
    const stamp = Date.now()
    const videoName = `cues-${stamp}.mp4`
    const video = await readFile(resolve('../../tests_assets/storage/media_mix/ad.mp4'))
    await page.goto('/media')
    const token = await page.evaluate(() => localStorage.getItem('token'))
    const headers = { Authorization: `Bearer ${token}` }
    const config = await (await page.request.get('/api/playout/config/1', { headers })).json()
    const convertUploads = config.storage.convert_uploads
    config.storage.convert_uploads = false
    expect((await page.request.put('/api/playout/config/1', { headers, data: config })).ok()).toBeTruthy()
    const created = await page.request.post('/api/graphics/1/templates', {
        headers,
        data: {
            name: `Cue preview ${stamp}`,
            duration: 0,
            layers: [{ id: 'title', kind: 'text', text: 'CUE PREVIEW', x: 5, y: 70, width: 60, height: 10 }],
        },
    })
    expect(created.ok()).toBeTruthy()
    const template = await created.json()

    try {
        await page
            .locator('input[type=file]')
            .setInputFiles([{ name: videoName, mimeType: 'video/mp4', buffer: video }])
        const open = page.getByRole('button', {
            name: `Trim, graphics and breaks in this video: ${videoName}`,
            exact: true,
        })
        await expect(open).toBeVisible()
        const saved = await page.request.put('/api/file/1/logo-marks', {
            headers,
            data: {
                path: videoName,
                graphics: [
                    { at: 2, action: 'take', template_id: template.id, duration: 3 },
                    { at: 10, action: 'take', template_id: template.id, duration: 0 },
                    { at: 15, action: 'out', template_id: template.id },
                ],
            },
        })
        expect(saved.ok(), await saved.text()).toBeTruthy()
        await open.click()
        const player = page.locator('video').first()
        await expect.poll(() => player.evaluate((element: HTMLVideoElement) => element.readyState)).toBeGreaterThan(0)
        const overlay = page.getByTestId('graphics-cues-overlay')

        for (const [seconds, shown] of [
            [1, false],
            [3, true],
            [6, false],
            [12, true],
            [16, false],
        ] as const) {
            await player.evaluate((element: HTMLVideoElement, time) => {
                element.currentTime = time
            }, seconds)
            if (shown) {
                await expect(overlay, `${seconds} s`).toContainText('CUE PREVIEW')
            } else {
                await expect(overlay, `${seconds} s`).toHaveCount(0)
            }
        }
    } finally {
        await page.request.delete(`/api/graphics/1/templates/${template.id}`, { headers })
        config.storage.convert_uploads = convertUploads
        await page.request.put('/api/playout/config/1', { headers, data: config })
    }
})
