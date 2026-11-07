<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { onBeforeRouteLeave, onBeforeRouteUpdate, useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'

import { type DataField, sourceBindings, sourceColumns, useGraphicsSources } from '@/composables/graphicsSources'
import { type GraphicsFont, loadGraphicsFont } from '@/composables/graphicsFonts'
import { CLOCK_FORMATS } from '@/composables/graphicsClock'
import TemplatePreview from '@/components/graphics/TemplatePreview.vue'
import StoragePicker from '@/components/schedule/StoragePicker.vue'
import { sendFileInChunks } from '@/composables/chunkedUpload'
import { useAuth } from '@/stores/auth'
import { randomId } from '@/composables/browser'
import { storageImageUrl } from '@/composables/logo'
import { AuthFetchError, authFetch } from '@/composables/authFetch'
import {
    type GraphicsTemplate,
    type TemplateLayer,
    type TemplateInput,
    LAYER_ANIMATIONS,
    fitLayer,
    templateFields,
    fieldList,
    useGraphicsApi,
    graphicsUploadFilename,
    sanitizeGraphicsUploadFilename,
} from '@/composables/graphicsTemplates'
import { useConfig } from '@/stores/config'

/** Author a template locally; only an explicit save changes the stored template. */
const { t } = useI18n()
const configStore = useConfig()
const graphicsApi = useGraphicsApi()
const { sources: dataSources } = useGraphicsSources()
const channelId = computed(() => configStore.channels[configStore.i]?.id)
const route = useRoute()
const router = useRouter()
const authStore = useAuth()
const draft = ref<TemplateInput>({
    name: '',
    group: '',
    duration: 8,
    during_lshape: 'follow',
    layers: [],
    always_on: false,
})
const selectedId = ref('')
const selected = computed(() => draft.value.layers.find((layer) => layer.id === selectedId.value))
const fields = computed(() => templateFields(draft.value.layers))
const preview = computed<GraphicsTemplate>(() => ({ ...draft.value, id: savedId.value ?? 0, fields: fields.value }))
const sampleValues = computed(() =>
    Object.fromEntries(
        fields.value
            .filter((field) => !sourceBindings(preview.value)[field])
            .map((field) => [field, '[' + field + ']']),
    ),
)
const savedId = ref<number>()
const baseline = ref(JSON.stringify(draft.value))
const history = ref<string[]>([])
const fonts = ref<string[]>([])
const uploadedFonts = ref<GraphicsFont[]>([])
const fontUploading = ref(false)
const groups = ref<string[]>([])
const assignment = ref('')
const originalAssignment = ref('')
const saving = ref(false)
const busy = ref(false)
const editorReady = ref(false)
const error = ref('')
const note = ref('')
const adding = ref(false)
const picking = ref(false)
const proportional = ref(false)
const unit = ref('%')
const canvas = ref<HTMLElement>()
// Canvas aids are local to the editor and never enter the saved template.
const showSafeAreas = ref(false)
const guideV = ref(false)
const guideH = ref(false)
// Scale presets use the created or loaded size, so consecutive presets never compound.
const scaleBases = new Map<string, { width: number; height: number }>()
const output = computed(() => configStore.playout?.output)
const frame = computed(() => ({ width: output.value?.width || 1920, height: output.value?.height || 1080 }))
const time = ref<number>()
const playing = ref(false)
const timelineEnd = computed(
    () =>
        Math.max(
            draft.value.duration || 5,
            ...draft.value.layers.map((layer) => (layer.delay_ms + layer.animation_ms) / 1000),
        ) + Math.max(0.4, ...draft.value.layers.map((layer) => layer.animation_ms / 1000)),
)
let animationFrame = 0
let loadVersion = 0
let drag:
    | { id: string; mode: string; x: number; y: number; rect: DOMRect; layer: TemplateLayer; snapshot: string }
    | undefined
const dirty = computed(
    () => JSON.stringify(draft.value) !== baseline.value || assignment.value !== originalAssignment.value,
)

async function uploadFont(event: Event) {
    const input = event.target as HTMLInputElement
    const file = input.files?.[0]
    const id = channelId.value
    const layerId = selectedId.value
    const version = loadVersion
    input.value = ''

    if (!file || !id) {
        return
    }

    fontUploading.value = true
    error.value = ''
    try {
        const asset = await authFetch<GraphicsFont>(
            `/api/graphics/${id}/font-assets?filename=${encodeURIComponent(file.name)}`,
            {
                method: 'POST',
                body: file,
                headers: { 'Content-Type': 'application/octet-stream' },
            },
        )
        await loadGraphicsFont(id, asset)
        uploadedFonts.value.push(asset)

        if (version === loadVersion && selectedId.value === layerId) {
            updateSelectedLayer({ font_family: asset.family })
        }
    } catch (failure) {
        error.value = String(failure)
    } finally {
        fontUploading.value = false
    }
}

function mapField(field: string, binding?: DataField) {
    rememberDraft()
    for (const layer of draft.value.layers) {
        if (!templateFields([layer]).includes(field)) {
            continue
        }
        layer.data_fields = { ...layer.data_fields }
        if (binding) {
            layer.data_fields[field] = binding
        } else {
            delete layer.data_fields[field]
        }
    }
}
function selectFieldSource(field: string, event: Event) {
    const id = Number((event.target as HTMLSelectElement).value)
    const source = dataSources.value.find((source) => source.id === id)
    mapField(field, source ? { source_id: id, column: sourceColumns(source)[0] ?? '', row: 0 } : undefined)
}
function changeFieldMapping(field: string, changes: Partial<DataField>) {
    const current = sourceBindings(preview.value)[field]
    if (current) {
        mapField(field, { ...current, ...changes })
    }
}

function rememberDraft() {
    history.value.push(JSON.stringify(draft.value))

    if (history.value.length > 100) {
        history.value.shift()
    }

    note.value = ''
}

function undo() {
    const previous = history.value.pop()

    if (!previous) {
        return
    }

    stopPreview()
    draft.value = JSON.parse(previous)

    if (!selected.value) {
        selectedId.value = draft.value.layers[0]?.id ?? ''
    }

    note.value = ''
}

function updateSelectedLayer(patch: Partial<TemplateLayer>) {
    if (!selected.value) {
        return
    }

    rememberDraft()

    if (patch.animation_ms !== undefined) {
        patch.clip_animation_ms = null
    }

    Object.assign(selected.value, patch)
}

function setAlwaysOn(value: boolean) {
    rememberDraft()
    draft.value.always_on = value
    draft.value.duration = 0
}

const videoCorner = computed(() => {
    const layer = selected.value

    if (!layer || layer.kind !== 'video') {
        return 'custom'
    }

    const horizontal = Math.abs(layer.x) < 0.01 ? 'left' : Math.abs(layer.x + layer.width - 100) < 0.01 ? 'right' : ''
    const vertical = Math.abs(layer.y) < 0.01 ? 'top' : Math.abs(layer.y + layer.height - 100) < 0.01 ? 'bottom' : ''

    return horizontal && vertical ? `${vertical}-${horizontal}` : 'custom'
})

function setVideoWindow(corner: string, size: number) {
    if (!selected.value || corner === 'custom') {
        return
    }

    const percent = Math.max(30, Math.min(95, size))
    const width = Math.max(2, Math.round((frame.value.width * percent) / 100) & ~1)
    const height = Math.max(2, Math.round((frame.value.height * percent) / 100) & ~1)
    const x = corner.endsWith('right') ? (frame.value.width - width) & ~1 : 0
    const y = corner.startsWith('bottom') ? (frame.value.height - height) & ~1 : 0
    updateSelectedLayer({
        x: (x * 100) / frame.value.width,
        y: (y * 100) / frame.value.height,
        width: (width * 100) / frame.value.width,
        height: (height * 100) / frame.value.height,
    })
}

function setDimension(key: 'x' | 'y' | 'width' | 'height', value: string) {
    if (!selected.value) {
        return
    }

    const number = Number(value.replace(',', '.'))

    if (!Number.isFinite(number)) {
        return
    }

    const axis = key === 'x' || key === 'width' ? frame.value.width : frame.value.height
    updateSelectedLayer(
        fitLayer(
            selected.value,
            { [key]: unit.value === 'px' ? (number / axis) * 100 : number },
            proportional.value && (key === 'width' || key === 'height'),
        ),
    )
}

function formatDimension(key: 'x' | 'y' | 'width' | 'height') {
    const value = selected.value?.[key] ?? 0
    const axis = key === 'x' || key === 'width' ? frame.value.width : frame.value.height
    return Math.round(value * (unit.value === 'px' ? axis / 100 : 1) * 100) / 100
}

function setScale(value: number) {
    const layer = selected.value

    if (!layer || !Number.isFinite(value) || value <= 0) {
        return
    }

    const base = scaleBases.get(layer.id) ?? layer
    updateSelectedLayer(
        fitLayer(layer, { width: (base.width * value) / 100, height: (base.height * value) / 100 }, true),
    )
}

const scaleValue = computed(() =>
    selected.value
        ? Math.round((selected.value.width / (scaleBases.get(selected.value.id)?.width ?? selected.value.width)) * 100)
        : 100,
)
function addLayer(kind: TemplateLayer['kind']) {
    if (
        draft.value.layers.length >= 64 ||
        (kind === 'video' &&
            (draft.value.always_on === true || draft.value.layers.some((layer) => layer.kind === 'video')))
    ) {
        return
    }

    rememberDraft()
    const layer: TemplateLayer = {
        id: randomId(),
        name: t('graphics.composer.kinds.' + kind),
        kind,
        visible: true,
        x: kind === 'video' ? 5 : 10,
        y: kind === 'video' ? 5 : 70,
        width: kind === 'video' ? 70 : 40,
        height: kind === 'video' ? 70 : 12,
        opacity: 1,
        clock_format: kind === 'clock' ? 'HH:mm:ss' : null,
        clock_timezone: null,
        shape: 'rectangle',
        stroke: null,
        stroke_width: 0.25,
        text: kind === 'text' ? t('graphics.composer.sampleText') : '',
        font_family: null,
        font_size: 4,
        bold: false,
        color: '#ffffff',
        align: 'left',
        image: null,
        fill: kind === 'box' ? '#f39223' : null,
        radius: 0,
        animation_in: 'fade',
        animation_out: 'fade',
        animation_ms: 400,
        delay_ms: 0,
    }

    draft.value.layers.push(layer)
    scaleBases.set(layer.id, { width: layer.width, height: layer.height })
    selectedId.value = layer.id
    adding.value = false
    stopPreview()
}

function setShape(shape: NonNullable<TemplateLayer['shape']>) {
    updateSelectedLayer({
        shape,
        ...(shape === 'rounded' ? { radius: 2 } : {}),
        ...(shape === 'line' ? { stroke: selected.value?.stroke || '#ffffff', fill: null } : {}),
    })
}

function removeLayer(id: string) {
    const layer = draft.value.layers.find((layer) => layer.id === id)

    if (!layer) {
        return
    }

    rememberDraft()
    draft.value.layers = draft.value.layers.filter((layer) => layer.id !== id)

    if (selectedId.value === id) {
        selectedId.value = draft.value.layers[0]?.id ?? ''
    }

    note.value = t('graphics.composer.removed', { name: layer.name })
}

function toggleLayer(layer: TemplateLayer) {
    rememberDraft()
    layer.visible = !layer.visible
}

function startDrag(event: PointerEvent, layer: TemplateLayer, mode: string) {
    if (event.button !== 0 || !canvas.value) {
        return
    }

    event.preventDefault()
    stopPreview()
    selectedId.value = layer.id
    ;(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId)
    drag = {
        id: layer.id,
        mode,
        x: event.clientX,
        y: event.clientY,
        rect: canvas.value.getBoundingClientRect(),
        layer: { ...layer },
        snapshot: JSON.stringify(draft.value),
    }
}

/** Snap dragging to the centre and safe edges without changing typed geometry. */
function moveLayer(event: PointerEvent) {
    if (!drag) {
        return
    }

    const layer = draft.value.layers.find((layer) => layer.id === drag!.id)

    if (!layer) {
        return
    }

    const original = drag.layer
    const dx = ((event.clientX - drag.x) / drag.rect.width) * 100
    const dy = ((event.clientY - drag.y) / drag.rect.height) * 100

    if (drag.mode === 'move') {
        const snap = (value: number, size: number) => {
            const targets = [50 - size / 2, 5, 95 - size]
            return Math.max(0, Math.min(targets.find((target) => Math.abs(value - target) < 1.2) ?? value, 100 - size))
        }

        layer.x = snap(original.x + dx, original.width)
        layer.y = snap(original.y + dy, original.height)
        guideV.value = Math.abs(layer.x + layer.width / 2 - 50) < 0.01
        guideH.value = Math.abs(layer.y + layer.height / 2 - 50) < 0.01
    } else {
        const patch =
            drag.mode === 'height'
                ? { height: original.height + dy }
                : drag.mode === 'width'
                  ? { width: original.width + dx }
                  : { width: original.width + dx, height: original.height + dy }
        Object.assign(layer, fitLayer(original, patch, proportional.value))
    }
}

function endDrag() {
    if (drag && JSON.stringify(draft.value) !== drag.snapshot) {
        history.value.push(drag.snapshot)
        note.value = ''
    }

    drag = undefined
    guideV.value = guideH.value = false
}

function seekPreview(event: Event) {
    stopPreview()
    time.value = Number((event.target as HTMLInputElement).value)
}

function stopPreview() {
    cancelAnimationFrame(animationFrame)
    playing.value = false
    time.value = undefined
}

function playPreview() {
    if (playing.value) {
        stopPreview()
        return
    }

    const start = performance.now()
    playing.value = true
    const tick = (now: number) => {
        time.value = (now - start) / 1000

        if (time.value < timelineEnd.value) {
            animationFrame = requestAnimationFrame(tick)
        } else {
            playing.value = false
        }
    }

    animationFrame = requestAnimationFrame(tick)
}

async function loadEditor() {
    const version = ++loadVersion

    stopPreview()

    busy.value = true
    editorReady.value = false
    error.value = note.value = ''
    try {
        const id = route.query.template === undefined ? undefined : Number(route.query.template)

        if (id !== undefined && (!Number.isSafeInteger(id) || id <= 0)) {
            throw new Error(t('graphics.composer.notFound'))
        }

        const [template, families, templates, assigned, assets] = await Promise.all([
            id === undefined ? undefined : graphicsApi.template(id),
            graphicsApi.fonts(),
            // Group suggestions must not block editing when the library is unavailable.
            graphicsApi.templates().catch(() => []),
            graphicsApi.channel(),
            authFetch<GraphicsFont[]>(`/api/graphics/${channelId.value}/font-assets`),
        ])

        if (version !== loadVersion) {
            return
        }

        savedId.value = template?.id
        assignment.value =
            assigned.find(
                (model) => model.template.id === template?.id && ['logo', 'ticker', 'lshape'].includes(model.kind),
            )?.kind ?? ''
        originalAssignment.value = assignment.value
        draft.value = template
            ? {
                  name: template.name,
                  group: template.group,
                  duration: template.duration,
                  during_lshape: template.during_lshape,
                  layers: template.layers,
                  always_on: template.always_on,
                  control_layer_id: template.control_layer_id,
              }
            : { name: '', group: '', duration: 8, during_lshape: 'follow', layers: [], always_on: false }
        uploadedFonts.value = assets
        fonts.value = families
        groups.value = [...new Set(templates.map((template) => template.group))].filter(Boolean)
        scaleBases.clear()
        for (const layer of draft.value.layers) {
            scaleBases.set(layer.id, { width: layer.width, height: layer.height })
        }

        selectedId.value = draft.value.layers[0]?.id ?? ''
        history.value = []
        baseline.value = JSON.stringify(draft.value)
        saving.value = adding.value = picking.value = false
        editorReady.value = true
    } catch (err) {
        if (version === loadVersion) {
            error.value = err instanceof Error ? err.message : String(err)
        }
    } finally {
        if (version === loadVersion) {
            busy.value = false
        }
    }
}

async function saveTemplate() {
    if (busy.value || !editorReady.value || !draft.value.name.trim()) {
        return
    }

    busy.value = true
    error.value = ''
    try {
        let result = await graphicsApi.save(draft.value, savedId.value)
        savedId.value = result.id

        if (assignment.value !== originalAssignment.value) {
            result = await graphicsApi.assign(result.id, assignment.value || null, draft.value.control_layer_id)
            originalAssignment.value = assignment.value
        }
        draft.value = {
            name: result.name,
            group: result.group,
            duration: result.duration,
            during_lshape: result.during_lshape,
            layers: result.layers,
            always_on: result.always_on,
            control_layer_id: result.control_layer_id,
        }

        baseline.value = JSON.stringify(draft.value)
        saving.value = false
        await router.replace({ name: 'graphics-composer', query: { channel: channelId.value, template: result.id } })
        note.value = t('graphics.composer.saved', { name: result.name })
    } catch (err) {
        error.value =
            err instanceof AuthFetchError &&
            err.status === 409 &&
            typeof err.data === 'string' &&
            err.data.startsWith('At most 16')
                ? t('graphics.composer.permanentLimit')
                : err instanceof AuthFetchError && err.status === 409
                  ? t('graphics.composer.nameTaken')
                  : err instanceof Error
                    ? err.message
                    : String(err)
    } finally {
        busy.value = false
    }
}

/** Retry only name conflicts; the backend also rejects overwrite races atomically. */
async function uploadImage(event: Event) {
    const input = event.target as HTMLInputElement
    const original = input.files?.[0]
    const layer = selected.value
    const channel = channelId.value

    if (!original || !layer || busy.value || channel === undefined) {
        return
    }

    const cleanedName = sanitizeGraphicsUploadFilename(original.name)

    if (!/\.(png|jpe?g|webp|gif)$/i.test(cleanedName)) {
        error.value = t('graphics.composer.imageOnly')
        input.value = ''
        return
    }

    busy.value = true
    error.value = ''

    try {
        await authFetch(`/api/file/${channel}/create-folder`, {
            method: 'POST',
            headers: { ...configStore.contentType, ...authStore.authHeader },
            body: JSON.stringify({ source: 'Graphics' }),
            responseType: 'text',
        })

        let number = 1

        while (true) {
            const name = graphicsUploadFilename(cleanedName, number)
            const file = new File([original], name, { type: original.type, lastModified: original.lastModified })

            try {
                await sendFileInChunks(
                    file,
                    { url: `/api/file/${channel}/upload?path=Graphics`, headers: authStore.authHeader },
                    (bytes) => {
                        note.value = t('graphics.composer.uploadProgress', {
                            percent: Math.round((bytes / file.size) * 100),
                        })
                    },
                )

                await setImage(layer, 'Graphics/' + name)
                note.value = t('graphics.composer.uploaded')
                break
            } catch (uploadError) {
                if (uploadError instanceof AuthFetchError && uploadError.status === 409 && number < 1000) {
                    number += 1
                    continue
                }

                throw uploadError
            }
        }
    } catch (uploadError) {
        error.value = uploadError instanceof Error ? uploadError.message : String(uploadError)
    } finally {
        busy.value = false
        input.value = ''
    }
}

/** Explicit repair for a previously saved full-frame logo; never rewrite an authored model automatically. */
async function restoreLogoPlacement() {
    const layer = selected.value

    if (!layer?.image || busy.value) {
        return
    }

    busy.value = true
    error.value = ''

    try {
        const source = await storageImageUrl(configStore.channels[configStore.i], layer.image)
        const image = new Image()
        image.src = source
        await image.decode()

        if (selected.value?.id !== layer.id) {
            return
        }

        const availableWidth = Math.max(2, frame.value.width - 24)
        const availableHeight = Math.max(2, frame.value.height - 24)
        const factor = Math.min(1, availableWidth / image.naturalWidth, availableHeight / image.naturalHeight)
        const width = Math.max(2, Math.floor(image.naturalWidth * factor) & ~1)
        const height = Math.max(2, Math.floor(image.naturalHeight * factor) & ~1)
        updateSelectedLayer({
            x: (Math.max(0, frame.value.width - width - 12) * 100) / frame.value.width,
            y: (12 * 100) / frame.value.height,
            width: (width * 100) / frame.value.width,
            height: (height * 100) / frame.value.height,
        })
        scaleBases.set(layer.id, { width: layer.width, height: layer.height })
    } catch (failure) {
        error.value = failure instanceof Error ? failure.message : String(failure)
    } finally {
        busy.value = false
    }
}

/** Size only the first image in an empty layer; replacements retain authored geometry. */
async function setImage(layer: TemplateLayer, path: string) {
    const version = loadVersion
    let geometry: Partial<TemplateLayer> = {}

    if (!layer.image) {
        const source = await storageImageUrl(configStore.channels[configStore.i], path)
        const image = new Image()
        image.src = source
        await image.decode()

        const factor = Math.min(1, frame.value.width / image.naturalWidth, frame.value.height / image.naturalHeight)
        const width = (image.naturalWidth * factor * 100) / frame.value.width
        const height = (image.naturalHeight * factor * 100) / frame.value.height
        geometry = {
            x: Math.max(0, Math.min(layer.x, 100 - width)),
            y: Math.max(0, Math.min(layer.y, 100 - height)),
            width,
            height,
        }
    }

    if (version !== loadVersion || !draft.value.layers.includes(layer)) {
        return
    }

    rememberDraft()
    Object.assign(layer, geometry, { image: path })
    scaleBases.set(layer.id, { width: layer.width, height: layer.height })
}

async function pickImage(item: { path: string }) {
    if (!/\.(png|jpe?g|webp|gif)$/i.test(item.path)) {
        error.value = t('graphics.composer.imageOnly')
        return
    }

    const layer = selected.value

    if (!layer || busy.value) {
        return
    }

    busy.value = true
    error.value = ''

    try {
        await setImage(layer, item.path)
        picking.value = false
    } catch (failure) {
        error.value = failure instanceof Error ? failure.message : String(failure)
    } finally {
        busy.value = false
    }
}

function confirmBeforeUnload(event: BeforeUnloadEvent) {
    if (editorReady.value && dirty.value) {
        event.preventDefault()
        event.returnValue = ''
    }
}

onBeforeRouteLeave(() => !editorReady.value || !dirty.value || window.confirm(t('graphics.composer.discard')))
onBeforeRouteUpdate((to) => {
    if (
        String(to.query.template ?? '') === String(route.query.template ?? '') &&
        to.query.channel === route.query.channel
    ) {
        return
    }

    if (dirty.value && !busy.value && !window.confirm(t('graphics.composer.discard'))) {
        return false
    }
})
watch(
    () => [route.query.template, channelId.value],
    () => {
        if (
            busy.value &&
            savedId.value !== undefined &&
            Number(route.query.template) === savedId.value &&
            !dirty.value
        ) {
            return
        }

        loadEditor()
    },
)
onMounted(() => {
    loadEditor()
    window.addEventListener('beforeunload', confirmBeforeUnload)
})
onBeforeUnmount(() => {
    ++loadVersion
    stopPreview()
    window.removeEventListener('beforeunload', confirmBeforeUnload)
})
</script>
<template>
    <section class="flex flex-col gap-4" :aria-label="t('graphics.composer.title')">
        <header class="flex flex-wrap items-end gap-3">
            <label class="field-label flex-auto min-w-48"
                >{{ t('graphics.composer.title') }}
                <input
                    v-model="draft.name"
                    :aria-label="t('graphics.composer.templateName')"
                    class="field-input font-semibold"
                    maxlength="100"
                    :placeholder="t('graphics.composer.templateName')"
                />
            </label>
            <span class="font-mono text-xs text-muted"
                >{{ frame.width }}×{{ frame.height }} · {{ output?.fps || 30 }} fps</span
            >
            <button class="btn-quiet" :disabled="!history.length" @click="undo">
                {{ t('graphics.composer.undo') }}
            </button>
            <button class="btn-quiet" :disabled="!draft.layers.length" @click="playPreview">
                {{ t(playing ? 'graphics.composer.stop' : 'graphics.composer.play') }}
            </button>
            <button class="btn-main" :disabled="busy || !editorReady" @click="saving = !saving">
                {{ t('graphics.composer.saveTemplate') }}
            </button>
            <RouterLink
                :to="{ name: 'graphics-onair', query: { channel: channelId, template: savedId } }"
                class="btn-quiet"
                >{{ t('graphics.composer.openOnAir') }}</RouterLink
            >
            <RouterLink :to="{ name: 'graphics-templates', query: { channel: channelId } }" class="link-accent">{{
                t('graphics.onAir.library')
            }}</RouterLink>
        </header>
        <div v-if="error" role="alert" class="text-error">
            {{ error }}
            <button v-if="!editorReady" class="btn-quiet" :disabled="busy" @click="loadEditor">
                {{ t('graphics.composer.retry') }}
            </button>
        </div>
        <div v-if="note" role="status" class="card-box p-3 flex items-center gap-3">
            <span class="flex-auto">{{ note }}</span
            ><button v-if="history.length" class="btn-quiet" @click="undo">{{ t('graphics.composer.undo') }}</button>
        </div>
        <form
            v-if="saving"
            class="card-box p-4 flex flex-wrap gap-3 items-end border-accent"
            @submit.prevent="saveTemplate"
        >
            <label class="field-label flex-1 min-w-48"
                >{{ t('graphics.composer.templateName')
                }}<input v-model="draft.name" class="field-input" required maxlength="100"
            /></label>
            <label class="field-label flex-1 min-w-40"
                >{{ t('graphics.composer.group')
                }}<input v-model="draft.group" class="field-input" list="graphics-groups" maxlength="100" /><datalist
                    id="graphics-groups"
                >
                    <option v-for="name in groups" :key="name" :value="name" /></datalist
            ></label>
            <div class="flex-1 min-w-40 text-sm">
                <span class="text-muted">{{ t('graphics.composer.fields') }}</span>
                <p class="m-0 mt-2 font-mono">{{ fieldList(preview) || t('graphics.templates.noFields') }}</p>
            </div>
            <button type="button" class="btn-quiet" :disabled="busy" @click="saving = false">
                {{ t('graphics.composer.cancel') }}
            </button>
            <label class="field-label flex-1 min-w-48"
                >{{ t('graphics.composer.assignment') }}
                <select v-model="assignment" class="field-input" :aria-label="t('graphics.composer.assignment')">
                    <option value="">{{ t('graphics.composer.noAssignment') }}</option>
                    <option v-for="role in ['logo', 'ticker', 'lshape']" :key="role" :value="role">
                        {{ t(`graphics.assigned.${role}`) }}
                    </option>
                </select>
            </label>
            <label v-if="['logo', 'lshape'].includes(assignment)" class="field-label flex-1 min-w-48"
                >{{ t('graphics.composer.controlImage') }}
                <select
                    v-model="draft.control_layer_id"
                    :aria-label="t('graphics.composer.controlImage')"
                    class="field-input"
                >
                    <option :value="null">{{ t('graphics.composer.firstImage') }}</option>
                    <option
                        v-for="layer in draft.layers.filter((layer) => layer.kind === 'image')"
                        :key="layer.id"
                        :value="layer.id"
                    >
                        {{ layer.name || layer.id }}
                    </option>
                </select>
            </label>
            <button class="btn-main" :disabled="busy || !draft.name.trim()">{{ t('graphics.composer.save') }}</button>
            <fieldset
                v-if="fields.length"
                class="basis-full w-full border-t border-line pt-4 flex flex-col gap-3 min-w-0"
            >
                <legend class="font-semibold text-sm">{{ t('graphics.sources.mapping') }}</legend>
                <p class="text-xs text-muted m-0">{{ t('graphics.sources.mappingHint') }}</p>
                <div v-for="field in fields" :key="field" class="grid sm:grid-cols-3 gap-2 min-w-0">
                    <label class="field-label"
                        >{{ t('graphics.sources.fieldSource', { field })
                        }}<select
                            class="field-input"
                            :value="sourceBindings(preview)[field]?.source_id ?? ''"
                            @change="selectFieldSource(field, $event)"
                        >
                            <option value="">{{ t('graphics.sources.manual') }}</option>
                            <option v-for="source in dataSources" :key="source.id" :value="source.id">
                                {{ source.name }}
                            </option>
                        </select></label
                    >
                    <template v-if="sourceBindings(preview)[field]">
                        <label class="field-label"
                            >{{ t('graphics.sources.column', { field })
                            }}<select
                                class="field-input"
                                :value="sourceBindings(preview)[field]?.column"
                                @change="
                                    changeFieldMapping(field, {
                                        column: ($event.target as HTMLSelectElement).value,
                                    })
                                "
                            >
                                <option
                                    v-for="column in sourceColumns(
                                        dataSources.find(
                                            (source) => source.id === sourceBindings(preview)[field]?.source_id,
                                        )!,
                                    )"
                                    :key="column"
                                    :value="column"
                                >
                                    {{ column }}
                                </option>
                            </select></label
                        >
                        <label class="field-label"
                            >{{ t('graphics.sources.record', { field })
                            }}<input
                                class="field-input"
                                type="number"
                                min="1"
                                max="200"
                                :value="(sourceBindings(preview)[field]?.row ?? 0) + 1"
                                @change="
                                    changeFieldMapping(field, {
                                        row: Number(($event.target as HTMLInputElement).value) - 1,
                                    })
                                "
                        /></label>
                    </template>
                </div>
                <RouterLink
                    class="link-accent text-sm"
                    :to="{ name: 'graphics-sources', query: { channel: channelId } }"
                    >{{ t('graphics.sections.sources') }}</RouterLink
                >
            </fieldset>
        </form>
        <div
            class="composer-grid"
            :class="{ 'opacity-60 pointer-events-none': busy || !editorReady }"
            :aria-busy="busy"
        >
            <aside class="card-box p-3 flex flex-col gap-3 min-w-0" :aria-label="t('graphics.composer.layers')">
                <div class="flex items-center justify-between">
                    <h2 class="m-0 text-base font-semibold">{{ t('graphics.composer.layers') }}</h2>
                    <button
                        class="btn-icon min-h-11 min-w-11"
                        :aria-label="t('graphics.composer.add')"
                        :aria-expanded="adding"
                        :disabled="draft.layers.length >= 64"
                        @click="adding = !adding"
                    >
                        <i class="bi bi-plus-lg" />
                    </button>
                </div>
                <div
                    v-if="adding"
                    class="flex flex-col gap-1 border border-line-strong rounded-field p-1"
                    role="group"
                    :aria-label="t('graphics.composer.layerType')"
                >
                    <button
                        v-for="kind in ['image', 'text', 'box', 'video', 'clock'] as const"
                        :key="kind"
                        class="btn-quiet text-left"
                        :disabled="
                            kind === 'video' &&
                            (draft.always_on === true || draft.layers.some((layer) => layer.kind === 'video'))
                        "
                        :title="
                            kind === 'video' && draft.always_on === true
                                ? t('graphics.composer.videoModelOnly')
                                : undefined
                        "
                        @click="addLayer(kind)"
                    >
                        {{ t('graphics.composer.kinds.' + kind) }}
                    </button>
                </div>
                <p v-if="!draft.layers.length" class="m-0 text-sm text-muted">{{ t('graphics.composer.empty') }}</p>
                <div
                    v-for="layer in draft.layers"
                    :key="layer.id"
                    class="flex items-center border rounded-field min-w-0"
                    :class="[
                        selectedId === layer.id ? 'border-accent bg-base-300' : 'border-line-strong',
                        { 'opacity-50': !layer.visible },
                    ]"
                >
                    <button
                        class="flex-auto min-w-0 min-h-12 text-left px-2 cursor-pointer"
                        @click="selectedId = layer.id"
                    >
                        <span class="text-[10px] text-muted block">{{
                            t('graphics.composer.kinds.' + layer.kind)
                        }}</span
                        ><span class="block text-sm truncate">{{ layer.name }}</span
                        ><span class="block font-mono text-xs text-accent truncate">{{
                            templateFields([layer])
                                .map((field) => '{' + field + '}')
                                .join(' ')
                        }}</span>
                    </button>
                    <button
                        class="btn-icon min-h-9 min-w-9"
                        :aria-label="
                            t(layer.visible ? 'graphics.composer.hide' : 'graphics.composer.show', { name: layer.name })
                        "
                        :aria-pressed="!layer.visible"
                        @click="toggleLayer(layer)"
                    >
                        <i :class="layer.visible ? 'bi bi-eye' : 'bi bi-eye-slash'" />
                    </button>
                    <button
                        class="btn-icon min-h-9 min-w-9"
                        :aria-label="t('graphics.composer.deleteNamed', { name: layer.name })"
                        @click="removeLayer(layer.id)"
                    >
                        <i class="bi bi-x-lg" />
                    </button>
                </div>
            </aside>
            <main class="min-w-0 flex flex-col gap-3">
                <div class="flex flex-wrap gap-3 items-center text-xs text-muted">
                    <span>{{ t('graphics.onAir.preview') }}</span>
                    <label class="flex items-center gap-2 cursor-pointer min-h-9">
                        <input v-model="showSafeAreas" type="checkbox" class="checkbox checkbox-sm" />
                        {{ t('graphics.composer.safeArea') }}
                    </label>
                    <span class="ml-auto font-mono">{{ time?.toFixed(2) ?? '0.00' }} s</span>
                </div>
                <p v-if="showSafeAreas" class="m-0 text-xs text-muted">
                    {{ t('graphics.composer.safeAreaHint') }}
                </p>
                <div
                    ref="canvas"
                    class="relative touch-none select-none"
                    @pointermove="moveLayer"
                    @pointerup="endDrag"
                    @pointercancel="endDrag"
                    @lostpointercapture="endDrag"
                >
                    <TemplatePreview
                        :template="preview"
                        :values="sampleValues"
                        :time="time"
                        class="composer-canvas rounded-field border border-line-strong"
                    >
                        <template #overlay>
                            <template v-if="showSafeAreas">
                                <div
                                    class="composer-safe-area composer-action-safe"
                                    role="img"
                                    :aria-label="t('graphics.composer.actionSafe')"
                                />
                                <div
                                    class="composer-safe-area composer-title-safe"
                                    role="img"
                                    :aria-label="t('graphics.composer.titleSafe')"
                                />
                            </template>
                            <div
                                v-if="guideV"
                                class="absolute left-1/2 inset-y-0 border-l border-cyan-300 pointer-events-none"
                            />
                            <div
                                v-if="guideH"
                                class="absolute top-1/2 inset-x-0 border-t border-cyan-300 pointer-events-none"
                            />
                            <template v-if="time === undefined">
                                <div
                                    v-for="layer in draft.layers.filter((layer) => layer.visible)"
                                    :key="layer.id"
                                    role="button"
                                    tabindex="0"
                                    :aria-label="t('graphics.composer.selectNamed', { name: layer.name })"
                                    class="absolute cursor-move outline-offset-1 focus-visible:outline-2 focus-visible:outline-accent"
                                    :class="selectedId === layer.id ? 'outline-2 outline-cyan-300 z-10' : ''"
                                    :style="{
                                        left: layer.x + '%',
                                        top: layer.y + '%',
                                        width: layer.width + '%',
                                        height: layer.height + '%',
                                    }"
                                    @keydown.enter="selectedId = layer.id"
                                    @pointerdown.stop="startDrag($event, layer, 'move')"
                                >
                                    <template v-if="selectedId === layer.id">
                                        <button
                                            v-for="handle in ['width', 'height', 'both']"
                                            :key="handle"
                                            class="absolute w-4 h-4 border-2 border-cyan-300 bg-base-100 rounded-xs z-10"
                                            :class="
                                                handle === 'width'
                                                    ? '-right-2 top-1/2 -translate-y-1/2 cursor-ew-resize'
                                                    : handle === 'height'
                                                      ? 'left-1/2 -bottom-2 -translate-x-1/2 cursor-ns-resize'
                                                      : '-right-2 -bottom-2 cursor-nwse-resize'
                                            "
                                            :aria-label="t('graphics.composer.handles.' + handle)"
                                            @pointerdown.stop="startDrag($event, layer, handle)"
                                        />
                                    </template>
                                </div>
                            </template>
                        </template>
                    </TemplatePreview>
                </div>
                <p class="m-0 text-xs text-muted font-mono" aria-live="polite">
                    {{
                        selected
                            ? `x ${formatDimension('x')} · y ${formatDimension('y')} · w ${formatDimension('width')} · h ${formatDimension('height')} ${unit}`
                            : t('graphics.composer.empty')
                    }}
                </p>
                <section class="card-box p-3 flex flex-col gap-2" :aria-label="t('graphics.composer.timeline')">
                    <h2 class="text-sm font-semibold m-0">{{ t('graphics.composer.timeline') }}</h2>
                    <label class="field-label"
                        >{{ t('graphics.composer.playhead')
                        }}<input
                            type="range"
                            min="0"
                            :max="timelineEnd"
                            step="0.01"
                            :value="time ?? 0"
                            @input="seekPreview"
                    /></label>
                    <div
                        v-for="layer in draft.layers"
                        :key="layer.id"
                        class="flex gap-2 items-center text-xs"
                        :class="{ 'opacity-40': !layer.visible }"
                    >
                        <button class="w-24 truncate text-left cursor-pointer" @click="selectedId = layer.id">
                            {{ layer.name }}
                        </button>
                        <div
                            class="relative flex-1 h-6 bg-base-300 rounded overflow-hidden"
                            :aria-label="`${layer.name}: ${layer.delay_ms} ms + ${layer.animation_ms} ms`"
                        >
                            <div
                                class="absolute inset-y-1 bg-accent/25"
                                :style="{
                                    left: (layer.delay_ms / 1000 / timelineEnd) * 100 + '%',
                                    width:
                                        Math.max(
                                            0,
                                            (((draft.duration || timelineEnd) - layer.delay_ms / 1000) / timelineEnd) *
                                                100,
                                        ) + '%',
                                }"
                            />
                            <div
                                class="absolute inset-y-1 bg-accent rounded"
                                :style="{
                                    left: (layer.delay_ms / 1000 / timelineEnd) * 100 + '%',
                                    width:
                                        (layer.animation_in === 'none'
                                            ? 0
                                            : (layer.animation_ms / 1000 / timelineEnd) * 100) + '%',
                                }"
                            />
                            <div
                                v-if="draft.duration && layer.animation_out !== 'none'"
                                class="absolute inset-y-1 bg-indigo-400 rounded"
                                :style="{
                                    left: (draft.duration / timelineEnd) * 100 + '%',
                                    width: (layer.animation_ms / 1000 / timelineEnd) * 100 + '%',
                                }"
                            />
                            <div
                                v-if="time !== undefined"
                                class="absolute inset-y-0 border-l border-white"
                                :style="{ left: (time / timelineEnd) * 100 + '%' }"
                            />
                        </div>
                    </div>
                    <p class="m-0 text-xs text-muted">
                        {{ t('graphics.composer.timelineHint') }} {{ t('graphics.composer.previewHint') }}
                    </p>
                    <button v-if="time !== undefined" class="btn-quiet" @click="stopPreview">
                        {{ t('graphics.composer.backToEdit') }}
                    </button>
                </section>
            </main>
            <aside class="card-box p-3 flex flex-col gap-3 min-w-0" :aria-label="t('graphics.composer.properties')">
                <template v-if="selected">
                    <label class="field-label"
                        >{{ t('graphics.composer.layerName')
                        }}<input
                            :value="selected.name"
                            class="field-input"
                            maxlength="100"
                            @change="updateSelectedLayer({ name: ($event.target as HTMLInputElement).value })"
                    /></label>
                    <button class="btn-quiet text-error" @click="removeLayer(selected.id)">
                        {{ t('graphics.composer.delete') }}
                    </button>
                    <div class="flex items-center justify-between">
                        <strong class="text-sm">{{ t('graphics.composer.position') }}</strong>
                        <div class="flex gap-1">
                            <button
                                v-for="choice in ['%', 'px']"
                                :key="choice"
                                class="btn-quiet min-h-9"
                                :class="{ 'text-accent bg-base-300': unit === choice }"
                                :aria-pressed="unit === choice"
                                @click="unit = choice"
                            >
                                {{ choice }}
                            </button>
                        </div>
                    </div>
                    <div class="grid grid-cols-2 gap-2">
                        <label v-for="key in ['x', 'y', 'width', 'height'] as const" :key="key" class="field-label"
                            >{{ t('graphics.composer.dimensions.' + key)
                            }}<input
                                class="field-input font-mono"
                                type="number"
                                step="any"
                                :value="formatDimension(key)"
                                @change="setDimension(key, ($event.target as HTMLInputElement).value)"
                        /></label>
                    </div>
                    <label class="flex items-center gap-2 text-sm"
                        ><input v-model="proportional" type="checkbox" class="checkbox checkbox-sm" />{{
                            t('graphics.composer.proportional')
                        }}</label
                    >
                    <label class="field-label"
                        >{{ t('graphics.composer.scale') }} (%)<input
                            type="number"
                            class="field-input font-mono"
                            min="1"
                            :value="scaleValue"
                            @change="setScale(Number(($event.target as HTMLInputElement).value))"
                    /></label>
                    <div class="grid grid-cols-4 gap-1">
                        <button
                            v-for="factor in [50, 75, 100, 150]"
                            :key="factor"
                            class="btn-quiet min-h-10 px-1!"
                            @click="setScale(factor)"
                        >
                            {{ factor }}%
                        </button>
                    </div>
                    <div class="flex flex-wrap gap-1">
                        <button class="btn-quiet min-h-10" @click="updateSelectedLayer({ x: 50 - selected.width / 2 })">
                            {{ t('graphics.composer.centerH') }}</button
                        ><button
                            class="btn-quiet min-h-10"
                            @click="updateSelectedLayer({ y: 50 - selected.height / 2 })"
                        >
                            {{ t('graphics.composer.centerV') }}</button
                        ><button
                            class="btn-quiet min-h-10"
                            @click="updateSelectedLayer(fitLayer(selected, { x: 0, width: 100 }, proportional))"
                        >
                            {{ t('graphics.composer.fullWidth') }}
                        </button>
                    </div>
                    <template v-if="selected.kind === 'text' || selected.kind === 'clock'">
                        <template v-if="selected.kind === 'clock'">
                            <label class="field-label"
                                >{{ t('graphics.composer.clockFormat') }}
                                <select
                                    class="field-input"
                                    :aria-label="t('graphics.composer.clockFormat')"
                                    :value="selected.clock_format || 'HH:mm:ss'"
                                    @change="
                                        updateSelectedLayer({
                                            clock_format: ($event.target as HTMLSelectElement).value,
                                        })
                                    "
                                >
                                    <option v-for="format in CLOCK_FORMATS" :key="format" :value="format">
                                        {{ format }}
                                    </option>
                                </select>
                            </label>
                            <label class="field-label"
                                >{{ t('graphics.composer.clockTimezone')
                                }}<input
                                    class="field-input"
                                    :value="selected.clock_timezone || ''"
                                    :placeholder="t('graphics.composer.channelTimezone')"
                                    @change="
                                        updateSelectedLayer({
                                            clock_timezone: ($event.target as HTMLInputElement).value.trim() || null,
                                        })
                                    "
                            /></label>
                            <p class="text-xs text-muted m-0">{{ t('graphics.composer.clockHint') }}</p>
                        </template>
                        <label v-if="selected.kind === 'text'" class="field-label"
                            >{{ t('graphics.composer.text')
                            }}<textarea
                                class="field-input min-h-20"
                                :value="selected.text"
                                maxlength="1000"
                                @change="updateSelectedLayer({ text: ($event.target as HTMLTextAreaElement).value })"
                            />
                        </label>
                        <p v-if="selected.kind === 'text'" class="m-0 text-xs text-muted">
                            {{ t('graphics.composer.fieldsHint') }}
                        </p>
                        <label class="field-label"
                            >{{ t('graphics.composer.font')
                            }}<select
                                class="field-input"
                                :value="selected.font_family ?? ''"
                                @change="
                                    updateSelectedLayer({
                                        font_family: ($event.target as HTMLSelectElement).value || null,
                                    })
                                "
                            >
                                <option value="">{{ t('graphics.composer.defaultFont') }}</option>
                                <option v-for="font in uploadedFonts" :key="font.id" :value="font.family">
                                    {{ font.name }} — {{ font.filename }} ({{ font.id.slice(0, 6) }})
                                </option>
                                <option
                                    v-if="
                                        selected.font_family?.startsWith('NoArFont_') &&
                                        !uploadedFonts.some((font) => font.family === selected?.font_family)
                                    "
                                    :value="selected.font_family"
                                >
                                    {{ t('graphics.composer.fontUnavailable') }}
                                </option>
                                <option v-for="font in fonts" :key="font" :value="font">{{ font }}</option>
                            </select></label
                        >
                        <label class="btn-quiet cursor-pointer inline-flex items-center justify-center">
                            {{
                                fontUploading ? t('graphics.composer.fontUploading') : t('graphics.composer.uploadFont')
                            }}
                            <input
                                type="file"
                                accept=".ttf,.otf"
                                class="sr-only"
                                :aria-label="t('graphics.composer.uploadFont')"
                                :disabled="fontUploading"
                                @change="uploadFont"
                            />
                        </label>
                        <p class="text-xs text-muted m-0">{{ t('graphics.composer.fontHint') }}</p>
                        <label class="field-label"
                            >{{ t('graphics.composer.fontSize') }} (%)<input
                                class="field-input"
                                type="number"
                                step="0.1"
                                min="0.1"
                                max="50"
                                :value="selected.font_size"
                                @change="
                                    updateSelectedLayer({
                                        font_size: Number(($event.target as HTMLInputElement).value),
                                    })
                                "
                        /></label>
                        <label class="flex items-center gap-2 text-sm"
                            ><input
                                type="checkbox"
                                class="checkbox checkbox-sm"
                                :checked="selected.bold"
                                @change="updateSelectedLayer({ bold: ($event.target as HTMLInputElement).checked })"
                            />{{ t('graphics.composer.bold') }}</label
                        >
                        <label class="field-label"
                            >{{ t('graphics.composer.color')
                            }}<input
                                class="field-input"
                                :value="selected.color"
                                @change="updateSelectedLayer({ color: ($event.target as HTMLInputElement).value })"
                        /></label>
                        <label class="field-label"
                            >{{ t('graphics.composer.align')
                            }}<select
                                class="field-input"
                                :value="selected.align"
                                @change="
                                    updateSelectedLayer({
                                        align: ($event.target as HTMLSelectElement).value as TemplateLayer['align'],
                                    })
                                "
                            >
                                <option
                                    v-for="align in ['left', 'center', 'right'] as const"
                                    :key="align"
                                    :value="align"
                                >
                                    {{ t('graphics.composer.alignments.' + align) }}
                                </option>
                            </select></label
                        >
                    </template>
                    <template v-if="selected.kind === 'video'">
                        <label class="field-label">
                            {{ t('graphics.composer.videoCorner') }}
                            <select
                                class="field-input"
                                :aria-label="t('graphics.composer.videoCorner')"
                                :value="videoCorner"
                                @change="setVideoWindow(($event.target as HTMLSelectElement).value, selected.width)"
                            >
                                <option value="custom">{{ t('graphics.composer.freeWindow') }}</option>
                                <option
                                    v-for="corner in ['top-left', 'top-right', 'bottom-left', 'bottom-right']"
                                    :key="corner"
                                    :value="corner"
                                >
                                    {{ t(`lshape.corners.${corner}`) }}
                                </option>
                            </select>
                        </label>
                        <label class="field-label">
                            {{ t('graphics.composer.videoSize') }} (%)
                            <input
                                type="number"
                                min="30"
                                max="95"
                                class="field-input"
                                :value="selected.width"
                                :aria-label="t('graphics.composer.videoSize')"
                                @change="
                                    setVideoWindow(
                                        videoCorner === 'custom' ? 'top-right' : videoCorner,
                                        Number(($event.target as HTMLInputElement).value),
                                    )
                                "
                            />
                        </label>
                    </template>
                    <template v-if="selected.kind === 'image'">
                        <button
                            v-if="assignment === 'logo' && selected.id === draft.control_layer_id"
                            class="btn-quiet"
                            :disabled="busy || !selected.image"
                            @click="restoreLogoPlacement"
                        >
                            {{ t('graphics.composer.restoreLogoPlacement') }}
                        </button>
                        <p
                            v-if="assignment === 'logo' && selected.id === draft.control_layer_id"
                            class="m-0 text-xs text-muted"
                        >
                            {{ t('graphics.composer.restoreLogoPlacementHint') }}
                        </p>
                        <label class="field-label"
                            >{{ t('graphics.composer.image')
                            }}<input
                                class="field-input"
                                :value="selected.image ?? ''"
                                @change="updateSelectedLayer({ image: ($event.target as HTMLInputElement).value })"
                        /></label>
                        <p class="m-0 text-xs text-muted">{{ t('graphics.composer.imageHint') }}</p>
                        <label class="field-label"
                            >{{ t('graphics.composer.upload')
                            }}<input
                                type="file"
                                accept="image/png,image/jpeg,image/webp,image/gif"
                                class="max-w-full text-xs"
                                @change="uploadImage"
                        /></label>
                        <button class="btn-quiet" @click="picking = !picking">
                            {{ t('graphics.composer.pickImage') }}
                        </button>
                        <div
                            v-if="picking"
                            class="h-64 shrink-0 overflow-auto border border-line-strong rounded-field p-2"
                        >
                            <StoragePicker class="h-full" hide-lives images-only @pick="pickImage" />
                        </div>
                    </template>
                    <template v-if="selected.kind === 'box'">
                        <label class="field-label"
                            >{{ t('graphics.composer.shape') }}
                            <select
                                class="field-input"
                                :aria-label="t('graphics.composer.shape')"
                                :value="selected.shape || 'rectangle'"
                                @change="
                                    setShape(
                                        ($event.target as HTMLSelectElement).value as NonNullable<
                                            TemplateLayer['shape']
                                        >,
                                    )
                                "
                            >
                                <option
                                    v-for="shape in ['rectangle', 'rounded', 'ellipse', 'line'] as const"
                                    :key="shape"
                                    :value="shape"
                                >
                                    {{ t(`graphics.composer.shapes.${shape}`) }}
                                </option>
                            </select>
                        </label>
                        <label class="field-label"
                            >{{ t('graphics.composer.stroke')
                            }}<input
                                class="field-input"
                                :value="selected.stroke || ''"
                                placeholder="#rrggbb"
                                @change="
                                    updateSelectedLayer({ stroke: ($event.target as HTMLInputElement).value || null })
                                "
                        /></label>
                        <label class="field-label"
                            >{{ t('graphics.composer.strokeWidth') }} (%)<input
                                class="field-input"
                                type="number"
                                min="0"
                                max="20"
                                step="0.1"
                                :value="selected.stroke_width ?? 0.25"
                                @change="
                                    updateSelectedLayer({
                                        stroke_width: Number(($event.target as HTMLInputElement).value),
                                    })
                                "
                        /></label>
                    </template>
                    <template v-if="selected.kind !== 'video'">
                        <label class="field-label"
                            >{{ t('graphics.composer.fill')
                            }}<input
                                class="field-input"
                                :value="selected.fill ?? ''"
                                placeholder="#rrggbb"
                                @change="
                                    updateSelectedLayer({ fill: ($event.target as HTMLInputElement).value || null })
                                "
                        /></label>
                        <label class="field-label"
                            >{{ t('graphics.composer.radius') }} (%)<input
                                class="field-input"
                                type="number"
                                min="0"
                                max="50"
                                step="0.1"
                                :value="selected.radius"
                                @change="
                                    updateSelectedLayer({ radius: Number(($event.target as HTMLInputElement).value) })
                                "
                        /></label>
                    </template>
                    <label class="field-label"
                        >{{ t('graphics.composer.opacity')
                        }}<input
                            class="field-input"
                            type="number"
                            min="0"
                            max="1"
                            step="0.05"
                            :value="selected.opacity"
                            @change="
                                updateSelectedLayer({ opacity: Number(($event.target as HTMLInputElement).value) })
                            "
                    /></label>
                    <div class="border-t border-line-strong pt-3 flex flex-col gap-3">
                        <h3 class="m-0 text-sm font-semibold">{{ t('graphics.composer.animation') }}</h3>
                        <label v-for="key in ['animation_in', 'animation_out'] as const" :key="key" class="field-label"
                            >{{ t('graphics.composer.' + key)
                            }}<select
                                class="field-input"
                                :value="selected[key]"
                                @change="updateSelectedLayer({ [key]: ($event.target as HTMLSelectElement).value })"
                            >
                                <option v-for="animation in LAYER_ANIMATIONS" :key="animation" :value="animation">
                                    {{ t('graphics.composer.animations.' + animation) }}
                                </option>
                            </select></label
                        >
                        <label v-for="key in ['animation_ms', 'delay_ms'] as const" :key="key" class="field-label"
                            >{{ t('graphics.composer.' + key) }} (ms)<input
                                class="field-input"
                                type="number"
                                min="0"
                                max="10000"
                                :value="selected[key]"
                                @change="
                                    updateSelectedLayer({ [key]: Number(($event.target as HTMLInputElement).value) })
                                "
                        /></label>
                    </div>
                </template>
                <label
                    v-if="!draft.layers.some((layer) => layer.kind === 'video')"
                    class="flex items-center gap-2 border-t border-line-strong pt-3"
                >
                    <input
                        type="checkbox"
                        class="checkbox checkbox-accent"
                        :checked="Boolean(draft.always_on)"
                        @change="setAlwaysOn(($event.target as HTMLInputElement).checked)"
                    />
                    {{ t('graphics.composer.alwaysOn') }}
                </label>
                <p v-if="!draft.layers.some((layer) => layer.kind === 'video')" class="m-0 text-xs text-muted">
                    {{ t('graphics.composer.alwaysOnHint') }}
                </p>
                <label v-if="!draft.always_on" class="field-label border-t border-line-strong pt-3"
                    >{{ t('graphics.templates.duration')
                    }}<input
                        v-model.number="draft.duration"
                        class="field-input"
                        type="number"
                        min="0"
                        max="86400"
                        step="0.1"
                    /><span class="text-xs text-muted">{{ t('graphics.templates.durationHint') }}</span></label
                >
                <label v-if="!draft.always_on" class="field-label"
                    >{{ t('graphics.composer.duringLshape')
                    }}<select v-model="draft.during_lshape" class="field-input">
                        <option v-for="rule in ['follow', 'wait', 'skip'] as const" :key="rule" :value="rule">
                            {{ t('graphics.templates.lshape.' + rule) }}
                        </option>
                    </select></label
                >
            </aside>
        </div>
    </section>
</template>

<style scoped>
.composer-canvas {
    background-color: #24272c;
    background-image:
        linear-gradient(to right, #ffffff24 1px, transparent 1px),
        linear-gradient(to bottom, #ffffff24 1px, transparent 1px),
        conic-gradient(#30343a 25%, transparent 0 50%, #30343a 0 75%, transparent 0);
    background-size:
        10% 10%,
        10% 10%,
        20px 20px;
}
.composer-safe-area {
    position: absolute;
    z-index: 20;
    pointer-events: none;
    border: 1px dashed;
    box-shadow: 0 0 0 1px #0009;
}
.composer-action-safe {
    inset: 5%;
    border-color: #fff;
}
.composer-title-safe {
    inset: 10%;
    border-color: #fbbf24;
}
.composer-grid {
    display: grid;
    gap: 1rem;
    grid-template-columns: minmax(0, 1fr);
    align-items: start;
}
@media (min-width: 1100px) {
    .composer-grid {
        grid-template-columns: 210px minmax(0, 1fr) 280px;
    }
    .composer-grid > aside {
        max-height: calc(100vh - 190px);
        overflow-y: auto;
    }
}
</style>
