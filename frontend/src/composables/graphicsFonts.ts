import { onBeforeUnmount, ref, watch } from 'vue'
import { authFetch } from '@/composables/authFetch'
import { useConfig } from '@/stores/config'
import type { TemplateLayer } from '@/composables/graphicsTemplates'

export interface GraphicsFont {
    id: string
    name: string
    filename: string
    family: string
    size: number
}

const loaded = new Map<string, Promise<void>>()

/** One immutable asset supplies the exact font bytes to browser and engine. */
export async function loadGraphicsFont(channel: number, font: GraphicsFont) {
    let pending = loaded.get(font.family)
    if (!pending) {
        pending = authFetch<ArrayBuffer>(`/api/graphics/${channel}/font-assets/${font.id}`, {
            responseType: 'arrayBuffer',
        })
            .then(async (bytes) => {
                const face = await new FontFace(font.family, bytes).load()
                document.fonts.add(face)
            })
            .catch((error) => {
                loaded.delete(font.family)
                throw error
            })
        loaded.set(font.family, pending)
    }
    return pending
}

/** Preview loads uploaded faces only when a composition actually uses them. */
export function useGraphicsFonts(layers: () => TemplateLayer[]) {
    const config = useConfig()
    const error = ref('')
    let alive = true
    let version = 0
    onBeforeUnmount(() => {
        alive = false
        version++
    })
    watch(
        () => [config.channels[config.i]?.id, ...layers().map((layer) => layer.font_family)],
        async () => {
            const channel = config.channels[config.i]?.id
            const current = ++version
            const required = [...new Set(layers().map((layer) => layer.font_family || ''))].filter((family) =>
                family.startsWith('NoArFont_'),
            )
            if (!channel || !required.length) {
                error.value = ''
                return
            }
            try {
                const fonts = await authFetch<GraphicsFont[]>(`/api/graphics/${channel}/font-assets`)
                for (const family of required) {
                    const font = fonts.find((font) => font.family === family)
                    if (!font) {
                        throw new Error(family)
                    }
                    await loadGraphicsFont(channel, font)
                }
                if (alive && current === version) {
                    error.value = ''
                }
            } catch {
                if (alive && current === version) {
                    error.value = 'unavailable'
                }
            }
        },
        { immediate: true },
    )
    return { fontError: error }
}
