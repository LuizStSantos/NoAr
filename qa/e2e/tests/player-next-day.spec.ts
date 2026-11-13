import { test, expect, type Page } from '@playwright/test'

// Read-only: the playlists of the day on air and of the next day, and the
// playout status stream, are answered by the test; nothing is saved. The
// name lookup of the programs is the real one.
test.skip(process.env.NOAR_E2E_WRITES !== '1', 'Run with run_staging.py for disposable authenticated tests.')
test.use({ storageState: process.env.NOAR_E2E_STATE_PATH })

async function read(page: Page, path: string) {
    const token = await page.evaluate(() => localStorage.getItem('token'))
    const response = await page.request.get(path, { headers: { Authorization: `Bearer ${token}` } })
    expect(response.ok(), `${path}: ${response.status()}`).toBeTruthy()

    return response.json()
}

/** The day whose playlist is on air: the day before, before the day start. */
function onAirDate(timezone: string, dayStart: string): string {
    const parts = Object.fromEntries(
        new Intl.DateTimeFormat('en-CA', {
            timeZone: timezone,
            year: 'numeric',
            month: '2-digit',
            day: '2-digit',
            hour: '2-digit',
            minute: '2-digit',
            second: '2-digit',
            hourCycle: 'h23',
        })
            .formatToParts(new Date())
            .map((part) => [part.type, part.value]),
    )
    const [h, m, s] = dayStart.split(':').map(Number)
    const now = Number(parts.hour) * 3600 + Number(parts.minute) * 60 + Number(parts.second)
    const today = new Date(`${parts.year}-${parts.month}-${parts.day}T12:00:00Z`)

    if (now < (h ?? 0) * 3600 + (m ?? 0) * 60 + (s ?? 0)) {
        today.setUTCDate(today.getUTCDate() - 1)
    }

    return today.toISOString().slice(0, 10)
}

function dayAfter(date: string): string {
    const day = new Date(`${date}T12:00:00Z`)
    day.setUTCDate(day.getUTCDate() + 1)

    return day.toISOString().slice(0, 10)
}

test('the last program of the day is followed by the first one of the next day', async ({ page }) => {
    await page.goto('/')
    const config = await read(page, '/api/playout/config/1')
    const channel = await read(page, '/api/channel/1')
    const today = onAirDate(channel.timezone || 'UTC', config.playlist.day_start || '00:00:00')
    const tomorrow = dayAfter(today)
    const storage = process.env.NOAR_E2E_STORAGE ?? '/e2e-data/media'
    const clip = (name: string, title: string) => ({
        in: 0,
        out: 1800,
        duration: 1800,
        source: `${storage}/${name}.mp4`,
        title,
    })
    // One program in two parts, the second on air: nothing of the day after it.
    const onAirDay = [clip('e2e-last-part-1', 'E2E last program'), clip('e2e-last-part-2', 'E2E last program')]
    const nextDay = [
        clip('e2e-opening', 'E2E first program of the next day'),
        clip('e2e-second', 'E2E second program of the next day'),
    ]

    await page.route(/\/api\/playlist\/1\?date=/, async (route) => {
        const date = new URL(route.request().url()).searchParams.get('date')
        const program = date === today ? onAirDay : date === tomorrow ? nextDay : null

        if (route.request().method() !== 'GET' || !program) {
            return route.continue()
        }

        return route.fulfill({ json: { channel: 'Channel 1', date, program } })
    })

    const status = {
        media: onAirDay[1],
        index: 1,
        elapsed: 12,
        shift: 0,
        mode: 'playlist',
        uptime: 600,
    }
    await page.route(/\/data\/event\/1\?/, (route) =>
        route.fulfill({
            status: 200,
            headers: { 'content-type': 'text/event-stream', 'cache-control': 'no-cache' },
            body: `data: ${JSON.stringify(status)}\n\n`,
        }),
    )

    await page.goto('/player?channel=1')
    const now = page.getByRole('region', { name: 'Now', exact: true })
    await expect(now).toContainText('E2E last program')
    await expect(now).toContainText('clip 2 of 2')

    // The box around the "Up next" label.
    const upNext = now.getByText('Up next', { exact: true }).locator('xpath=..')
    await expect(upNext).toContainText('E2E first program of the next day')
    await expect(upNext).not.toContainText('E2E second program of the next day')

    await page.unroute(/\/api\/playlist\/1\?date=/)
    await page.unroute(/\/data\/event\/1\?/)
})
