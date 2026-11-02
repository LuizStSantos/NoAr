/**
 * DeckLink video modes by the names the cards use (`1080i5994`, `720p50`,
 * `ntsc`…): picture size and the whole-frame rate the channel renders at.
 */
export interface VideoModeInfo {
    width: number
    height: number
    /** Frames per second, such as 29.97 for `1080i5994` and `1080p2997`. */
    fps: number
    interlaced: boolean
}

const SIZES: Record<string, [number, number]> = {
    '720': [1280, 720],
    '1080': [1920, 1080],
    '2160': [3840, 2160],
}

const RATES: Record<string, number> = {
    '2398': 23.976,
    '24': 24,
    '25': 25,
    '2997': 29.97,
    '30': 30,
    '50': 50,
    '5994': 59.94,
    '60': 60,
}

export function videoModeInfo(name: string): VideoModeInfo | null {
    if (name === 'ntsc') {
        return { width: 720, height: 486, fps: 29.97, interlaced: true }
    }
    if (name === 'pal') {
        return { width: 720, height: 576, fps: 25, interlaced: true }
    }

    const match = /^(720|1080|2160)([pi])(\d+)$/.exec(name)
    const size = match ? SIZES[match[1]!] : undefined
    const rate = match ? RATES[match[3]!] : undefined

    if (!match || !size || rate === undefined) {
        return null
    }

    const interlaced = match[2] === 'i'
    // An interlaced mode is named by its field rate: 59.94i is 29.97 frames.
    const fps = interlaced ? Math.round((rate / 2) * 1000) / 1000 : rate

    return { width: size[0], height: size[1], fps, interlaced }
}

/** `1080i5994` as `1080i 59.94`, the way broadcast engineers read it. */
export function videoModeLabel(name: string): string {
    const info = videoModeInfo(name)

    if (!info) {
        return name
    }
    if (name === 'ntsc' || name === 'pal') {
        return name.toUpperCase()
    }

    const rate = info.interlaced ? info.fps * 2 : info.fps

    return `${info.height}${info.interlaced ? 'i' : 'p'} ${Number(rate.toFixed(2))}`
}

/** The card and mode of a `decklink://N?mode=M` address. */
export function parseDeckLink(address: string): { device: string; mode: string } {
    const match = /^decklink:\/\/(\d+)(?:\?(.*))?$/i.exec(address.trim())
    const params = new URLSearchParams(match?.[2] ?? '')

    return { device: match?.[1] ?? '0', mode: params.get('mode') ?? '1080i5994' }
}
