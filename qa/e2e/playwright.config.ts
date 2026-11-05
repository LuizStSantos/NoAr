import { defineConfig, devices } from '@playwright/test'

if (process.env.NOAR_E2E_STAGING !== '1') {
  throw new Error('Refusing E2E execution: set NOAR_E2E_STAGING=1 for a disposable staging instance.')
}

const baseURL = process.env.NOAR_E2E_BASE_URL || 'http://127.0.0.1:8787'
const target = new URL(baseURL)
const loopback = ['127.0.0.1', 'localhost', '::1', '[::1]'].includes(target.hostname)
if (!['http:', 'https:'].includes(target.protocol)) {
  throw new Error('E2E target must be HTTP(S).')
}
if (!loopback && process.env.NOAR_E2E_ALLOW_REMOTE !== '1') {
  throw new Error('Non-loopback E2E target rejected. Use NOAR_E2E_ALLOW_REMOTE=1 only for verified staging.')
}

export default defineConfig({
  globalSetup: './global-setup.ts',
  testDir: './tests',
  fullyParallel: false,
  forbidOnly: true,
  retries: 0,
  workers: 1,
  timeout: 30_000,
  reporter: [['list'], ['html', { open: 'never', outputFolder: './playwright-report' }]],
  use: {
    baseURL,
    locale: 'en-US',
    timezoneId: 'America/Sao_Paulo',
    viewport: { width: 1440, height: 1000 },
    launchOptions: process.env.NOAR_E2E_CHROMIUM
      ? { executablePath: process.env.NOAR_E2E_CHROMIUM }
      : {},
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
  outputDir: './test-results',
})
