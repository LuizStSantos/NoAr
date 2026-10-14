/** Helpers shared by the player screens. */

/** What a library file carries when it is dragged onto the playlist. */
export interface LibraryFile {
    /** Absolute path, as playlists store it. */
    source: string
    name: string
    duration: number
}

export const LIBRARY_DRAG_TYPE = 'application/x-noar-media'

/** Rows of the playlist dragged to another place. */
export const ROW_DRAG_TYPE = 'application/x-noar-row'

const GUIDE_IMAGE_STEMS = [
    'poster',
    'folder',
    'cover',
    'capa',
    'show',
    'landscape',
    'thumb',
    'fanart',
    'backdrop',
    'background',
]
const IMAGE_EXTENSIONS = ['jpg', 'jpeg', 'png', 'webp', 'gif', 'bmp', 'tbn']

/** `HH:MM:SS` of seconds since midnight; playlists may run past 24 h. */
export function clockTime(seconds: number): string {
    const total = Math.max(0, Math.floor(seconds)) % 86400
    const h = Math.floor(total / 3600)
    const m = Math.floor((total % 3600) / 60)
    const s = total % 60

    return [h, m, s].map((part) => String(part).padStart(2, '0')).join(':')
}

/** `HH:MM` of seconds since midnight. */
export function clockShort(seconds: number): string {
    return clockTime(seconds).slice(0, 5)
}

/** `0:30`, `12:05`, `1:02:05` for lengths. */
export function lengthLabel(seconds: number): string {
    const total = Math.max(0, Math.round(seconds))
    const h = Math.floor(total / 3600)
    const m = Math.floor((total % 3600) / 60)
    const s = String(total % 60).padStart(2, '0')

    return h > 0 ? `${h}:${String(m).padStart(2, '0')}:${s}` : `${m}:${s}`
}

/** `24:00:00` style total length, hours not wrapped. */
export function totalLength(seconds: number): string {
    const total = Math.max(0, Math.round(seconds))
    const h = Math.floor(total / 3600)
    const m = Math.floor((total % 3600) / 60)
    const s = total % 60

    return [h, m, s].map((part) => String(part).padStart(2, '0')).join(':')
}

/** `0:30.0`, `1:02:05.5`: a position with tenths, as the clip editor shows it. */
export function preciseLength(seconds: number): string {
    const value = Math.max(0, seconds)
    const tenths = Math.round(value * 10) % 10
    const whole = Math.floor(Math.round(value * 10) / 10)

    return `${lengthLabel(whole)}.${tenths}`
}

/** Parse `h:mm:ss.d`, `m:ss.d` or plain seconds; `null` when it is not a time. */
export function parseLength(text: string): number | null {
    const value = text.trim().replace(',', '.')

    if (!/^\d+(:\d{1,2}){0,2}(\.\d+)?$/.test(value)) {
        return null
    }

    return value
        .split(':')
        .map(Number)
        .reduce((total, part) => total * 60 + part, 0)
}

export function fileName(path: string): string {
    return path.split('/').pop() ?? path
}

/** A storage path shown as `Folder / file.mp4`, relative to the storage root. */
export function storageLabel(source: string, storage: string): string {
    const root = storage.replace(/\/+$/, '')
    const relative = root && source.startsWith(`${root}/`) ? source.slice(root.length + 1) : source

    return relative.split('/').filter(Boolean).join(' / ')
}

export function isRemoteSource(source: string): boolean {
    return /^(https?|rtmps?|srt|udp|rtp|rtsp|tcp):\/\//i.test(source.trim())
}

/** Posters, backdrops and `.nfo` files describe programs for the guide; they are not played. */
export function isGuideAsset(name: string): boolean {
    const lower = name.toLowerCase()
    const extension = lower.split('.').pop() ?? ''

    if (extension === 'nfo') {
        return true
    }

    if (!IMAGE_EXTENSIONS.includes(extension)) {
        return false
    }

    const stem = lower.slice(0, -(extension.length + 1))

    return GUIDE_IMAGE_STEMS.includes(stem) || /-(thumb|poster|fanart|landscape)$/.test(stem)
}

/** `3 d 04 h`, `4 h 12 min`, `12 min`. */
export function uptimeLabel(seconds: number): string {
    const days = Math.floor(seconds / 86400)
    const hours = Math.floor((seconds % 86400) / 3600)
    const minutes = Math.floor((seconds % 3600) / 60)

    if (days > 0) {
        return `${days} d ${String(hours).padStart(2, '0')} h`
    }

    if (hours > 0) {
        return `${hours} h ${String(minutes).padStart(2, '0')} min`
    }

    return `${Math.max(1, minutes)} min`
}

/** Level in dB as a meter width in percent, -60 dB to 0 dB. */
export function meterPercent(db: number | null | undefined): number {
    if (db === null || db === undefined || !Number.isFinite(db)) {
        return 0
    }

    return Math.min(100, Math.max(0, ((db + 60) / 60) * 100))
}
