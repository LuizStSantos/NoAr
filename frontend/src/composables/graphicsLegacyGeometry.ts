/** Compatibility geometry for clip marks with legacy scale/position expressions. */
export interface Size {
    width: number
    height: number
}

/** Where the logo sits in the output picture, in percent of its width and height. */
export interface LegacyImageBox {
    left: number
    top: number
    width: number
    height: number
}

/** Round down to an even number, like the playout does for picture sizes. */
function evenPixel(value: number): number {
    return Math.floor(value / 2) * 2
}

/** One side of a `WIDTH:HEIGHT` logo scale; `null` keeps the image's proportion. */
function parseLegacyDimension(value: string, input: number, output: number): number | null {
    const text = value.trim()

    if (text === '-1') {
        return null
    }

    if (text === 'iw' || text === 'ih') {
        return input
    }

    if (['W', 'H', 'main_w', 'main_h'].includes(text)) {
        return output
    }

    if (text.endsWith('%')) {
        const percent = Number(text.slice(0, -1).trim())

        if (!Number.isFinite(percent) || percent <= 0) {
            throw new Error(`logo scale ${value}`)
        }

        return Math.round((output * percent) / 100)
    }

    if (!/^\d+$/.test(text)) {
        throw new Error(`logo scale ${value}`)
    }

    return Number(text)
}

/** The logo's size in output pixels for its scale setting (as the playout sizes it). */
export function legacyImageSize(scale: string, image: Size, frame: Size): Size {
    const trimmed = scale.trim()

    if (!trimmed) {
        return { width: Math.max(2, evenPixel(image.width)), height: Math.max(2, evenPixel(image.height)) }
    }

    const separator = trimmed.includes(':') ? ':' : 'x'
    const [w, h] = trimmed.split(separator)

    if (w === undefined || h === undefined) {
        throw new Error(`logo scale ${scale}`)
    }

    let width = parseLegacyDimension(w, image.width, frame.width)
    let height = parseLegacyDimension(h, image.height, frame.height)

    if (width !== null && height === null) {
        height = Math.floor((width * image.height) / image.width)
    }

    if (width === null && height !== null) {
        width = Math.floor((height * image.width) / image.height)
    }

    return {
        width: Math.max(2, evenPixel(width ?? image.width)),
        height: Math.max(2, evenPixel(height ?? image.height)),
    }
}

/** One axis of a position such as `W-w-12`: `main` is the picture, `overlay` the logo. */
function evaluateLegacyAxis(expr: string, main: number, overlay: number): number {
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

        if (!part) {
            continue
        }

        const sign = part.startsWith('-') ? -1 : 1

        if (sign < 0) {
            part = part.slice(1)
        }

        const [term = '', divisorText] = part.split('/')
        const divisor = divisorText === undefined ? 1 : Number(divisorText.trim())

        if (!Number.isInteger(divisor) || divisor <= 0) {
            throw new Error(`logo position ${expr}`)
        }

        const name = term.trim()
        // A share of the room around the logo: 0% the left (or top) edge, 100% the other.
        const value = name.endsWith('%')
            ? Math.round(((main - overlay) * Number(name.slice(0, -1).trim())) / 100)
            : name === 'M'
              ? main
              : name === 'O'
                ? overlay
                : Number(name)

        if (!Number.isFinite(value)) {
            throw new Error(`logo position ${expr}`)
        }

        total += sign * Math.trunc(value / divisor)
    }

    return total
}

/**
 * Where the logo lands in a `frame`-sized output, in percent, worked out as
 * the playout does from the scale and position settings and the image size.
 * `null` when a setting is not understood.
 */
export function legacyImageBox(scale: string, position: string, image: Size, frame: Size): LegacyImageBox | null {
    try {
        const size = legacyImageSize(scale, image, frame)
        const [x, y] = position.split(':')

        if (x === undefined || y === undefined) {
            return null
        }

        const clamp = (value: number, room: number) => Math.min(Math.max(value, 0), Math.max(room, 0))
        const left = evenPixel(clamp(evaluateLegacyAxis(x, frame.width, size.width), frame.width - size.width))
        const top = evenPixel(clamp(evaluateLegacyAxis(y, frame.height, size.height), frame.height - size.height))

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
export function imageBoxStyle(box: LegacyImageBox): Record<string, string> {
    return {
        left: `${box.left}%`,
        top: `${box.top}%`,
        width: `${box.width}%`,
        height: `${box.height}%`,
    }
}
