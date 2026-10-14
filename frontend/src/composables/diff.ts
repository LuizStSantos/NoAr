/** Number of values that differ between two settings trees. */
export function countChanges(a: unknown, b: unknown): number {
    if (Array.isArray(a) || Array.isArray(b)) {
        const left = Array.isArray(a) ? a : []
        const right = Array.isArray(b) ? b : []
        let count = Math.abs(left.length - right.length)

        for (let index = 0; index < Math.min(left.length, right.length); index++) {
            count += countChanges(left[index], right[index])
        }

        return count
    }

    if (a && b && typeof a === 'object' && typeof b === 'object') {
        const keys = new Set([...Object.keys(a), ...Object.keys(b)])
        let count = 0

        for (const key of keys) {
            // Client-side helpers of the playlist settings.
            if (key.endsWith('InSec')) {
                continue
            }

            count += countChanges((a as Record<string, unknown>)[key], (b as Record<string, unknown>)[key])
        }

        return count
    }

    // Number inputs may hand back strings; compare their meaning.
    if (typeof a === 'number' || typeof b === 'number') {
        return Number(a) === Number(b) ? 0 : 1
    }

    return (a ?? '') === (b ?? '') ? 0 : 1
}
