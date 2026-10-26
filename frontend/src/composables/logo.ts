import { createFilePreviewUrl } from '@/composables/fileAccess'

export interface Size {
    width: number
    height: number
}

/** Where the logo sits in the output picture, in percent of its width and height. */
export interface LogoBox {
    left: number
    top: number
    width: number
    height: number
}

/** Round down to an even number, like the playout does for picture sizes. */
function even(value: number): number {
    return Math.floor(value / 2) * 2
}

/** One side of a `WIDTH:HEIGHT` logo scale; `null` keeps the image's proportion. */
function dimension(value: string, input: number, output: number): number | null {
    const text = value.trim()

    if (text === '-1') return null
    if (text === 'iw' || text === 'ih') return input
    if (['W', 'H', 'main_w', 'main_h'].includes(text)) return output

    if (text.endsWith('%')) {
        const percent = Number(text.slice(0, -1).trim())

        if (!Number.isFinite(percent) || percent <= 0) throw new Error(`logo scale ${value}`)

        return Math.round((output * percent) / 100)
    }

    if (!/^\d+$/.test(text)) throw new Error(`logo scale ${value}`)

    return Number(text)
}

/** The logo's size in output pixels for its scale setting (as the playout sizes it). */
export function logoSize(scale: string, image: Size, frame: Size): Size {
    const trimmed = scale.trim()

    if (!trimmed) {
        return { width: Math.max(2, even(image.width)), height: Math.max(2, even(image.height)) }
    }

    const separator = trimmed.includes(':') ? ':' : 'x'
    const [w, h] = trimmed.split(separator)

    if (w === undefined || h === undefined) throw new Error(`logo scale ${scale}`)

    let width = dimension(w, image.width, frame.width)
    let height = dimension(h, image.height, frame.height)

    if (width !== null && height === null) height = Math.floor((width * image.height) / image.width)
    if (width === null && height !== null) width = Math.floor((height * image.width) / image.height)

    return {
        width: Math.max(2, even(width ?? image.width)),
        height: Math.max(2, even(height ?? image.height)),
    }
}

/** One axis of a position such as `W-w-12`: `main` is the picture, `overlay` the logo. */
function axis(expr: string, main: number, overlay: number): number {
    const normalized = expr
        .replaceAll('main_w', 'M')
        .replaceAll('main_h', 'M')
        .replaceAll('overlay_w', 'O')
        .replaceAll('overlay_h', 'O')
        .replace(/[WH]/g, 'M')
        .replace(/[wh]/g, 'O')
        .replaceAll('-', '+-')
    let total = 0

    for (const raw of normalized.split('+')) {
        let part = raw.trim()

        if (!part) continue

        const sign = part.startsWith('-') ? -1 : 1

        if (sign < 0) part = part.slice(1)

        const [term = '', divisorText] = part.split('/')
        const divisor = divisorText === undefined ? 1 : Number(divisorText.trim())

        if (!Number.isInteger(divisor) || divisor <= 0) throw new Error(`logo position ${expr}`)

        const name = term.trim()
        const value = name === 'M' ? main : name === 'O' ? overlay : Number(name)

        if (!Number.isFinite(value)) throw new Error(`logo position ${expr}`)

        total += sign * Math.trunc(value / divisor)
    }

    return total
}

/**
 * Where the logo lands in a `frame`-sized output, in percent, worked out as
 * the playout does from the scale and position settings and the image size.
 * `null` when a setting is not understood.
 */
export function logoBox(scale: string, position: string, image: Size, frame: Size): LogoBox | null {
    try {
        const size = logoSize(scale, image, frame)
        const [x, y] = position.split(':')

        if (x === undefined || y === undefined) return null

        const clamp = (value: number, room: number) => Math.min(Math.max(value, 0), Math.max(room, 0))
        const left = even(clamp(axis(x, frame.width, size.width), frame.width - size.width))
        const top = even(clamp(axis(y, frame.height, size.height), frame.height - size.height))

        return {
            left: (left / frame.width) * 100,
            top: (top / frame.height) * 100,
            width: (size.width / frame.width) * 100,
            height: (size.height / frame.height) * 100,
        }
    } catch {
        return null
    }
}

/** CSS that places an element on `box`. */
export function logoBoxStyle(box: LogoBox): Record<string, string> {
    return {
        left: `${box.left}%`,
        top: `${box.top}%`,
        width: `${box.width}%`,
        height: `${box.height}%`,
    }
}

/**
 * A URL that shows the storage image at `path` (relative to the channel's
 * storage, or absolute inside it), or '' when there is none.
 */
export async function storageImageUrl(
    channel: { id: number; storage: string } | undefined,
    path: string | undefined,
): Promise<string> {
    const storage = channel?.storage ?? ''
    const image = path?.trim()

    if (!image || !storage) {
        return ''
    }

    const absolute = image.startsWith('/') ? image : `${storage.replace(/\/+$/, '')}/${image}`
    const parent = storage.slice(0, storage.lastIndexOf('/'))

    if (!absolute.startsWith(storage)) {
        throw new Error('outside the storage')
    }

    return createFilePreviewUrl(channel?.id, absolute.slice(parent.length))
}
