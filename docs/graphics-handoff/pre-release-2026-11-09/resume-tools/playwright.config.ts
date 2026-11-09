import base from '/workspace/NoAr/qa/e2e/playwright.config.ts'

export default {
    ...base,
    globalSetup: '/workspace/NoAr/qa/e2e/global-setup.ts',
    testDir: '/workspace/NoAr/qa/e2e/tests',
    timeout: 60_000,
    outputDir: '/workspace/noar-cloud/review-20261109/browser/test-results',
    reporter: [
        ['list'],
        ['json', { outputFile: '/workspace/noar-cloud/review-20261109/browser/results.json' }],
        ['html', { open: 'never', outputFolder: '/workspace/noar-cloud/review-20261109/browser/playwright-report' }],
    ],
}
