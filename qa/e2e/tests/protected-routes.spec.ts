import { test, expect } from '@playwright/test'

// Regressions for the public/auth route boundary; no real login or writes.
// Playwright config requires explicit disposable staging opt-in.
for (const route of ['/player', '/playlist', '/schedule', '/graphics', '/media', '/report', '/configure']) {
  test(`anonymous visitor cannot view protected ${route}`, async ({ page }) => {
    await page.goto(route)
    await expect(page).toHaveURL(/\/login(?:\?|$)/)
    await expect(page.locator('input[name="password"]')).toBeVisible()
  })
}
