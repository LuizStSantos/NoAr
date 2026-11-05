import { authFetch } from '@/composables/authFetch'
import { useAuth } from '@/stores/auth'
import { useConfig } from '@/stores/config'

/** A layer of a graphics template; positions and sizes in percent of the frame. */
export interface TemplateLayer {
    id: string
    name: string
    kind: 'text' | 'image' | 'box' | 'video'
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
    animation_in: LayerAnimation
    animation_out: LayerAnimation
    animation_ms: number
    delay_ms: number
}

export type LayerAnimation =
    'none' | 'fade' | 'slide_left' | 'slide_right' | 'slide_up' | 'slide_down' | 'wipe' | 'scale'
export const LAYER_ANIMATIONS: LayerAnimation[] = [
    'none',
    'fade',
    'slide_left',
    'slide_right',
    'slide_up',
    'slide_down',
    'wipe',
    'scale',
]
export type TemplateInput = Omit<GraphicsTemplate, 'id' | 'fields'>

/** Keep geometry inside the frame, including when a locked resize reaches an edge. */
export function fitLayer(
    layer: TemplateLayer,
    patch: Partial<TemplateLayer>,
    proportional = false,
): Partial<TemplateLayer> {
    let width = Math.max(0.1, patch.width ?? layer.width)
    let height = Math.max(0.1, patch.height ?? layer.height)
    if (proportional && (patch.width !== undefined || patch.height !== undefined)) {
        const factor = patch.width !== undefined ? width / layer.width : height / layer.height
        width = layer.width * factor
        height = layer.height * factor
    }
    const resizing = patch.width !== undefined || patch.height !== undefined
    const x = Math.max(0, Math.min(patch.x ?? layer.x, resizing ? 99.9 : 100 - width))
    const y = Math.max(0, Math.min(patch.y ?? layer.y, resizing ? 99.9 : 100 - height))
    if (proportional) {
        const factor = Math.min(1, (100 - x) / width, (100 - y) / height)
        width *= factor
        height *= factor
    } else {
        width = Math.min(width, 100 - x)
        height = Math.min(height, 100 - y)
    }
    return { ...patch, x, y, width, height }
}

/** Derive the same operator fields that the API discovers in text and image layers. */
export function templateFields(layers: TemplateLayer[]): string[] {
    const fields = layers.flatMap((layer) => {
        const source =
            layer.kind === 'text'
                ? layer.text
                : layer.kind === 'image' && /^\{[\p{L}\p{N}_-]+\}$/u.test(layer.image?.trim() ?? '')
                  ? layer.image!
                  : ''
        return [...source.matchAll(/\{([\p{L}\p{N}_-]+)\}/gu)].map((match) => match[1]!)
    })
    return [...new Set(fields)]
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
        template: (id: number) => authFetch<GraphicsTemplate>(`${base()}/templates/${id}`, { headers: headers() }),
        fonts: () => authFetch<string[]>(`${base()}/fonts`, { headers: headers() }),
        save: (input: TemplateInput, id?: number) =>
            authFetch<GraphicsTemplate>(`${base()}/templates${id === undefined ? '' : `/${id}`}`, {
                method: id === undefined ? 'POST' : 'PUT',
                headers: headers(),
                body: JSON.stringify(input),
            }),
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
