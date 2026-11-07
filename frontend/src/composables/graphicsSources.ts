import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { authFetch } from '@/composables/authFetch'
import { useConfig } from '@/stores/config'
import type { GraphicsTemplate } from '@/composables/graphicsTemplates'

export interface DataField {
    source_id: number
    column: string
    row: number
}
export interface GraphicsSource {
    id: number
    name: string
    kind: 'csv' | 'rss' | 'json'
    url: string
    interval: number
    records_path: string
    rows: Record<string, string>[]
    last_success: number | null
    last_attempt: number | null
    error: string | null
    revision: number
}
export interface SourceInput {
    name: string
    kind: 'csv' | 'rss' | 'json'
    url: string
    interval: number
    records_path: string
    document: string
    revision: number
}
const cache = ref<Record<number, GraphicsSource[]>>({})
const fetched = new Map<number, number>()
const pending = new Map<number, Promise<void>>()

export function sourceColumns(source?: GraphicsSource) {
    return [...new Set((source?.rows ?? []).flatMap((row) => Object.keys(row)))].sort()
}
export function sourceBindings(
    template: Pick<GraphicsTemplate, 'layers'> | null | undefined,
): Record<string, DataField> {
    return Object.assign({}, ...(template?.layers ?? []).map((layer) => layer.data_fields ?? {}))
}
export function sourceValues(
    template: Pick<GraphicsTemplate, 'layers'> | null | undefined,
    sources: GraphicsSource[],
    records: Record<string, number> = {},
) {
    const values: Record<string, string> = {}
    for (const [field, binding] of Object.entries(sourceBindings(template))) {
        const source = sources.find((source) => source.id === binding.source_id)
        const value = source?.rows[records[String(binding.source_id)] ?? binding.row]?.[binding.column]
        if (value !== undefined) {
            values[field] = value
        }
    }
    return values
}

/** Share one bounded polling request across cards and editor previews on a channel. */
export function useGraphicsSources(enabled: () => boolean = () => true) {
    const config = useConfig()
    const channel = computed(() => config.channels[config.i]?.id)
    const sources = computed(() => cache.value[channel.value ?? 0] ?? [])
    const unavailable = ref(false)
    let timer: ReturnType<typeof setInterval> | undefined
    let alive = true
    async function refresh(force = true) {
        const id = channel.value
        if (!enabled() || !id || (!force && performance.now() - (fetched.get(id) ?? -Infinity) < 2000)) {
            return
        }
        let request = pending.get(id)
        if (!request) {
            request = authFetch<GraphicsSource[]>(`/api/graphics/${id}/sources`)
                .then((data) => {
                    cache.value[id] = data
                    fetched.set(id, performance.now())
                })
                .finally(() => pending.delete(id))
            pending.set(id, request)
        }
        try {
            await request
            if (alive) {
                unavailable.value = false
            }
        } catch {
            if (alive) {
                unavailable.value = true
            }
        }
    }
    onMounted(() => {
        void refresh(false)
        timer = setInterval(() => {
            void refresh(false)
        }, 2000)
    })
    onBeforeUnmount(() => {
        alive = false
        clearInterval(timer)
    })
    watch(
        () => [channel.value, enabled()],
        () => {
            void refresh(false)
        },
    )
    return { sources, unavailable, refresh }
}
