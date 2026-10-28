import type { Shares } from '@/components/settings/PositionPad.vue'

/** The corners the L-shape video can go to. */
export const LSHAPE_CORNERS = ['top-left', 'top-right', 'bottom-left', 'bottom-right'] as const

export const CORNER_SHARES: Record<string, Shares> = {
    'top-left': { x: 0, y: 0 },
    top: { x: 50, y: 0 },
    'top-right': { x: 100, y: 0 },
    left: { x: 0, y: 50 },
    center: { x: 50, y: 50 },
    right: { x: 100, y: 50 },
    'bottom-left': { x: 0, y: 100 },
    bottom: { x: 50, y: 100 },
    'bottom-right': { x: 100, y: 100 },
}

/** The logo's box on the pad, in percent of the screen. */
export const LOGO_BOX = { width: 12, height: 12 * (16 / 9) }

/** A logo's box on the pad, from its size on air (percent of the screen), kept large enough to grab. */
export function padBox(box: { width: number; height: number } | null | undefined): { width: number; height: number } {
    if (!box) {
        return LOGO_BOX
    }

    const grow = Math.max(1, 6 / Math.max(box.width, 0.1), 6 / Math.max(box.height, 0.1))

    return { width: Math.min(40, box.width * grow), height: Math.min(40, box.height * grow) }
}

/**
 * Where a position puts the box, as shares: `X%:Y%` (also with a margin,
 * `12%+9`), a place name, or an expression like `W-w-12:12` (read as its
 * edge or middle).
 */
export function sharesOf(position: string | null | undefined): Shares {
    const value = position?.trim() ?? ''

    if (CORNER_SHARES[value]) {
        return CORNER_SHARES[value]
    }

    const [x = '', y = ''] = value.split(':')

    return { x: axisShare(x, /W|main_w/), y: axisShare(y, /H|main_h/) }
}

/** One axis of a position as a share: its percent, else its edge or middle. */
function axisShare(part: string, full: RegExp): number {
    const share = /^\s*([\d.]+)\s*%/.exec(part)

    if (share) {
        return Math.min(100, Math.max(0, Number(share[1])))
    }

    return part.includes('/2') ? 50 : full.test(part) ? 100 : 0
}

/** The channel's logo place: where it is, as shares, and its margin in pixels. */
export interface LogoPlace {
    shares: Shares
    margin: number
}

/**
 * Reads the channel's logo position: an edge with a margin (`12`,
 * `W-w-12`), the middle (`W/2-w/2`), or a share with a margin (`25%+6`).
 * `null` for an expression it does not know.
 */
export function readLogoPlace(position: string | null | undefined): LogoPlace | null {
    const [x, y] = (position ?? '').replace(/\s+/g, '').split(':')

    if (x === undefined || y === undefined) {
        return null
    }

    const axis = (value: string, big: string, small: string): { share: number; margin: number | null } | null => {
        if (/^\d+$/.test(value)) {
            return { share: 0, margin: Number(value) }
        }

        if (value === `${big}/2-${small}/2`) {
            return { share: 50, margin: null }
        }

        const end = new RegExp(`^${big}-${small}-(\\d+)$`).exec(value)

        if (end) {
            return { share: 100, margin: Number(end[1]) }
        }

        const share = /^([\d.]+)%([+-]\d+)?$/.exec(value)

        if (!share) {
            return null
        }

        const percent = Number(share[1])
        const offset = Number(share[2] ?? 0)
        const weight = 1 - (2 * percent) / 100

        return percent > 100
            ? null
            : { share: percent, margin: Math.abs(weight) > 0.05 ? Math.round(offset / weight) : null }
    }
    const horizontal = axis(x, 'W', 'w')
    const vertical = axis(y, 'H', 'h')

    if (!horizontal || !vertical) {
        return null
    }

    return {
        shares: { x: horizontal.share, y: vertical.share },
        margin: Math.max(0, horizontal.margin ?? vertical.margin ?? 12),
    }
}

/**
 * The position expression for a logo place: the edges and the middle as
 * before (`12`, `W-w-12`, `W/2-w/2`), anywhere else a share of the room
 * inside the margin, so the logo keeps clear of the edges as it moves.
 */
export function writeLogoPlace(place: LogoPlace): string {
    const margin = Math.max(0, Math.round(place.margin))
    const axis = (share: number, big: string, small: string) => {
        const value = Math.round(Math.min(100, Math.max(0, share)) * 10) / 10

        if (value === 0) return `${margin}`
        if (value === 50) return `${big}/2-${small}/2`
        if (value === 100) return `${big}-${small}-${margin}`

        const offset = Math.round(margin * (1 - (2 * value) / 100))

        return `${value}%${offset < 0 ? offset : `+${offset}`}`
    }

    return `${axis(place.shares.x, 'W', 'w')}:${axis(place.shares.y, 'H', 'h')}`
}
