const fs = require('node:fs')
const path = require('node:path')
const { chromium, expect } = require('/workspace/NoAr/qa/e2e/node_modules/@playwright/test')

const artifacts = '/workspace/noar-cloud/review-20261109/browser'
const baseURL = 'http://127.0.0.1:5757'

async function main() {
    const browser = await chromium.launch({ executablePath: '/usr/bin/chromium' })
    const context = await browser.newContext({ baseURL, storageState: path.join(artifacts, 'session.json') })
    const page = await context.newPage()
    await page.addInitScript(() => {
        localStorage.setItem('language', 'pt-br')
        localStorage.setItem('theme', 'dark')
    })
    await page.goto('/graphics/templates?channel=1')
    const token = await page.evaluate(() => localStorage.getItem('token'))
    const headers = { Authorization: `Bearer ${token}` }
    const api = context.request
    const originalsResponse = await api.get('/api/graphics/1/channel', { headers })
    expect(originalsResponse.ok()).toBeTruthy()
    const originals = await originalsResponse.json()
    const canonicalized = []
    const evidence = { fixture: 'Existing compatibility models temporarily restored to raw conversion-default names for localization screenshots', locale: 'pt-br', theme: 'dark', captures: [], templates: [] }

    async function canonical(role) {
        const model = originals.find(model => model.kind === role)
        expect(model, `Missing compatibility role ${role}`).toBeTruthy()
        const template = { ...model.template, name: role, group: 'Channel graphics', layers: model.template.layers.map(layer => ({ ...layer })) }
        for (const layer of template.layers) {
            if (layer.id === 'image' && layer.kind === 'image') layer.name = 'Image'
            if (layer.id === 'picture' && layer.kind === 'video') layer.name = 'Picture'
        }
        const response = await api.put(`/api/graphics/1/templates/${template.id}`, { headers, data: template })
        expect(response.ok(), await response.text()).toBeTruthy()
        canonicalized.push(model)
        return response.json()
    }

    async function capture(name, width, screen, template) {
        await page.setViewportSize({ width, height: width === 390 ? 844 : 1000 })
        await page.goto(`/graphics/${screen}?channel=1${template ? `&template=${template.id}` : ''}`)
        if (screen === 'composer') {
            await expect(page.getByLabel('Nome do modelo', { exact: true })).toHaveValue(template.name.startsWith('logo') ? 'Logo do canal' : 'L-shape do canal')
            await expect(page.getByLabel('Nome da camada', { exact: true })).toHaveValue('Imagem')
            if (template.name.startsWith('lshape')) {
                await page.getByRole('button', { name: 'Selecionar a camada Vídeo no canto', exact: true }).click()
                await expect(page.getByLabel('Nome da camada', { exact: true })).toHaveValue('Vídeo no canto')
            }
        } else if (screen === 'templates') {
            await page.getByRole('button', { name: 'Channel graphics', exact: true }).click()
            await expect(page.getByRole('heading', { name: 'Logo do canal', exact: true })).toBeVisible()
            await expect(page.getByRole('heading', { name: 'L-shape do canal', exact: true })).toBeVisible()
        } else {
            await expect(page.locator('figcaption').first()).toContainText('L-shape do canal')
        }
        await expect(page.locator('.graphics-preview').first()).toBeVisible()
        await expect(page.locator('vite-error-overlay')).toHaveCount(0)
        const images = page.locator('.graphics-preview img')
        if (await images.count()) {
            await expect.poll(() => images.evaluateAll(items => items.every(item => item.complete && item.naturalWidth > 0))).toBe(true)
        }
        const overflow = await page.evaluate(() => document.documentElement.scrollWidth > innerWidth)
        expect(overflow).toBe(false)
        await page.evaluate(() => window.scrollTo(0, 0))
        const filename = `${name}-${width}-pt-BR.jpg`
        await page.screenshot({ path: path.join(artifacts, filename), type: 'jpeg', quality: 88, fullPage: true })
        evidence.captures.push({ filename, width, horizontal_overflow: overflow })
    }

    try {
        const logo = await canonical('logo')
        const lshape = await canonical('lshape')
        if (originals.some(model => model.kind === 'ticker')) await canonical('ticker')
        for (const width of [1440, 390]) {
            await capture('converted-models', width, 'templates')
            await capture('converted-logo-composer', width, 'composer', logo)
            await capture('converted-lshape-composer', width, 'composer', lshape)
            await capture('converted-lshape-onair', width, 'onair', lshape)
        }
        for (const template of [logo, lshape]) {
            const response = await api.get(`/api/graphics/1/templates/${template.id}`, { headers })
            const saved = await response.json()
            expect(saved.name).toBe(template.name)
            expect(saved.layers.map(layer => [layer.id, layer.name])).toEqual(template.layers.map(layer => [layer.id, layer.name]))
            evidence.templates.push({ id: saved.id, raw_name: saved.name, layers: saved.layers.map(({ id, name, kind }) => ({ id, name, kind })) })
        }
        evidence.status = 'PASS'
        fs.writeFileSync(path.join(artifacts, 'converted-captures.json'), JSON.stringify(evidence, null, 2) + '\n')
        console.log(JSON.stringify({ status: evidence.status, captures: evidence.captures.map(item => item.filename) }))
    } finally {
        for (const model of canonicalized) {
            const restore = await api.put(`/api/graphics/1/templates/${model.template.id}`, { headers, data: model.template })
            expect(restore.ok(), await restore.text()).toBeTruthy()
        }
        await context.close()
        await browser.close()
    }
}
main().catch(error => { console.error(error.stack); process.exitCode = 1 })
