import { expect, type Page } from '@playwright/test'

/** A successful Composer save returns to the channel's templates. */
export async function expectBackToTemplates(page: Page, channel = 1) {
    await expect(page).toHaveURL(new RegExp(`/graphics/templates\\?channel=${channel}$`))
}

/** After a save, open the saved template again to keep editing it. */
export async function reopenSaved(page: Page, id: number, channel = 1) {
    await expectBackToTemplates(page, channel)
    await page.goto(`/graphics/composer?channel=${channel}&template=${id}`)
    await expect(page.getByRole('button', { name: /^(Save template|Salvar modelo)$/ })).toBeEnabled()
}
