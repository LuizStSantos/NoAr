import { test, expect } from '@playwright/test'

test.skip(process.env.NOAR_E2E_WRITES !== '1', 'Run against an owned disposable installation.')
test.use({ storageState: process.env.NOAR_E2E_STATE_PATH })

test('Storage folder dialogs keep create, cancel, rename, move and delete behavior', async ({ page }) => {
    const stamp = Date.now()
    const source = `maintenance-source-${stamp}`
    const renamed = `maintenance-renamed-${stamp}`
    const destination = `maintenance-destination-${stamp}`
    await page.goto('/media')

    for (const name of [source, destination]) {
        await page.getByRole('button', { name: 'New folder', exact: true }).click()
        const dialog = page.getByRole('dialog', { name: 'New folder', exact: true })
        await dialog.getByRole('textbox', { name: 'Folder name', exact: true }).fill(name)
        await dialog.getByRole('button', { name: 'Ok', exact: true }).click()
        await expect(page.getByRole('button', { name: `Open ${name}`, exact: true })).toBeVisible()
    }

    await page.getByRole('button', { name: `Rename ${source}`, exact: true }).click()
    let dialog = page.getByRole('dialog', { name: 'Rename', exact: true })
    await dialog.getByRole('textbox', { name: 'New name', exact: true }).fill(renamed)
    await dialog.getByRole('button', { name: 'Cancel', exact: true }).last().click()
    await expect(page.getByRole('button', { name: `Open ${source}`, exact: true })).toBeVisible()
    await expect(page.getByRole('button', { name: `Open ${renamed}`, exact: true })).toHaveCount(0)

    await page.getByRole('button', { name: `Rename ${source}`, exact: true }).click()
    dialog = page.getByRole('dialog', { name: 'Rename', exact: true })
    await dialog.getByRole('textbox', { name: 'New name', exact: true }).fill(renamed)
    await dialog.getByRole('button', { name: 'Ok', exact: true }).click()
    await expect(page.getByRole('button', { name: `Open ${renamed}`, exact: true })).toBeVisible()

    await page.getByRole('button', { name: `Move ${renamed}`, exact: true }).click()
    dialog = page.getByRole('dialog', { name: 'Move 1 item', exact: true })
    await dialog.getByRole('button', { name: destination, exact: true }).click()
    await dialog.getByRole('button', { name: 'Move here', exact: true }).click()
    await expect(page.getByRole('button', { name: `Open ${renamed}`, exact: true })).toHaveCount(0)
    await page.getByRole('button', { name: `Open ${destination}`, exact: true }).click()
    await expect(page.getByRole('button', { name: `Open ${renamed}`, exact: true })).toBeVisible()
    // Folder selection is local navigation; use its existing bookmark URL on reload.
    await page.goto(`/media?path=${encodeURIComponent(destination)}`)
    await page.reload()
    await expect(page.getByRole('button', { name: `Open ${renamed}`, exact: true })).toBeVisible()

    await page.getByRole('button', { name: `Delete ${renamed}`, exact: true }).click()
    dialog = page.getByRole('dialog', { name: 'Delete', exact: true })
    await dialog.getByRole('button', { name: 'Delete', exact: true }).click()
    await expect(page.getByRole('button', { name: `Open ${renamed}`, exact: true })).toHaveCount(0)
    await page.goto('/media')
    await page.getByRole('button', { name: `Delete ${destination}`, exact: true }).click()
    await page
        .getByRole('dialog', { name: 'Delete', exact: true })
        .getByRole('button', { name: 'Delete', exact: true })
        .click()
    await expect(page.getByRole('button', { name: `Open ${destination}`, exact: true })).toHaveCount(0)
})

test('Report loading errors clear stale export data and recover after changing the day', async ({ page }) => {
    await page.goto('/report')
    const spreadsheet = page.getByRole('button', { name: /Download spreadsheet/ })
    await expect(spreadsheet).toBeEnabled()
    await page.route('**/api/report/1?*', (route) =>
        route.fulfill({ status: 503, contentType: 'text/plain', body: 'Report temporarily unavailable' }),
    )
    await page.getByRole('button', { name: 'Next day', exact: true }).click()
    await expect(page.getByRole('alert')).toContainText('Report temporarily unavailable')
    await expect(spreadsheet).toBeDisabled()
    await page.unroute('**/api/report/1?*')
    await page.getByRole('button', { name: 'Previous day', exact: true }).click()
    await expect(spreadsheet).toBeEnabled()
})
