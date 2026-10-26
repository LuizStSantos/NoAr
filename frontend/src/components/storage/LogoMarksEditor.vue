<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { cloneDeep } from 'es-toolkit/object'
import { isEqual } from 'es-toolkit/predicate'

import { useDialogFocus } from '@/composables/dialogFocus'
import { createFilePreviewUrl } from '@/composables/fileAccess'
import { type Size, logoBox, logoBoxStyle, storageImageUrl } from '@/composables/logo'
import { type LogoSpan, type VideoMarks, baseName, readLogoMarks, saveLogoMarks } from '@/composables/storage'
import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'

/**
 * Side panel that marks, while the video plays, where it keeps the logo off
 * and where its ad breaks are (signalled with SCTE-35), such as a commercial
 * break edited into a programme.
 */
const props = defineProps<{
    channelId: number
    /** Storage path of the video. */
    path: string
    /** Its length, known before the browser loads it (or when it cannot). */
    length?: number
}>()

const emit = defineEmits<{ close: []; saved: [marks: VideoMarks] }>()

const { t } = useI18n()
const configStore = useConfig()
const indexStore = useIndex()

const MAX_SPANS = 16

type MarkKind = 'hide' | 'breaks'

const marks = ref<VideoMarks>({ hide: [], breaks: [] })
const baseline = ref<VideoMarks>({ hide: [], breaks: [] })
/** Which marks the buttons and the list edit. */
const kind = ref<MarkKind>('hide')
/** The list being edited; changes go straight into `marks`. */
const spans = computed(() => marks.value[kind.value])
const loading = ref(true)
const saving = ref(false)
const videoUrl = ref('')
const logoUrl = ref('')
/** The logo image's own size, once it loaded. */
const logoImage = ref<Size | null>(null)
const videoFailed = ref(false)
const video = ref<HTMLVideoElement>()
const position = ref(0)
const duration = ref(props.length ?? 0)
/** Where the logo was marked to go off; the stretch closes where it comes back. */
const pendingFrom = ref<number | null>(null)
/** The text in each time field while it is being typed. */
const drafts = ref<Record<string, string>>({})
const confirmClose = ref(false)

const dirty = computed(() => !isEqual(marks.value, baseline.value))
const sorted = computed(() => [...spans.value].sort((a, b) => a.from - b.from))

function sortedOf(list: LogoSpan[]): LogoSpan[] {
    return [...list].sort((a, b) => a.from - b.from)
}

function inside(list: LogoSpan[]): boolean {
    return list.some((span) => position.value >= span.from && position.value <= span.to)
}

/** The logo is off at the play position. */
const hiddenNow = computed(() => inside(marks.value.hide))
/** The play position is inside a marked break. */
const breakNow = computed(() => inside(marks.value.breaks))
/** Inside a mark of the kind being edited. */
const insideNow = computed(() => (kind.value === 'hide' ? hiddenNow.value : breakNow.value))

/** Labels of the kind being edited. */
function label(key: string, values?: Record<string, unknown>): string {
    const scope = kind.value === 'hide' ? 'storage.logoMarks' : 'storage.breakMarks'

    return values ? t(`${scope}.${key}`, values) : t(`${scope}.${key}`)
}

/** How far ahead a break is announced, from the ad break settings. */
const prerollLabel = computed(() => `${configStore.playout?.scte35?.preroll ?? 6} s`)

/** Breaks often sit exactly where the logo goes off: start from those. */
function copyFromLogo() {
    marks.value.breaks = cloneDeep(marks.value.hide)
    drafts.value = {}
}
const overlaps = computed(() =>
    sorted.value.some((span, index) => index > 0 && span.from < sorted.value[index - 1]!.to),
)
const invalid = computed(() =>
    (['hide', 'breaks'] as const).some((which) => {
        const list = sortedOf(marks.value[which])

        return (
            list.some((span) => !(span.to > span.from) || span.from < 0) ||
            list.some((span, index) => index > 0 && span.from < list[index - 1]!.to)
        )
    }),
)

/** `1:02:03.4` or `02:03.4`, tenths kept. */
function formatTime(seconds: number): string {
    const total = Math.max(0, Math.round(seconds * 10)) / 10
    const h = Math.floor(total / 3600)
    const m = Math.floor((total % 3600) / 60)
    const s = (total % 60).toFixed(1).padStart(4, '0')

    return h ? `${h}:${String(m).padStart(2, '0')}:${s}` : `${String(m).padStart(2, '0')}:${s}`
}

/** Seconds from `h:mm:ss.s`, `mm:ss.s` or plain seconds; null when not a time. */
function parseTime(text: string): number | null {
    const parts = text.trim().replace(',', '.').split(':')

    if (!parts.length || parts.length > 3 || parts.some((part) => !/^\d+(\.\d+)?$/.test(part))) {
        return null
    }

    return parts.reduce((total, part) => total * 60 + Number(part), 0)
}

function fieldText(index: number, edge: 'from' | 'to'): string {
    return drafts.value[`${index}-${edge}`] ?? formatTime(spans.value[index]![edge])
}

function setField(index: number, edge: 'from' | 'to', text: string) {
    drafts.value[`${index}-${edge}`] = text

    const seconds = parseTime(text)

    if (seconds !== null) {
        spans.value[index]![edge] = seconds
    }
}

function settleField(index: number, edge: 'from' | 'to') {
    delete drafts.value[`${index}-${edge}`]
}

function seek(seconds: number) {
    if (video.value) {
        video.value.currentTime = seconds
    }

    position.value = seconds
}

function seekBar(event: MouseEvent) {
    const bar = event.currentTarget as HTMLElement
    const share = (event.clientX - bar.getBoundingClientRect().left) / bar.clientWidth

    if (duration.value) {
        seek(Math.min(duration.value, Math.max(0, share * duration.value)))
    }
}

function markOff() {
    pendingFrom.value = position.value
}

function markOn() {
    if (pendingFrom.value === null || position.value <= pendingFrom.value) {
        return
    }

    spans.value.push({ from: pendingFrom.value, to: position.value })
    spans.value.sort((a, b) => a.from - b.from)
    pendingFrom.value = null
}

function remove(index: number) {
    spans.value.splice(index, 1)
    drafts.value = {}
}

function percent(seconds: number): string {
    return `${duration.value ? Math.min(100, (seconds / duration.value) * 100) : 0}%`
}

async function previewUrl(path: string): Promise<string> {
    return createFilePreviewUrl(props.channelId, `/${path.replace(/^\/+/, '')}`)
}

async function load() {
    loading.value = true

    try {
        const loaded = await readLogoMarks(props.channelId, props.path)

        marks.value = cloneDeep(loaded)
        baseline.value = cloneDeep(loaded)
        videoUrl.value = await previewUrl(props.path)
    } catch (error) {
        indexStore.msgAlert('error', error instanceof Error ? error.message : String(error), 4)
    } finally {
        loading.value = false
    }

    const channel = configStore.channels.find((entry) => entry.id === props.channelId)

    logoUrl.value = await storageImageUrl(channel, configStore.playout?.processing?.logo).catch(() => '')
}

/** The output picture, which the preview takes the shape of. */
const frameSize = computed<Size>(() => ({
    width: Math.max(1, configStore.playout?.output?.width || 1920),
    height: Math.max(1, configStore.playout?.output?.height || 1080),
}))

/** The logo where and as large as the channel puts it on air. */
const logoStyle = computed(() => {
    const processing = configStore.playout?.processing
    const box =
        logoImage.value &&
        processing &&
        logoBox(processing.logo_scale, processing.logo_position, logoImage.value, frameSize.value)

    return box ? logoBoxStyle(box) : { top: '3%', right: '2%', height: '6%' }
})

const logoOpacity = computed(() => (hiddenNow.value ? 0 : (configStore.playout?.processing?.logo_opacity ?? 1)))

/** The channel's animation as the logo goes off or comes back (a fade lasts 1 s unless set). */
const logoTransition = computed(() => {
    const processing = configStore.playout?.processing
    const kind = hiddenNow.value ? processing?.logo_transition_out : processing?.logo_transition_in

    return kind === 'cut' ? 'none' : `opacity ${processing?.logo_transition_seconds ?? 1}s linear`
})

function logoLoaded(event: Event) {
    const image = event.target as HTMLImageElement

    logoImage.value = image.naturalWidth ? { width: image.naturalWidth, height: image.naturalHeight } : null
}

async function save() {
    saving.value = true

    try {
        const saved = await saveLogoMarks(props.channelId, props.path, {
            hide: sortedOf(marks.value.hide),
            breaks: sortedOf(marks.value.breaks),
        })

        baseline.value = cloneDeep(saved)
        marks.value = cloneDeep(saved)
        indexStore.msgAlert('success', t('storage.logoMarks.saved'), 2)
        emit('saved', saved)
        emit('close')
    } catch (error) {
        indexStore.msgAlert('error', error instanceof Error ? error.message : String(error), 4)
    } finally {
        saving.value = false
    }
}

function requestClose() {
    if (dirty.value && !confirmClose.value) {
        confirmClose.value = true
    } else {
        emit('close')
    }
}

const dialog = ref<HTMLElement>()

useDialogFocus(dialog, { onEscape: requestClose })

onMounted(() => {
    document.body.style.overflow = 'hidden'
    load()
})

onBeforeUnmount(() => {
    document.body.style.overflow = ''
})
</script>
<template>
    <div class="z-50 fixed inset-0 flex justify-end bg-black/50" @click.self="requestClose">
        <section
            ref="dialog"
            role="dialog"
            aria-modal="true"
            aria-labelledby="logo-marks-title"
            tabindex="-1"
            class="outline-none w-full max-w-[800px] h-full overflow-y-auto bg-base-100 border-l border-line-strong shadow-2xl flex flex-col"
        >
            <header class="flex items-start gap-3 px-5 pt-5 pb-3">
                <div class="flex-1 min-w-0">
                    <h2 id="logo-marks-title" class="m-0 text-lg font-semibold">{{ t('storage.logoMarks.title') }}</h2>
                    <p class="m-0 text-[13px] text-muted truncate" :title="path">{{ baseName(path) }}</p>
                </div>
                <button type="button" class="btn-icon" :aria-label="t('storage.close')" @click="requestClose">
                    <i class="bi bi-x-lg" aria-hidden="true" />
                </button>
            </header>

            <div v-if="loading" class="py-10 flex justify-center">
                <span class="loading loading-spinner" />
            </div>

            <div v-else class="px-5 pb-5 flex flex-col gap-4">
                <div role="radiogroup" :aria-label="t('storage.logoMarks.kind')" class="flex gap-1 bg-base-200 rounded-md p-1 self-start">
                    <button
                        v-for="option in ['hide', 'breaks'] as const"
                        :key="option"
                        type="button"
                        role="radio"
                        :aria-checked="kind === option"
                        class="px-3 py-1 rounded text-[13px] cursor-pointer"
                        :class="kind === option ? 'bg-base-100 font-semibold shadow-sm' : 'text-muted'"
                        @click="
                            kind = option;
                            pendingFrom = null;
                            drafts = {}
                        "
                    >
                        <i
                            class="bi me-1"
                            :class="option === 'hide' ? 'bi-eye-slash' : 'bi-badge-ad'"
                            aria-hidden="true"
                        />{{ option === 'hide' ? t('storage.logoMarks.kindLogo') : t('storage.logoMarks.kindBreaks') }}
                        <span class="text-muted font-normal">({{ marks[option].length }})</span>
                    </button>
                </div>

                <p class="m-0 text-[13px] text-muted">{{ label('intro') }}</p>

                <div
                    class="relative w-full bg-black rounded-field overflow-hidden"
                    :style="{ aspectRatio: `${frameSize.width} / ${frameSize.height}` }"
                >
                    <video
                        v-if="videoUrl && !videoFailed"
                        ref="video"
                        :src="videoUrl"
                        controls
                        preload="auto"
                        playsinline
                        class="w-full h-full object-contain"
                        @timeupdate="position = ($event.target as HTMLVideoElement).currentTime"
                        @seeked="position = ($event.target as HTMLVideoElement).currentTime"
                        @loadedmetadata="duration = ($event.target as HTMLVideoElement).duration || duration"
                        @error="videoFailed = true"
                    />
                    <p v-else class="absolute inset-0 m-0 flex items-center justify-center p-6 text-center text-muted">
                        {{ t('storage.logoMarks.noVideo') }}
                    </p>
                    <img
                        v-if="logoUrl"
                        :src="logoUrl"
                        alt=""
                        class="absolute max-w-none pointer-events-none"
                        :style="{ ...logoStyle, opacity: logoOpacity, transition: logoTransition }"
                        aria-hidden="true"
                        @load="logoLoaded"
                        @error="logoUrl = ''"
                    />
                    <span
                        v-else
                        class="absolute flex items-center text-[11px] font-semibold text-white border border-white/70 rounded px-1.5 pointer-events-none"
                        :style="{ ...logoStyle, opacity: logoOpacity, transition: logoTransition }"
                        aria-hidden="true"
                        >LOGO</span
                    >
                </div>

                <div class="flex flex-col gap-1.5">
                    <div
                        class="relative h-7 rounded-md bg-field border border-line cursor-pointer overflow-hidden"
                        role="presentation"
                        @click="seekBar"
                    >
                        <span
                            v-for="(span, index) in sortedOf(marks.hide)"
                            :key="`hide-${index}`"
                            class="absolute top-0 h-1/2 bg-after-clip/40 border-x border-after-clip"
                            :class="kind === 'hide' ? '' : 'opacity-40'"
                            :style="{
                                left: percent(span.from),
                                width: `calc(${percent(span.to)} - ${percent(span.from)})`,
                            }"
                        />
                        <span
                            v-for="(span, index) in sortedOf(marks.breaks)"
                            :key="`break-${index}`"
                            class="absolute bottom-0 h-1/2 bg-live/40 border-x border-live"
                            :class="kind === 'breaks' ? '' : 'opacity-40'"
                            :style="{
                                left: percent(span.from),
                                width: `calc(${percent(span.to)} - ${percent(span.from)})`,
                            }"
                        />
                        <span
                            v-if="pendingFrom !== null && position > pendingFrom"
                            class="absolute inset-y-0"
                            :class="kind === 'hide' ? 'bg-after-clip/20' : 'bg-live/20'"
                            :style="{
                                left: percent(pendingFrom),
                                width: `calc(${percent(position)} - ${percent(pendingFrom)})`,
                            }"
                        />
                        <span class="absolute inset-y-0 w-0.5 bg-accent" :style="{ left: percent(position) }" />
                    </div>
                    <div class="flex justify-between font-mono text-xs text-muted">
                        <span>{{ formatTime(position) }}</span>
                        <span :class="insideNow ? (kind === 'hide' ? 'text-after-clip' : 'text-live') : 'text-ok'">
                            {{ insideNow ? label('offHere') : label('onHere') }}
                        </span>
                        <span>{{ duration ? formatTime(duration) : '' }}</span>
                    </div>
                </div>

                <div class="flex flex-wrap items-center gap-2">
                    <button
                        type="button"
                        class="btn-quiet"
                        :disabled="spans.length >= MAX_SPANS || insideNow"
                        @click="markOff"
                    >
                        <i class="bi" :class="kind === 'hide' ? 'bi-eye-slash' : 'bi-box-arrow-right'" aria-hidden="true" />{{
                            label('markOff')
                        }}
                    </button>
                    <button
                        type="button"
                        class="btn-quiet"
                        :disabled="pendingFrom === null || position <= pendingFrom"
                        @click="markOn"
                    >
                        <i class="bi" :class="kind === 'hide' ? 'bi-eye' : 'bi-box-arrow-in-left'" aria-hidden="true" />{{
                            label('markOn')
                        }}
                    </button>
                    <span v-if="pendingFrom !== null" class="text-[13px] text-after-clip" role="status">
                        {{ label('pending', { time: formatTime(pendingFrom) }) }}
                        <button
                            type="button"
                            class="ms-1 bg-transparent border-0 p-0 text-muted cursor-pointer hover:underline"
                            @click="pendingFrom = null"
                        >
                            {{ t('cancel') }}
                        </button>
                    </span>
                </div>

                <div class="flex flex-col gap-2">
                    <h3 class="m-0 text-sm font-semibold">{{ label('list') }}</h3>
                    <p v-if="!spans.length" class="m-0 text-[13px] text-muted">
                        {{ label('none') }}
                        <button
                            v-if="kind === 'breaks' && marks.hide.length"
                            type="button"
                            class="ms-1 bg-transparent border-0 p-0 text-accent cursor-pointer hover:underline"
                            @click="copyFromLogo"
                        >
                            {{ t('storage.breakMarks.fromLogo', marks.hide.length) }}
                        </button>
                    </p>
                    <ul v-else class="m-0 p-0 list-none flex flex-col gap-1.5">
                        <li
                            v-for="(span, index) in spans"
                            :key="index"
                            class="flex flex-wrap items-center gap-2 bg-base-200 rounded-md px-3 py-2"
                        >
                            <span class="text-xs text-muted w-4">{{ index + 1 }}</span>
                            <label class="flex items-center gap-1.5 text-xs text-muted">
                                {{ label('from') }}
                                <input
                                    :value="fieldText(index, 'from')"
                                    class="field-input font-mono text-[13px] w-28 min-h-8"
                                    @input="setField(index, 'from', ($event.target as HTMLInputElement).value)"
                                    @blur="settleField(index, 'from')"
                                />
                            </label>
                            <button
                                type="button"
                                class="btn-icon min-w-8 min-h-8"
                                :aria-label="t('storage.logoMarks.goTo', { time: formatTime(span.from) })"
                                @click="seek(span.from)"
                            >
                                <i class="bi bi-skip-start" aria-hidden="true" />
                            </button>
                            <label class="flex items-center gap-1.5 text-xs text-muted">
                                {{ label('to') }}
                                <input
                                    :value="fieldText(index, 'to')"
                                    class="field-input font-mono text-[13px] w-28 min-h-8"
                                    @input="setField(index, 'to', ($event.target as HTMLInputElement).value)"
                                    @blur="settleField(index, 'to')"
                                />
                            </label>
                            <button
                                type="button"
                                class="btn-icon min-w-8 min-h-8"
                                :aria-label="t('storage.logoMarks.goTo', { time: formatTime(span.to) })"
                                @click="seek(span.to)"
                            >
                                <i class="bi bi-skip-end" aria-hidden="true" />
                            </button>
                            <span class="flex-1 text-xs text-muted text-right font-mono">
                                {{ span.to > span.from ? formatTime(span.to - span.from) : '' }}
                            </span>
                            <button
                                type="button"
                                class="btn-icon min-w-8 min-h-8 text-error"
                                :aria-label="t('storage.logoMarks.remove')"
                                @click="remove(index)"
                            >
                                <i class="bi bi-trash" aria-hidden="true" />
                            </button>
                        </li>
                    </ul>
                    <p v-if="invalid" class="m-0 text-[13px] text-error" role="alert">
                        {{ overlaps ? label('overlap') : label('reversed') }}
                    </p>
                    <p class="m-0 field-hint">{{ label('hint', { preroll: prerollLabel }) }}</p>
                </div>
            </div>

            <footer class="mt-auto sticky bottom-0 bg-base-100 border-t border-line px-5 py-3 flex items-center gap-2">
                <template v-if="confirmClose">
                    <span class="flex-1 text-[13px]">{{ t('storage.logoMarks.unsaved') }}</span>
                    <button type="button" class="btn-quiet" @click="confirmClose = false">
                        {{ t('storage.logoMarks.keepEditing') }}
                    </button>
                    <button type="button" class="btn-quiet text-error" @click="emit('close')">
                        {{ t('storage.logoMarks.discard') }}
                    </button>
                </template>
                <template v-else>
                    <span class="flex-1" />
                    <button type="button" class="btn-quiet" @click="requestClose">{{ t('cancel') }}</button>
                    <button type="button" class="btn-main" :disabled="saving || !dirty || invalid" @click="save">
                        <span v-if="saving" class="loading loading-spinner loading-sm" />
                        {{ t('storage.logoMarks.saveAll') }}
                    </button>
                </template>
            </footer>
        </section>
    </div>
</template>
