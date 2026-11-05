<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { onBeforeRouteLeave, onBeforeRouteUpdate, useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'

import TemplatePreview from '@/components/graphics/TemplatePreview.vue'
import StoragePicker from '@/components/schedule/StoragePicker.vue'
import { sendFileInChunks } from '@/composables/chunkedUpload'
import { useAuth } from '@/stores/auth'
import { AuthFetchError, authFetch } from '@/composables/authFetch'
import {
    type GraphicOnAir,
    type GraphicsTemplate,
    type TemplateLayer,
    type TemplateInput,
    LAYER_ANIMATIONS,
    fitLayer,
    templateFields,
    fieldList,
    useGraphicsApi,
} from '@/composables/graphicsTemplates'
import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'

/** Grafismo › Modelos: the channel's templates as cards, filtered by group. */
const { t } = useI18n()
const configStore = useConfig()
const indexStore = useIndex()
const graphicsApi = useGraphicsApi()
const channelId = computed(() => configStore.channels[configStore.i]?.id)
const route = useRoute()
const router = useRouter()
const authStore = useAuth()
const isAdmin = computed(() => ['global_admin', 'channel_admin'].includes(authStore.role))
const composing = computed(() => route.name === 'graphics-composer')
const draft = ref<TemplateInput>({ name: '', group: '', duration: 8, during_lshape: 'follow', layers: [] })
const selectedId = ref('')
const selected = computed(() => draft.value.layers.find((layer) => layer.id === selectedId.value))
const fields = computed(() => templateFields(draft.value.layers))
const preview = computed<GraphicsTemplate>(() => ({ ...draft.value, id: savedId.value ?? 0, fields: fields.value }))
const sampleValues = computed(() => Object.fromEntries(fields.value.map((field) => [field, '[' + field + ']'])))
const savedId = ref<number>()
const baseline = ref(JSON.stringify(draft.value))
const history = ref<string[]>([])
const fonts = ref<string[]>([])
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
const guideV = ref(false)
const guideH = ref(false)
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
const dirty = computed(() => JSON.stringify(draft.value) !== baseline.value)

function remember() {
    history.value.push(JSON.stringify(draft.value))
    if (history.value.length > 100) history.value.shift()
    note.value = ''
}
function undo() {
    const previous = history.value.pop()
    if (!previous) return
    stopPreview()
    draft.value = JSON.parse(previous)
    if (!selected.value) selectedId.value = draft.value.layers[0]?.id ?? ''
    note.value = ''
}
function update(patch: Partial<TemplateLayer>) {
    if (!selected.value) return
    remember()
    Object.assign(selected.value, patch)
}
function dimension(key: 'x' | 'y' | 'width' | 'height', value: string) {
    if (!selected.value) return
    const number = Number(value.replace(',', '.'))
    if (!Number.isFinite(number)) return
    const axis = key === 'x' || key === 'width' ? frame.value.width : frame.value.height
    update(
        fitLayer(
            selected.value,
            { [key]: unit.value === 'px' ? (number / axis) * 100 : number },
            proportional.value && (key === 'width' || key === 'height'),
        ),
    )
}
function shownDimension(key: 'x' | 'y' | 'width' | 'height') {
    const value = selected.value?.[key] ?? 0
    const axis = key === 'x' || key === 'width' ? frame.value.width : frame.value.height
    return Math.round(value * (unit.value === 'px' ? axis / 100 : 1) * 100) / 100
}
function scale(value: number) {
    const layer = selected.value
    if (!layer || !Number.isFinite(value) || value <= 0) return
    const base = scaleBases.get(layer.id) ?? layer
    update(fitLayer(layer, { width: (base.width * value) / 100, height: (base.height * value) / 100 }, true))
}
const scaleValue = computed(() =>
    selected.value
        ? Math.round((selected.value.width / (scaleBases.get(selected.value.id)?.width ?? selected.value.width)) * 100)
        : 100,
)
function addLayer(kind: TemplateLayer['kind']) {
    if (
        draft.value.layers.length >= 64 ||
        (kind === 'video' && draft.value.layers.some((layer) => layer.kind === 'video'))
    )
        return
    remember()
    const layer: TemplateLayer = {
        id: crypto.randomUUID(),
        name: t('graphics.composer.kinds.' + kind),
        kind,
        visible: true,
        x: kind === 'video' ? 5 : 10,
        y: kind === 'video' ? 5 : 70,
        width: kind === 'video' ? 70 : 40,
        height: kind === 'video' ? 70 : 12,
        opacity: 1,
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
function removeLayer(id: string) {
    const layer = draft.value.layers.find((layer) => layer.id === id)
    if (!layer) return
    remember()
    draft.value.layers = draft.value.layers.filter((layer) => layer.id !== id)
    if (selectedId.value === id) selectedId.value = draft.value.layers[0]?.id ?? ''
    note.value = t('graphics.composer.removed', { name: layer.name })
}
function toggleLayer(layer: TemplateLayer) {
    remember()
    layer.visible = !layer.visible
}
function grab(event: PointerEvent, layer: TemplateLayer, mode: string) {
    if (event.button !== 0 || !canvas.value) return
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
function move(event: PointerEvent) {
    if (!drag) return
    const layer = draft.value.layers.find((layer) => layer.id === drag!.id)
    if (!layer) return
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
function seek(event: Event) {
    stopPreview()
    time.value = Number((event.target as HTMLInputElement).value)
}
function stopPreview() {
    cancelAnimationFrame(animationFrame)
    playing.value = false
    time.value = undefined
}
function play() {
    if (playing.value) {
        stopPreview()
        return
    }
    const start = performance.now()
    playing.value = true
    const tick = (now: number) => {
        time.value = (now - start) / 1000
        if (time.value < timelineEnd.value) animationFrame = requestAnimationFrame(tick)
        else playing.value = false
    }
    animationFrame = requestAnimationFrame(tick)
}
async function loadEditor() {
    if (!composing.value) return
    const version = ++loadVersion
    stopPreview()
    busy.value = true
    editorReady.value = false
    error.value = note.value = ''
    try {
        const id = route.query.template === undefined ? undefined : Number(route.query.template)
        if (id !== undefined && (!Number.isSafeInteger(id) || id <= 0)) throw new Error(t('graphics.composer.notFound'))
        const [template, families] = await Promise.all([
            id === undefined ? undefined : graphicsApi.template(id),
            graphicsApi.fonts(),
        ])
        if (version !== loadVersion) return
        savedId.value = template?.id
        draft.value = template
            ? {
                  name: template.name,
                  group: template.group,
                  duration: template.duration,
                  during_lshape: template.during_lshape,
                  layers: template.layers,
              }
            : { name: '', group: '', duration: 8, during_lshape: 'follow', layers: [] }
        fonts.value = families
        scaleBases.clear()
        for (const layer of draft.value.layers) scaleBases.set(layer.id, { width: layer.width, height: layer.height })
        selectedId.value = draft.value.layers[0]?.id ?? ''
        history.value = []
        baseline.value = JSON.stringify(draft.value)
        saving.value = adding.value = picking.value = false
        editorReady.value = true
    } catch (err) {
        if (version === loadVersion) error.value = err instanceof Error ? err.message : String(err)
    } finally {
        if (version === loadVersion) busy.value = false
    }
}
async function save() {
    if (busy.value || !editorReady.value || !draft.value.name.trim()) return
    busy.value = true
    error.value = ''
    try {
        const result = await graphicsApi.save(draft.value, savedId.value)
        savedId.value = result.id
        draft.value = {
            name: result.name,
            group: result.group,
            duration: result.duration,
            during_lshape: result.during_lshape,
            layers: result.layers,
        }
        baseline.value = JSON.stringify(draft.value)
        saving.value = false
        await router.replace({ name: 'graphics-composer', query: { channel: channelId.value, template: result.id } })
        note.value = t('graphics.composer.saved', { name: result.name })
    } catch (err) {
        error.value =
            err instanceof AuthFetchError && err.status === 409
                ? t('graphics.composer.nameTaken')
                : err instanceof Error
                  ? err.message
                  : String(err)
    } finally {
        busy.value = false
    }
}
async function upload(event: Event) {
    const input = event.target as HTMLInputElement
    const file = input.files?.[0]
    const layer = selected.value
    if (!file || !layer || busy.value) return
    busy.value = true
    error.value = ''
    try {
        await authFetch(`/api/file/${channelId.value}/create-folder`, {
            method: 'POST',
            headers: { ...configStore.contentType, ...authStore.authHeader },
            body: JSON.stringify({ source: 'Graphics' }),
            responseType: 'text',
        })
        await sendFileInChunks(
            file,
            { url: `/api/file/${channelId.value}/upload?path=Graphics`, headers: authStore.authHeader },
            (bytes) => {
                note.value = t('graphics.composer.uploadProgress', { percent: Math.round((bytes / file.size) * 100) })
            },
        )
        remember()
        layer.image = 'Graphics/' + file.name
        note.value = t('graphics.composer.uploaded')
    } catch (err) {
        error.value = err instanceof Error ? err.message : String(err)
    } finally {
        busy.value = false
        input.value = ''
    }
}
function pickImage(item: { path: string }) {
    if (!/\.(png|jpe?g|webp|gif)$/i.test(item.path)) {
        error.value = t('graphics.composer.imageOnly')
        return
    }
    update({ image: item.path })
    picking.value = false
}
function beforeUnload(event: BeforeUnloadEvent) {
    if (composing.value && editorReady.value && dirty.value) {
        event.preventDefault()
        event.returnValue = ''
    }
}
onBeforeRouteLeave(
    () => !composing.value || !editorReady.value || !dirty.value || window.confirm(t('graphics.composer.discard')),
)
onBeforeRouteUpdate((to) => {
    if (!composing.value) return
    if (
        String(to.query.template ?? '') === String(route.query.template ?? '') &&
        to.query.channel === route.query.channel
    )
        return
    if (dirty.value && !busy.value && !window.confirm(t('graphics.composer.discard'))) return false
})
watch(
    () => [route.name, route.query.template, channelId.value],
    () => {
        if (
            composing.value &&
            busy.value &&
            savedId.value !== undefined &&
            Number(route.query.template) === savedId.value &&
            !dirty.value
        )
            return
        loadEditor()
    },
)
onMounted(() => {
    loadEditor()
    window.addEventListener('beforeunload', beforeUnload)
})
onBeforeUnmount(() => {
    ++loadVersion
    stopPreview()
    window.removeEventListener('beforeunload', beforeUnload)
})

const templates = ref<GraphicsTemplate[]>([])
const onAir = ref<GraphicOnAir[]>([])
const loaded = ref(false)
const group = ref<string | null>(null)

const groups = computed(() => [...new Set(templates.value.map((template) => template.group))].filter(Boolean))
const shown = computed(() =>
    group.value === null ? templates.value : templates.value.filter((template) => template.group === group.value),
)

/** Sample values so a card shows what the template looks like. */
function sample(template: GraphicsTemplate): Record<string, string> {
    return Object.fromEntries(template.fields.map((field) => [field, `[${field}]`]))
}

async function loadLibrary() {
    try {
        ;[templates.value, onAir.value] = await Promise.all([graphicsApi.templates(), graphicsApi.onAir()])
    } catch (error) {
        indexStore.msgAlert(
            'error',
            error instanceof AuthFetchError && typeof error.data === 'string'
                ? error.data
                : t('graphics.templates.loadFailed'),
            4,
        )
    } finally {
        loaded.value = true
    }
}
onMounted(loadLibrary)
watch(composing, (value) => {
    if (!value) loadLibrary()
})
</script>
<template>
    <template v-if="!composing">
        <div class="flex flex-wrap items-start justify-between gap-3">
            <div>
                <h1 class="m-0 text-xl font-semibold">{{ t('graphics.sections.templates') }}</h1>
                <p class="mt-1 mb-0 text-muted">{{ t('graphics.intro.templates') }}</p>
            </div>
            <RouterLink
                v-if="isAdmin"
                :to="{ name: 'graphics-composer', query: { channel: channelId } }"
                class="btn-main"
            >
                + {{ t('graphics.library.new') }}
            </RouterLink>
        </div>

        <div class="flex flex-wrap items-center gap-2" role="group" :aria-label="t('graphics.library.filter')">
            <button
                type="button"
                class="min-h-9 px-3 rounded-full border text-sm cursor-pointer"
                :class="group === null ? 'border-accent bg-base-300' : 'border-line-strong'"
                :aria-pressed="group === null"
                @click="group = null"
            >
                {{ t('graphics.library.all') }}
            </button>
            <button
                v-for="name in groups"
                :key="name"
                type="button"
                class="min-h-9 px-3 rounded-full border text-sm cursor-pointer"
                :class="group === name ? 'border-accent bg-base-300' : 'border-line-strong'"
                :aria-pressed="group === name"
                @click="group = name"
            >
                {{ name }}
            </button>
            <span class="ml-auto text-sm text-muted">{{ t('graphics.onAir.count', { count: shown.length }) }}</span>
        </div>

        <p v-if="loaded && !templates.length" class="m-0 text-muted">{{ t('graphics.templates.none') }}</p>

        <div class="grid grid-cols-[repeat(auto-fill,minmax(260px,1fr))] gap-3">
            <article v-for="template in shown" :key="template.id" class="card-box overflow-hidden flex flex-col">
                <TemplatePreview
                    :template="template"
                    :values="sample(template)"
                    class="bg-[linear-gradient(180deg,#5b7da6_0%,#3f5f86_55%,#3f6b2f_55%,#2f5524_100%)]"
                >
                    <span
                        v-if="onAir.some((graphic) => graphic.template_id === template.id && !graphic.leaving)"
                        class="absolute right-2 top-2 z-10 px-2 py-0.5 rounded-full bg-error text-error-content text-[11px] font-bold"
                    >
                        {{ t('graphics.onAir.badge') }}
                    </span>
                </TemplatePreview>
                <div class="px-4 py-3 flex flex-col gap-1.5 flex-auto">
                    <div class="flex items-baseline justify-between gap-2">
                        <h2 class="m-0 text-sm font-semibold">{{ template.name }}</h2>
                        <span class="text-xs text-muted">{{ template.group || t('graphics.templates.noGroup') }}</span>
                    </div>
                    <span class="text-xs text-muted">
                        {{ fieldList(template) || t('graphics.templates.noFields') }} ·
                        {{
                            template.duration
                                ? t('graphics.library.seconds', { seconds: template.duration })
                                : t('graphics.library.untilOut')
                        }}
                    </span>
                    <span class="text-xs text-muted">{{
                        t(`graphics.templates.lshape.${template.during_lshape}`)
                    }}</span>
                    <div class="flex gap-2 mt-auto pt-2">
                        <RouterLink
                            v-if="isAdmin"
                            :to="{ name: 'graphics-composer', query: { channel: channelId, template: template.id } }"
                            class="btn-quiet min-h-9"
                            >{{ t('graphics.composer.edit') }}</RouterLink
                        >
                        <RouterLink
                            :to="{ name: 'graphics-onair', query: { channel: channelId, template: template.id } }"
                            class="btn-main min-h-9"
                        >
                            {{ t('graphics.library.toAir') }}
                        </RouterLink>
                    </div>
                </div>
            </article>
        </div>
    </template>
    <section v-else class="flex flex-col gap-4" :aria-label="t('graphics.composer.title')">
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
            <button class="btn-quiet" :disabled="!draft.layers.length" @click="play">
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
        <form v-if="saving" class="card-box p-4 flex flex-wrap gap-3 items-end border-accent" @submit.prevent="save">
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
            <button class="btn-main" :disabled="busy || !draft.name.trim()">{{ t('graphics.composer.save') }}</button>
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
                        v-for="kind in ['image', 'text', 'box', 'video'] as const"
                        :key="kind"
                        class="btn-quiet text-left"
                        :disabled="kind === 'video' && draft.layers.some((layer) => layer.kind === 'video')"
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
                    <span>{{ t('graphics.onAir.preview') }}</span
                    ><span>{{ t('graphics.composer.safeArea') }}</span
                    ><span class="ml-auto font-mono">{{ time?.toFixed(2) ?? '0.00' }} s</span>
                </div>
                <div
                    ref="canvas"
                    class="relative touch-none select-none"
                    @pointermove="move"
                    @pointerup="endDrag"
                    @pointercancel="endDrag"
                    @lostpointercapture="endDrag"
                >
                    <TemplatePreview
                        :template="preview"
                        :values="sampleValues"
                        :time="time"
                        class="rounded-field border border-line-strong bg-[linear-gradient(180deg,#5b7da6_0%,#3f5f86_55%,#3f6b2f_55%,#2f5524_100%)]"
                    >
                        <template #overlay>
                            <div class="absolute inset-[5%] border border-dashed border-white/30 pointer-events-none" />
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
                                    @pointerdown.stop="grab($event, layer, 'move')"
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
                                            @pointerdown.stop="grab($event, layer, handle)"
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
                            ? `x ${shownDimension('x')} · y ${shownDimension('y')} · w ${shownDimension('width')} · h ${shownDimension('height')} ${unit}`
                            : t('graphics.composer.empty')
                    }}
                </p>
                <section class="card-box p-3 flex flex-col gap-2" :aria-label="t('graphics.composer.timeline')">
                    <h2 class="text-sm font-semibold m-0">{{ t('graphics.composer.timeline') }}</h2>
                    <label class="field-label"
                        >{{ t('graphics.composer.playhead')
                        }}<input type="range" min="0" :max="timelineEnd" step="0.01" :value="time ?? 0" @input="seek"
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
                            @change="update({ name: ($event.target as HTMLInputElement).value })"
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
                                :value="shownDimension(key)"
                                @change="dimension(key, ($event.target as HTMLInputElement).value)"
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
                            @change="scale(Number(($event.target as HTMLInputElement).value))"
                    /></label>
                    <div class="grid grid-cols-4 gap-1">
                        <button
                            v-for="factor in [50, 75, 100, 150]"
                            :key="factor"
                            class="btn-quiet min-h-10 px-1!"
                            @click="scale(factor)"
                        >
                            {{ factor }}%
                        </button>
                    </div>
                    <div class="flex flex-wrap gap-1">
                        <button class="btn-quiet min-h-10" @click="update({ x: 50 - selected.width / 2 })">
                            {{ t('graphics.composer.centerH') }}</button
                        ><button class="btn-quiet min-h-10" @click="update({ y: 50 - selected.height / 2 })">
                            {{ t('graphics.composer.centerV') }}</button
                        ><button
                            class="btn-quiet min-h-10"
                            @click="update(fitLayer(selected, { x: 0, width: 100 }, proportional))"
                        >
                            {{ t('graphics.composer.fullWidth') }}
                        </button>
                    </div>
                    <template v-if="selected.kind === 'text'">
                        <label class="field-label"
                            >{{ t('graphics.composer.text')
                            }}<textarea
                                class="field-input min-h-20"
                                :value="selected.text"
                                maxlength="1000"
                                @change="update({ text: ($event.target as HTMLTextAreaElement).value })"
                            />
                        </label>
                        <p class="m-0 text-xs text-muted">{{ t('graphics.composer.fieldsHint') }}</p>
                        <label class="field-label"
                            >{{ t('graphics.composer.font')
                            }}<select
                                class="field-input"
                                :value="selected.font_family ?? ''"
                                @change="update({ font_family: ($event.target as HTMLSelectElement).value || null })"
                            >
                                <option value="">{{ t('graphics.composer.defaultFont') }}</option>
                                <option v-for="font in fonts" :key="font" :value="font">{{ font }}</option>
                            </select></label
                        >
                        <label class="field-label"
                            >{{ t('graphics.composer.fontSize') }} (%)<input
                                class="field-input"
                                type="number"
                                step="0.1"
                                min="0.1"
                                max="50"
                                :value="selected.font_size"
                                @change="update({ font_size: Number(($event.target as HTMLInputElement).value) })"
                        /></label>
                        <label class="flex items-center gap-2 text-sm"
                            ><input
                                type="checkbox"
                                class="checkbox checkbox-sm"
                                :checked="selected.bold"
                                @change="update({ bold: ($event.target as HTMLInputElement).checked })"
                            />{{ t('graphics.composer.bold') }}</label
                        >
                        <label class="field-label"
                            >{{ t('graphics.composer.color')
                            }}<input
                                class="field-input"
                                :value="selected.color"
                                @change="update({ color: ($event.target as HTMLInputElement).value })"
                        /></label>
                        <label class="field-label"
                            >{{ t('graphics.composer.align')
                            }}<select
                                class="field-input"
                                :value="selected.align"
                                @change="
                                    update({
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
                    <template v-if="selected.kind === 'image'">
                        <label class="field-label"
                            >{{ t('graphics.composer.image')
                            }}<input
                                class="field-input"
                                :value="selected.image ?? ''"
                                @change="update({ image: ($event.target as HTMLInputElement).value })"
                        /></label>
                        <p class="m-0 text-xs text-muted">{{ t('graphics.composer.imageHint') }}</p>
                        <label class="field-label"
                            >{{ t('graphics.composer.upload')
                            }}<input
                                type="file"
                                accept="image/png,image/jpeg,image/webp,image/gif"
                                class="max-w-full text-xs"
                                @change="upload"
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
                    <template v-if="selected.kind !== 'video'">
                        <label class="field-label"
                            >{{ t('graphics.composer.fill')
                            }}<input
                                class="field-input"
                                :value="selected.fill ?? ''"
                                placeholder="#rrggbb"
                                @change="update({ fill: ($event.target as HTMLInputElement).value || null })"
                        /></label>
                        <label class="field-label"
                            >{{ t('graphics.composer.radius') }} (%)<input
                                class="field-input"
                                type="number"
                                min="0"
                                max="50"
                                step="0.1"
                                :value="selected.radius"
                                @change="update({ radius: Number(($event.target as HTMLInputElement).value) })"
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
                            @change="update({ opacity: Number(($event.target as HTMLInputElement).value) })"
                    /></label>
                    <div class="border-t border-line-strong pt-3 flex flex-col gap-3">
                        <h3 class="m-0 text-sm font-semibold">{{ t('graphics.composer.animation') }}</h3>
                        <label v-for="key in ['animation_in', 'animation_out'] as const" :key="key" class="field-label"
                            >{{ t('graphics.composer.' + key)
                            }}<select
                                class="field-input"
                                :value="selected[key]"
                                @change="update({ [key]: ($event.target as HTMLSelectElement).value })"
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
                                @change="update({ [key]: Number(($event.target as HTMLInputElement).value) })"
                        /></label>
                    </div>
                </template>
                <label class="field-label border-t border-line-strong pt-3"
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
                <label class="field-label"
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
