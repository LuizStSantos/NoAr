import { test, expect } from '@playwright/test'

// This is a read-only API contract test against disposable staging.
test('setup endpoint exposes expected public contract', async ({ request }) => {
  const response = await request.get('/api/setup')
  expect(response.status()).toBe(200)
  const data: unknown = await response.json()
  expect(data).toBeTruthy()
  expect(typeof data).toBe('object')
  expect(typeof (data as Record<string, unknown>).required).toBe('boolean')
})
