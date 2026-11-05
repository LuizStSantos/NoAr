import { authFetch } from '@/composables/authFetch'
import { useAuth } from '@/stores/auth'
import { useConfig } from '@/stores/config'

/** A layer of a graphics template; positions and sizes in percent of the frame. */
export interface TemplateLayer {
    id: string
    name: string
    kind: 'text' | 'image' | 'box'
    visible: boolean
    x: number
    y: number
    width: number
    height: number
    opacity: number
    text: string
    font_family: string | null
    /** Percent of the frame height. */
    font_size: number
    bold: boolean
    color: string
    align: 'left' | 'center' | 'right'
    image: string | null
    fill: string | null
    radius: number
}

export interface GraphicsTemplate {
    id: number
    name: string
    group: string
    duration: number
    during_lshape: 'follow' | 'wait' | 'skip'
    layers: TemplateLayer[]
    fields: string[]
}

export interface GraphicOnAir {
    take_id: number
    template_id: number
    name: string
    since: number
    leaving: boolean
}

export type TakeResult = { state: 'on_air'; take_id: number } | { state: 'waiting' }

/** `text` with each `{field}` replaced by its value, as the engine does. */
export function fillFields(text: string, values: Record<string, string>): string {
    return text.replace(/\{([\p{L}\p{N}_-]+)\}/gu, (_, name: string) => values[name] ?? '')
}

/** The template's fields as `{name} {role}`. */
export function fieldList(template: GraphicsTemplate): string {
    return template.fields.map((field) => '{' + field + '}').join(' ')
}

export function onAirSince(seconds: number): string {
    const whole = Math.floor(seconds)

    return whole < 60 ? `${whole} s` : `${Math.floor(whole / 60)} min ${String(whole % 60).padStart(2, '0')} s`
}

/** The graphics API of the selected channel. */
export function useGraphicsApi() {
    const authStore = useAuth()
    const configStore = useConfig()

    const base = () => `/api/graphics/${configStore.channels[configStore.i]?.id}`
    const headers = () => ({ ...configStore.contentType, ...authStore.authHeader })

    return {
        templates: () => authFetch<GraphicsTemplate[]>(`${base()}/templates`, { headers: headers() }),
        onAir: () => authFetch<GraphicOnAir[]>(`${base()}/on-air`, { headers: headers() }),
        take: (templateId: number, fields: Record<string, string>, duration?: number) =>
            authFetch<TakeResult>(`${base()}/take`, {
                method: 'POST',
                headers: headers(),
                body: JSON.stringify({ template_id: templateId, fields, duration }),
            }),
        out: (takeId?: number) =>
            authFetch(`${base()}/out`, {
                method: 'POST',
                headers: headers(),
                body: JSON.stringify(takeId === undefined ? {} : { take_id: takeId }),
            }),
    }
}
