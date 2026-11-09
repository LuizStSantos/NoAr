import type { RouteLocationNormalized } from 'vue-router'

type EditorGuard = (to: RouteLocationNormalized) => boolean
let activeGuard: EditorGuard | undefined

/** Ask before global channel initialization can replace a dirty editor. */
export function registerEditorNavigationGuard(guard: EditorGuard): () => void {
    activeGuard = guard

    return () => {
        if (activeGuard === guard) {
            activeGuard = undefined
        }
    }
}

export function confirmEditorNavigation(to: RouteLocationNormalized): boolean {
    return activeGuard?.(to) ?? true
}
