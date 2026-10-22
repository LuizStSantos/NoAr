import { test, expect } from '@playwright/test'

test('login shows credentials fields and password toggle works', async ({ page }) => {
  await page.goto('/login')
  const user = page.locator('input[name="username"]')
  const password = page.locator('input[name="password"]')
  await expect(user).toBeVisible()
  await expect(password).toBeVisible()
  await expect(password).toHaveAttribute('type', 'password')
  // LoginView.vue places the icon button next to the password input in one span.
  await password.locator('..').locator('button[type="button"]').click()
  await expect(password).toHaveAttribute('type', 'text')
})

test('protected schedule page sends unauthenticated user to login', async ({ page }) => {
  await page.goto('/schedule')
  await expect(page).toHaveURL(/\/login(?:\?|$)/)
  await expect(page.locator('input[name="username"]')).toBeVisible()
})
