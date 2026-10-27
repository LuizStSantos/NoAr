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

/**
 * Where a position puts the box, as shares: `X%:Y%`, a place name, or an
 * expression like `W-w-12:12` (read as its edge or middle).
 */
export function sharesOf(position: string | null | undefined): Shares {
    const value = position?.trim() ?? ''
    const shares = /^([\d.]+)%?\s*:\s*([\d.]+)%?$/.exec(value)

    if (CORNER_SHARES[value]) {
        return CORNER_SHARES[value]
    }

    if (shares && value.includes('%')) {
        return { x: Number(shares[1]), y: Number(shares[2]) }
    }

    const [x = '', y = ''] = value.split(':')
    const axis = (part: string, full: RegExp) => (part.includes('/2') ? 50 : full.test(part) ? 100 : 0)

    return { x: axis(x, /W|main_w/), y: axis(y, /H|main_h/) }
}
