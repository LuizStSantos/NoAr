<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'

import { type DataField, sourceBindings, useGraphicsSources } from '@/composables/graphicsSources'
import { type GraphicsFont, loadGraphicsFont } from '@/composables/graphicsFonts'
import ComposerCanvas from '@/components/graphics/composer/ComposerCanvas.vue'
import ComposerLayerList from '@/components/graphics/composer/ComposerLayerList.vue'
import ComposerFieldMappings from '@/components/graphics/composer/ComposerFieldMappings.vue'
import ComposerTimeline from '@/components/graphics/composer/ComposerTimeline.vue'
import ComposerLayerInspector, { type InspectorTab } from '@/components/graphics/composer/ComposerLayerInspector.vue'
import StoragePicker from '@/components/schedule/StoragePicker.vue'
import { sendFileInChunks } from '@/composables/chunkedUpload'
import { registerEditorNavigationGuard } from '@/composables/editorNavigation'
import { useGraphicsDisplayNames } from '@/composables/graphicsDisplayNames'
import { useAuth } from '@/stores/auth'
import { randomId } from '@/composables/browser'
import { storageImageUrl } from '@/composables/graphicsImages'
import { AuthFetchError, authFetch } from '@/composables/authFetch'
import {
    type GraphicsTemplate,
    type TemplateLayer,
    type TemplateInput,
    fitLayer,
    templateFields,
    fieldList,
    useGraphicsApi,
    graphicsUploadFilename,
    sanitizeGraphicsUploadFilename,
} from '@/composables/graphicsTemplates'
import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'
import { useComposerGeometry } from '@/composables/graphicsComposerGeometry'
import { useComposerPreview } from '@/composables/graphicsComposerPreview'

/**
 * Route entry and single owner of the draft, undo history, persistence and asset requests.
 * Child panels emit commands; only an explicit save changes the stored template.
 */
const { t } = useI18n()
const { templateName, layerName } = useGraphicsDisplayNames()
const configStore = useConfig()
const graphicsApi = useGraphicsApi()
const { sources: dataSources } = useGraphicsSources()
const channelId = computed(() => configStore.channels[configStore.i]?.id)
const route = useRoute()
const router = useRouter()
const indexStore = useIndex()
const authStore = useAuth()
const draft = ref<TemplateInput>({
    name: '',
    group: '',
    duration: 8,
    during_lshape: 'follow',
    layers: [],
    always_on: false,
})
// These editable labels project defaults; only a deliberate rename changes the raw draft.
const displayedName = computed({
    get: () => templateName(draft.value),
    set: (value: string) => {
        if (value !== templateName(draft.value)) {
            draft.value.name = value
        }
    },
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
const navigationPending = ref(false)
/** The properties tab in use, kept for this browser (a convenience: it falls back to Position). */
const INSPECTOR_TABS: InspectorTab[] = ['position', 'content', 'style', 'animation', 'template']
const inspectorTab = ref<InspectorTab>(
    (() => {
        try {
            const saved = localStorage.getItem('composer-tab') as InspectorTab | null
            return saved && INSPECTOR_TABS.includes(saved) ? saved : 'position'
        } catch {
            return 'position'
        }
    })(),
)
watch(inspectorTab, (value) => {
    try {
        localStorage.setItem('composer-tab', value)
    } catch {
        // Private windows may refuse storage; the tab simply is not remembered.
    }
})
const saving = ref(false)
const busy = ref(false)
const editorReady = ref(false)
const editorLocked = computed(() => busy.value || navigationPending.value || fontUploading.value)
const error = ref('')
const note = ref('')
const adding = ref(false)
const picking = ref(false)
const output = computed(() => configStore.playout?.output)
const frame = computed(() => ({ width: output.value?.width || 1920, height: output.value?.height || 1080 }))
const {
    proportional,
    unit,
    videoCorner,
    scaleValue,
    setVideoWindow,
    setDimension,
    formatDimension,
    setScale,
    rememberScaleBase,
    resetScaleBases,
} = useComposerGeometry(selected, frame, updateSelectedLayer)
const { time, playing, timelineEnd, previewDuration, seekPreview, stopPreview, playPreview } = useComposerPreview(
    () => draft.value,
)
/** While the preview plays or is scrubbed, the template leaves after its preview time, so its exit shows too. */
const canvasTemplate = computed(() =>
    time.value === undefined ? preview.value : { ...preview.value, duration: previewDuration.value },
)
let loadVersion = 0
let interactionSnapshot: string | undefined
const dirty = computed(
    () => JSON.stringify(draft.value) !== baseline.value || assignment.value !== originalAssignment.value,
)

/** Every authoring request belongs to one document, channel and layer object. */
function captureAuthoringOperation(layer: TemplateLayer) {
    const version = loadVersion
    const document = draft.value
    const channel = configStore.channels[configStore.i]
    const storage = channel ? { id: channel.id, storage: channel.storage } : undefined
    const dimensions = { ...frame.value }

    return {
        channel: storage,
        frame: dimensions,
        current: () =>
            version === loadVersion &&
            document === draft.value &&
            storage?.id === channelId.value &&
            !navigationPending.value &&
            document.layers.includes(layer),
    }
}

type AuthoringOperation = ReturnType<typeof captureAuthoringOperation>

function mapField(field: string, binding?: DataField) {
    if (editorLocked.value) {
        return
    }

    rememberDraft()

    for (const layer of draft.value.layers) {
        layer.data_fields = { ...layer.data_fields }

        if (binding && templateFields([layer]).includes(field)) {
            layer.data_fields[field] = binding
        } else {
            delete layer.data_fields[field]
        }
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
    if (editorLocked.value) {
        return
    }

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
    if (!selected.value || busy.value || navigationPending.value) {
        return
    }

    applyLayerPatch(selected.value, patch)
}

/** Internal operations can publish their checked result while the editor stays locked. */
function applyLayerPatch(layer: TemplateLayer, patch: Partial<TemplateLayer>) {
    rememberDraft()

    if (patch.animation_ms !== undefined) {
        patch.clip_animation_ms = null
    }

    Object.assign(layer, patch)
    pruneLayerMappings(layer)
}

function pruneLayerMappings(layer: TemplateLayer) {
    if (layer.data_fields) {
        const fields = templateFields([layer])
        layer.data_fields = Object.fromEntries(
            Object.entries(layer.data_fields).filter(([field]) => fields.includes(field)),
        )
    }
}

function setAlwaysOn(value: boolean) {
    if (editorLocked.value) {
        return
    }

    rememberDraft()
    draft.value.always_on = value
    draft.value.duration = 0
}

function addLayer(kind: TemplateLayer['kind']) {
    if (
        editorLocked.value ||
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
    // A new layer starts with what it shows: its text, image, shape or clock.
    inspectorTab.value = kind === 'video' ? 'position' : 'content'
    rememberScaleBase(layer)
    selectedId.value = layer.id
    adding.value = false
    stopPreview()
}

function removeLayer(id: string) {
    const layer = draft.value.layers.find((layer) => layer.id === id)

    if (!layer || editorLocked.value) {
        return
    }

    rememberDraft()
    draft.value.layers = draft.value.layers.filter((layer) => layer.id !== id)

    if (selectedId.value === id) {
        selectedId.value = draft.value.layers[0]?.id ?? ''
    }

    note.value = t('graphics.composer.removed', { name: layerName(layer) })
}

/** Move a layer one step in front of (drawn after) or behind the next one. */
function moveLayer(id: string, toward: 'front' | 'back') {
    const layers = draft.value.layers
    const index = layers.findIndex((layer) => layer.id === id)
    const target = toward === 'front' ? index + 1 : index - 1

    if (editorLocked.value || index < 0 || target < 0 || target >= layers.length) {
        return
    }

    rememberDraft()
    const reordered = [...layers]
    ;[reordered[index], reordered[target]] = [reordered[target]!, reordered[index]!]
    draft.value.layers = reordered
}

/** Put a layer at `target` in the drawing order (0 = back), as a drag in the list does. */
function reorderLayer(id: string, target: number) {
    const layers = draft.value.layers
    const index = layers.findIndex((layer) => layer.id === id)

    if (editorLocked.value || index < 0 || target < 0 || target >= layers.length || index === target) {
        return
    }

    rememberDraft()
    const reordered = [...layers]
    const [layer] = reordered.splice(index, 1)
    reordered.splice(target, 0, layer!)
    draft.value.layers = reordered
}

function toggleLayer(layer: TemplateLayer) {
    if (editorLocked.value) {
        return
    }

    rememberDraft()
    layer.visible = !layer.visible
}

function startLayerInteraction(layerId: string) {
    stopPreview()
    selectedId.value = layerId
    interactionSnapshot = JSON.stringify(draft.value)
}

function updateLayerInteraction(layerId: string, patch: Partial<TemplateLayer>) {
    const layer = draft.value.layers.find((candidate) => candidate.id === layerId)

    if (layer) {
        Object.assign(layer, patch)
    }
}

/** Commit a whole pointer gesture once, rather than every previewed movement. */
function finishLayerInteraction() {
    if (interactionSnapshot && JSON.stringify(draft.value) !== interactionSnapshot) {
        history.value.push(interactionSnapshot)
        note.value = ''
    }

    interactionSnapshot = undefined
}

/** Ignore stale requests and keep the document read-only until its entire load succeeds. */
async function loadEditor() {
    const version = ++loadVersion

    stopPreview()

    busy.value = true
    fontUploading.value = false
    interactionSnapshot = undefined
    saving.value = adding.value = picking.value = false
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
        resetScaleBases(draft.value.layers)

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

/** Bind each save to the submitted document, channel and editor generation. */
async function saveTemplate() {
    if (editorLocked.value || !editorReady.value || !draft.value.name.trim()) {
        return
    }

    const version = loadVersion
    const channel = channelId.value
    const templateId = savedId.value
    const role = assignment.value
    const previousRole = originalAssignment.value
    const input: TemplateInput = JSON.parse(JSON.stringify(draft.value))
    input.layers.forEach(pruneLayerMappings)
    input.control_layer_id ??= input.layers.find((layer) => layer.kind === 'image')?.id ?? null
    const api = useGraphicsApi(() => channel)
    const current = () => version === loadVersion && channel === channelId.value && !navigationPending.value

    busy.value = true
    error.value = ''
    try {
        const changedAssignment = role !== previousRole
        const result = await api.save(
            input,
            templateId,
            changedAssignment ? { role: role || null, control_layer_id: input.control_layer_id } : undefined,
        )

        if (!current()) {
            return
        }

        savedId.value = result.id
        originalAssignment.value = role
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
        // Saved: back to the templates, where the saved one is listed.
        indexStore.msgAlert('success', t('graphics.composer.saved', { name: templateName(result) }), 3)
        await router.push({ name: 'graphics-templates', query: { channel } })
    } catch (err) {
        if (current()) {
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
        }
    } finally {
        if (current()) {
            busy.value = false
        }
    }
}

/** Publish uploaded fonts only into the document that initiated the request. */
async function uploadFont(event: Event) {
    const input = event.target as HTMLInputElement
    const file = input.files?.[0]
    const layer = selected.value
    input.value = ''

    if (!file || !layer || editorLocked.value) {
        return
    }

    const operation = captureAuthoringOperation(layer)
    const channel = operation.channel?.id

    if (channel === undefined) {
        return
    }

    fontUploading.value = true
    error.value = ''
    try {
        const asset = await authFetch<GraphicsFont>(
            `/api/graphics/${channel}/font-assets?filename=${encodeURIComponent(file.name)}`,
            {
                method: 'POST',
                body: file,
                headers: { 'Content-Type': 'application/octet-stream' },
            },
        )

        if (!operation.current()) {
            return
        }

        await loadGraphicsFont(channel, asset)

        if (operation.current()) {
            uploadedFonts.value.push(asset)
            applyLayerPatch(layer, { font_family: asset.family })
        }
    } catch (failure) {
        if (operation.current()) {
            error.value = String(failure)
        }
    } finally {
        if (operation.current()) {
            fontUploading.value = false
        }
    }
}

async function uploadImage(event: Event) {
    const input = event.target as HTMLInputElement
    const original = input.files?.[0]
    const layer = selected.value
    const channel = channelId.value

    if (!original || !layer || editorLocked.value || channel === undefined) {
        return
    }

    const operation = captureAuthoringOperation(layer)
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

        while (operation.current()) {
            const name = graphicsUploadFilename(cleanedName, number)
            const file = new File([original], name, { type: original.type, lastModified: original.lastModified })

            try {
                await sendFileInChunks(
                    file,
                    { url: `/api/file/${channel}/upload?path=Graphics`, headers: authStore.authHeader },
                    (bytes) => {
                        if (operation.current()) {
                            note.value = t('graphics.composer.uploadProgress', {
                                percent: Math.round((bytes / file.size) * 100),
                            })
                        }
                    },
                )

                await setImage(layer, 'Graphics/' + name, operation)
                if (operation.current()) {
                    note.value = t('graphics.composer.uploaded')
                }
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
        if (operation.current()) {
            error.value = uploadError instanceof Error ? uploadError.message : String(uploadError)
        }
    } finally {
        if (operation.current()) {
            busy.value = false
        }
        input.value = ''
    }
}

/** Explicit repair for a previously saved full-frame logo; never rewrite an authored model automatically. */
async function restoreLogoPlacement() {
    const layer = selected.value

    if (!layer?.image || editorLocked.value) {
        return
    }

    const operation = captureAuthoringOperation(layer)
    const dimensions = operation.frame
    busy.value = true
    error.value = ''

    try {
        const source = await storageImageUrl(operation.channel, layer.image)
        const image = new Image()
        image.src = source
        await image.decode()

        if (!operation.current() || selected.value !== layer) {
            return
        }

        const availableWidth = Math.max(2, dimensions.width - 24)
        const availableHeight = Math.max(2, dimensions.height - 24)
        const factor = Math.min(1, availableWidth / image.naturalWidth, availableHeight / image.naturalHeight)
        const width = Math.max(2, Math.floor(image.naturalWidth * factor) & ~1)
        const height = Math.max(2, Math.floor(image.naturalHeight * factor) & ~1)
        applyLayerPatch(layer, {
            x: (Math.max(0, dimensions.width - width - 12) * 100) / dimensions.width,
            y: (12 * 100) / dimensions.height,
            width: (width * 100) / dimensions.width,
            height: (height * 100) / dimensions.height,
        })
        rememberScaleBase(layer)
    } catch (failure) {
        if (operation.current()) {
            error.value = failure instanceof Error ? failure.message : String(failure)
        }
    } finally {
        if (operation.current()) {
            busy.value = false
        }
    }
}

/** Size only the first image in an empty layer; replacements retain authored geometry. */
async function setImage(layer: TemplateLayer, path: string, operation: AuthoringOperation) {
    if (!operation.current()) {
        return
    }

    const dimensions = operation.frame
    let geometry: Partial<TemplateLayer> = {}

    if (!layer.image) {
        const source = await storageImageUrl(operation.channel, path)
        const image = new Image()
        image.src = source
        await image.decode()

        const factor = Math.min(1, dimensions.width / image.naturalWidth, dimensions.height / image.naturalHeight)
        const width = (image.naturalWidth * factor * 100) / dimensions.width
        const height = (image.naturalHeight * factor * 100) / dimensions.height
        geometry = {
            x: Math.max(0, Math.min(layer.x, 100 - width)),
            y: Math.max(0, Math.min(layer.y, 100 - height)),
            width,
            height,
        }
    }

    if (!operation.current()) {
        return
    }

    applyLayerPatch(layer, { ...geometry, image: path })
    rememberScaleBase(layer)
}

async function pickImage(item: { path: string }) {
    if (!/\.(png|jpe?g|webp|gif)$/i.test(item.path)) {
        error.value = t('graphics.composer.imageOnly')
        return
    }

    const layer = selected.value

    if (!layer || editorLocked.value) {
        return
    }

    const operation = captureAuthoringOperation(layer)
    busy.value = true
    error.value = ''

    try {
        await setImage(layer, item.path, operation)

        if (operation.current()) {
            picking.value = false
        }
    } catch (failure) {
        if (operation.current()) {
            error.value = failure instanceof Error ? failure.message : String(failure)
        }
    } finally {
        if (operation.current()) {
            busy.value = false
        }
    }
}

function confirmBeforeUnload(event: BeforeUnloadEvent) {
    if (editorReady.value && dirty.value) {
        event.preventDefault()
        event.returnValue = ''
    }
}

const unregisterNavigationGuard = registerEditorNavigationGuard((to) => {
    if (
        to.name === route.name &&
        String(to.query.template ?? '') === String(route.query.template ?? '') &&
        String(to.query.channel ?? '') === String(route.query.channel ?? '')
    ) {
        return true
    }

    if (editorReady.value && dirty.value && !window.confirm(t('graphics.composer.discard'))) {
        return false
    }

    ++loadVersion
    navigationPending.value = true
    return true
})
const unregisterAfterNavigation = router.afterEach((_to, _from, failure) => {
    if (!navigationPending.value) {
        return
    }

    navigationPending.value = false

    if (failure) {
        busy.value = false
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
    unregisterNavigationGuard()
    unregisterAfterNavigation()
    window.removeEventListener('beforeunload', confirmBeforeUnload)
})
</script>
<template>
    <fieldset
        class="composer-page flex flex-col gap-3 min-w-0 border-0 p-0 m-0"
        :aria-label="t('graphics.composer.title')"
        :disabled="editorLocked"
        :inert="editorLocked"
        :aria-busy="busy"
    >
        <header class="flex flex-wrap items-center gap-2" role="toolbar" :aria-label="t('graphics.composer.title')">
            <RouterLink
                :to="{ name: 'graphics-templates', query: { channel: channelId } }"
                class="inline-flex items-center gap-1.5 min-h-9 px-1 text-sm font-medium text-accent-text"
            >
                <i class="bi bi-arrow-left" aria-hidden="true" />{{ t('graphics.editor.back') }}
            </RouterLink>
            <span class="w-px h-6 bg-line-strong" aria-hidden="true" />
            <input
                v-model="displayedName"
                :aria-label="t('graphics.composer.templateName')"
                class="field-input font-semibold w-64 max-w-full min-h-9"
                maxlength="100"
                :placeholder="t('graphics.composer.templateName')"
            />
            <span v-if="draft.group" class="text-xs text-muted px-2 py-1 bg-base-200 rounded">{{ draft.group }}</span>
            <span class="font-mono text-xs text-muted"
                >{{ frame.width }}×{{ frame.height }} · {{ output?.fps || 30 }} fps</span
            >
            <div class="ml-auto flex flex-wrap items-center gap-2">
                <button class="btn-quiet min-h-9" :disabled="!history.length" @click="undo">
                    {{ t('graphics.composer.undo') }}
                </button>
                <button
                    class="btn-quiet min-h-9"
                    :disabled="!draft.layers.length"
                    :aria-label="t(playing ? 'graphics.composer.stop' : 'graphics.composer.play')"
                    @click="playPreview"
                >
                    <i class="bi" :class="playing ? 'bi-stop-fill' : 'bi-play-fill'" aria-hidden="true" />
                    {{ t(playing ? 'graphics.composer.stopShort' : 'graphics.composer.playShort') }}
                </button>
                <RouterLink
                    :to="{ name: 'graphics-onair', query: { channel: channelId, template: savedId } }"
                    class="btn-quiet min-h-9"
                    >{{ t('graphics.composer.openOnAir') }}</RouterLink
                >
                <button class="btn-main min-h-9" :disabled="busy || !editorReady" @click="saving = !saving">
                    {{ t('graphics.composer.saveTemplate') }}
                </button>
            </div>
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
                }}<input v-model="displayedName" class="field-input" required maxlength="100"
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
            <button class="btn-main" :disabled="busy || !draft.name.trim()">{{ t('graphics.composer.save') }}</button>
            <ComposerFieldMappings :fields="fields" :template="preview" :sources="dataSources" @map="mapField">
                <RouterLink
                    class="link-accent text-sm"
                    :to="{ name: 'graphics-sources', query: { channel: channelId } }"
                    >{{ t('graphics.sections.sources') }}</RouterLink
                >
            </ComposerFieldMappings>
        </form>
        <div
            class="composer-grid"
            :class="{ 'opacity-60 pointer-events-none': busy || !editorReady }"
            :aria-busy="busy"
        >
            <ComposerLayerList
                v-model:selected-id="selectedId"
                v-model:adding="adding"
                :layers="draft.layers"
                :always-on="draft.always_on"
                @add="addLayer"
                @toggle="toggleLayer"
                @remove="removeLayer"
                @move="moveLayer"
                @reorder="reorderLayer"
            />
            <main class="min-w-0 min-h-0 flex flex-col gap-2">
                <ComposerCanvas
                    v-model:selected-id="selectedId"
                    :template="canvasTemplate"
                    :values="sampleValues"
                    :time="time"
                    :proportional="proportional"
                    @interaction-start="startLayerInteraction"
                    @interaction-change="updateLayerInteraction"
                    @interaction-end="finishLayerInteraction"
                />
                <p class="m-0 text-xs text-muted font-mono" aria-live="polite">
                    {{
                        selected
                            ? `x ${formatDimension('x')} · y ${formatDimension('y')} · w ${formatDimension('width')} · h ${formatDimension('height')} ${unit}`
                            : t('graphics.composer.empty')
                    }}
                </p>
                <ComposerTimeline
                    v-model:selected-id="selectedId"
                    :layers="draft.layers"
                    :duration="previewDuration"
                    :time="time"
                    :end="timelineEnd"
                    @seek="seekPreview"
                    @stop="stopPreview"
                />
            </main>
            <ComposerLayerInspector
                v-model:tab="inspectorTab"
                :selected="selected"
                :fonts="fonts"
                :uploaded-fonts="uploadedFonts"
                :font-uploading="fontUploading"
                @update-layer="updateSelectedLayer"
                @remove="removeLayer"
                @upload-font="uploadFont"
            >
                <template #geometry>
                    <template v-if="selected">
                        <template v-if="assignment === 'logo' && selected.id === draft.control_layer_id">
                            <button class="btn-quiet" :disabled="busy || !selected.image" @click="restoreLogoPlacement">
                                {{ t('graphics.composer.restoreLogoPlacement') }}
                            </button>
                            <p class="m-0 text-xs text-muted">
                                {{ t('graphics.composer.restoreLogoPlacementHint') }}
                            </p>
                        </template>
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
                            <button
                                class="btn-quiet min-h-10"
                                @click="updateSelectedLayer({ x: 50 - selected.width / 2 })"
                            >
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
                    </template>
                </template>
                <template #video>
                    <template v-if="selected?.kind === 'video'">
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
                </template>
                <template #image>
                    <template v-if="selected?.kind === 'image'">
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
            </ComposerLayerInspector>
        </div>
    </fieldset>
</template>

<style scoped>
.composer-grid {
    display: grid;
    gap: 0.75rem;
    grid-template-columns: minmax(0, 1fr);
    align-items: start;
}
/* Wide screens: the whole editor fits the window. Only the side panels and
   the layer list scroll, never the page. */
@media (min-width: 1100px) {
    .composer-page {
        height: calc(100dvh - 5.5rem);
        min-height: 36rem;
    }
    .composer-grid {
        flex: 1 1 0;
        min-height: 0;
        align-items: stretch;
        grid-template-columns: 216px minmax(0, 1fr) 300px;
    }
    .composer-grid > aside {
        min-height: 0;
        overflow-y: auto;
    }
}
</style>
