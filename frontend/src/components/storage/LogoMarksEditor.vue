<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { cloneDeep } from 'es-toolkit/object'
import { isEqual } from 'es-toolkit/predicate'

import StoragePicker from '@/components/schedule/StoragePicker.vue'
import PositionPad, { type Shares } from '@/components/settings/PositionPad.vue'
import SpanBar from '@/components/storage/SpanBar.vue'
import GenericModal from '@/components/utils/GenericModal.vue'
import TrimBar from '@/components/storage/TrimBar.vue'
import { useDialogFocus } from '@/composables/dialogFocus'
import { createFilePreviewUrl } from '@/composables/fileAccess'
import { LOGO_BOX, LSHAPE_CORNERS, sharesOf } from '@/composables/graphics'
import { type Size, logoBox, logoBoxStyle, storageImageUrl } from '@/composables/logo'
import { type LogoSpan, type VideoMarks, baseName, readLogoMarks, saveLogoMarks } from '@/composables/storage'
import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'

/**
 * Side panel that marks, while the video plays, the part of it that airs
 * (its trim), where it keeps the logo off, where its ad breaks are
 * (signalled with SCTE-35), such as a commercial break edited into a
 * programme, and where it goes into the L-shape.
 */
const props = defineProps<{
    channelId: number
    /** Storage path of the video. */
    path: string
    /** Its length, known before the browser loads it (or when it cannot). */
    length?: number
    /** The tab it opens on. */
    initialKind?: Tab
    /**
     * One entry of a playlist: its own marks (none yet: a copy of the
     * video's to start from) and the part it plays. Saving hands them back
     * and leaves the video's marks in the storage as they are.
     */
    entry?: { marks: VideoMarks | null; from: number; to: number }
}>()

const emit = defineEmits<{ close: []; saved: [marks: VideoMarks] }>()

const { t } = useI18n()
const configStore = useConfig()
const indexStore = useIndex()

const MAX_SPANS = 16

type MarkKind = 'hide' | 'breaks' | 'lshape'
type Tab = MarkKind | 'trim'

const MARK_KINDS: MarkKind[] = ['hide', 'breaks', 'lshape']

/** How each kind of mark looks: bar colour, labels, chip and icons in and out of a stretch. */
const LOOK: Record<
    MarkKind,
    {
        tone: 'logo' | 'break' | 'lshape'
        scope: string
        chip: string
        bar: string
        inside: string
        outside: string
        off: string
        on: string
    }
> = {
    hide: {
        tone: 'logo',
        scope: 'storage.logoMarks',
        chip: 'bg-after-clip/15 text-after-clip',
        bar: 'bg-after-clip/60',
        inside: 'bi-eye-slash',
        outside: 'bi-eye',
        off: 'bi-eye-slash',
        on: 'bi-eye',
    },
    breaks: {
        tone: 'break',
        scope: 'storage.breakMarks',
        chip: 'bg-live/15 text-live',
        bar: 'bg-live/60',
        inside: 'bi-badge-ad',
        outside: 'bi-tv',
        off: 'bi-box-arrow-right',
        on: 'bi-box-arrow-in-left',
    },
    lshape: {
        tone: 'lshape',
        scope: 'storage.lshapeMarks',
        chip: 'bg-info/15 text-info',
        bar: 'bg-info/60',
        inside: 'bi-aspect-ratio',
        outside: 'bi-fullscreen',
        off: 'bi-aspect-ratio',
        on: 'bi-fullscreen',
    },
}

function emptyMarks(): VideoMarks {
    return {
        hide: [],
        breaks: [],
        trim: null,
        lshape: [],
        lshape_graphic: null,
        lshape_corner: null,
        lshape_scale: null,
        logo_position: null,
    }
}

const marks = ref<VideoMarks>(emptyMarks())
const baseline = ref<VideoMarks>(emptyMarks())
/** Which marks the buttons and the list edit. */
const kind = ref<Tab>(props.initialKind ?? 'hide')
/** The kind of stretch being edited (the logo's while on the trim tab). */
const markKind = computed<MarkKind>(() => (kind.value === 'trim' ? 'hide' : kind.value))
const look = computed(() => LOOK[markKind.value])
/** The list being edited; changes go straight into `marks`. */
const spans = computed(() => marks.value[markKind.value])
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

const dirty = computed(() => !isEqual(normalized(marks.value), normalized(baseline.value)))
const sorted = computed(() => [...spans.value].sort((a, b) => a.from - b.from))

function sortedOf(list: LogoSpan[]): LogoSpan[] {
    return [...list].sort((a, b) => a.from - b.from)
}

/** Where the part that airs starts and ends: the saved trim, else the whole video. */
const trimFrom = computed({
    get: () => marks.value.trim?.from ?? 0,
    set: (from: number) => (marks.value.trim = { from, to: trimTo.value }),
})
const trimTo = computed({
    get: () => marks.value.trim?.to ?? duration.value,
    set: (to: number) => (marks.value.trim = { from: trimFrom.value, to }),
})
const trimInvalid = computed(() => {
    const trim = marks.value.trim

    return (
        !!trim &&
        (!(trim.to > trim.from) ||
            trim.from < 0 ||
            (duration.value > 0 && (trim.from >= duration.value || trim.to > duration.value + 0.05)))
    )
})
/** A trim that keeps the whole video is no trim. */
function normalized(value: VideoMarks): VideoMarks {
    const trim = value.trim
    const whole = trim && trim.from <= 0 && duration.value > 0 && trim.to >= duration.value - 0.05

    return {
        hide: sortedOf(value.hide),
        breaks: sortedOf(value.breaks),
        trim: whole ? null : (trim ?? null),
        lshape: sortedOf(value.lshape),
        lshape_graphic: value.lshape_graphic?.trim() || null,
        lshape_corner: value.lshape_corner || null,
        lshape_scale: value.lshape_scale ?? null,
        logo_position: value.logo_position || null,
    }
}

/** Where a preview started with the buttons stops, while it plays. */
const stopAt = ref<number | null>(null)

/** Play from `from` and stop at `to`. */
function playBetween(from: number, to: number) {
    seek(Math.max(0, from))
    stopAt.value = duration.value > 0 ? Math.min(to, duration.value) : to
    video.value?.play().catch(() => (stopAt.value = null))
}

/** Playing the part that airs: stop at its end. */
function playTrim() {
    playBetween(trimFrom.value, trimTo.value)
}

/** Hear the cut at the out point: start a few seconds before it. */
function playOut() {
    playBetween(Math.max(trimFrom.value, trimTo.value - 3), trimTo.value)
}

/** Seconds of video shown around a stretch, so how it goes in and out is seen. */
const AROUND = 3

/** Watch a stretch as it airs: all of it, or just how it goes in or comes out. */
function playSpan(span: LogoSpan, part: 'whole' | 'in' | 'out') {
    if (part === 'in') {
        playBetween(span.from - AROUND, Math.min(span.to, span.from + AROUND))
    } else if (part === 'out') {
        playBetween(Math.max(span.from, span.to - AROUND), span.to + AROUND)
    } else {
        playBetween(span.from - AROUND, span.to + AROUND)
    }
}

/** The stretch the watch buttons play: the one picked, else the first. */
const watchedSpan = computed(() => spans.value[selectedSpan.value ?? -1] ?? sorted.value[0] ?? null)

/** The play button of a row: pick that stretch and watch it. */
function watchRow(index: number) {
    const span = spans.value[index]

    if (span) {
        selectedSpan.value = index
        playSpan(span, 'whole')
    }
}

/** Its number in the list, for the buttons. */
const watchedNumber = computed(() => (watchedSpan.value ? spans.value.indexOf(watchedSpan.value) + 1 : 0))

function timeUpdate(event: Event) {
    position.value = (event.target as HTMLVideoElement).currentTime

    if (stopAt.value !== null && position.value >= stopAt.value) {
        video.value?.pause()
        stopAt.value = null
    }
}

function trimText(edge: 'from' | 'to'): string {
    return drafts.value[`trim-${edge}`] ?? formatTime(edge === 'from' ? trimFrom.value : trimTo.value)
}

function setTrimText(edge: 'from' | 'to', text: string) {
    drafts.value[`trim-${edge}`] = text

    const seconds = parseTime(text)

    if (seconds !== null) {
        if (edge === 'from') {
            trimFrom.value = seconds
        } else {
            trimTo.value = seconds
        }
    }
}

/** The stretch picked on the bar or in the list. */
const selectedSpan = ref<number | null>(null)

/** A handle of the bar moved a stretch. */
function changeSpan(index: number, span: LogoSpan) {
    spans.value[index] = span
    delete drafts.value[`${index}-from`]
    delete drafts.value[`${index}-to`]
}

/** One lane per kind of mark on the timeline: the one being edited is the big bar. */
const LANES: Tab[] = ['trim', 'hide', 'breaks', 'lshape']
const LANE_BAR: Record<Tab, string> = {
    trim: 'bg-ok/50',
    hide: LOOK.hide.bar,
    breaks: LOOK.breaks.bar,
    lshape: LOOK.lshape.bar,
}

function tabLabel(option: Tab): string {
    return {
        trim: t('storage.trim.kind'),
        hide: t('storage.logoMarks.kindLogo'),
        breaks: t('storage.logoMarks.kindBreaks'),
        lshape: t('storage.logoMarks.kindLShape'),
    }[option]
}

const TAB_ICON: Record<Tab, string> = {
    trim: 'bi-scissors',
    hide: 'bi-eye-slash',
    breaks: 'bi-badge-ad',
    lshape: 'bi-aspect-ratio',
}

/** The stretches a lane shows: the trim's is the part that airs. */
function laneSpans(option: Tab): LogoSpan[] {
    if (option === 'trim') {
        const trim = normalized(marks.value).trim

        return duration.value > 0 ? [trim ?? { from: 0, to: duration.value }] : []
    }

    return marks.value[option]
}

/** The play position is in a part the trim cuts off. */
const trimCutNow = computed(() => position.value < trimFrom.value || position.value > trimTo.value)

function selectKind(option: Tab) {
    selectedSpan.value = null
    kind.value = option
    pendingFrom.value = null
    drafts.value = {}
}

/** Put the in or the out point at the play position. */
function trimHere(edge: 'from' | 'to') {
    const seconds = Math.round(position.value * 10) / 10

    if (edge === 'from') {
        trimFrom.value = seconds
    } else {
        trimTo.value = seconds
    }

    drafts.value = {}
}

function settleTrim(edge: 'from' | 'to') {
    delete drafts.value[`trim-${edge}`]
}

function clearTrim() {
    marks.value.trim = null
    drafts.value = {}
}

function inside(list: LogoSpan[]): boolean {
    return list.some((span) => position.value >= span.from && position.value <= span.to)
}

/** The logo is off at the play position. */
const hiddenNow = computed(() => inside(marks.value.hide))
/** The play position is inside an L-shape stretch. */
const lshapeNow = computed(() => inside(marks.value.lshape))
/** Inside a mark of the kind being edited. */
const insideNow = computed(() => inside(marks.value[markKind.value]))

/** Labels of the kind being edited. */
function label(key: string, values?: Record<string, unknown>): string {
    const scope = look.value.scope

    return values ? t(`${scope}.${key}`, values) : t(`${scope}.${key}`)
}

/** The L-shape graphic and where the picture goes, as the channel puts it on air. */
const lshapeUrl = ref('')
/** In the L-shape at the play position, with a graphic to show it: the logo is off then. */
const inLShape = computed(() => lshapeNow.value && !!lshapeUrl.value)
/** Just out of the L-shape: the logo waits for the picture to grow back. */
const leftLShape = ref(false)
let leftTimer: ReturnType<typeof setTimeout> | undefined

watch(inLShape, (inside, before) => {
    clearTimeout(leftTimer)
    leftLShape.value = before && !inside
    leftTimer = leftLShape.value ? setTimeout(() => (leftLShape.value = false), 1300) : undefined
})

/** As on air: the logo goes off first (0.4 s), then the picture shrinks (0.8 s); out, the other way. */
const lshapeWindow = computed(() => {
    const share = lshapeScale.value / 100
    const corner = lshapeCorner.value

    return {
        transform: inLShape.value ? `scale(${share})` : 'none',
        transformOrigin: `${corner.startsWith('top') ? 'top' : 'bottom'} ${corner.endsWith('left') ? 'left' : 'right'}`,
        transition: `transform 0.8s ease-in-out ${inLShape.value ? '0.4s' : '0s'}`,
    }
})

/** The L-shape of this video: its own corner and size, else the channel's. */
const channelCorner = computed(() => configStore.playout?.processing?.lshape_corner || 'top-right')
const channelScale = computed(() => configStore.playout?.processing?.lshape_scale ?? 75)
const lshapeCorner = computed(() => marks.value.lshape_corner || channelCorner.value)
const lshapeScale = computed(() => marks.value.lshape_scale ?? channelScale.value)
const LSHAPE_SIZES = [30, 35, 40, 45, 50, 55, 60, 65, 70, 75, 80, 85, 90, 95]

/** Where the logo goes in this video: its own place, else the channel's. */
const channelLogoPosition = computed(() => configStore.playout?.processing?.logo_position || 'W-w-12:12')
const logoPosition = computed(() => marks.value.logo_position || channelLogoPosition.value)
const logoShares = computed(() => sharesOf(logoPosition.value))

/** The logo's box on the pad, from its real size when known. */
const logoPadBox = computed(() => {
    const processing = configStore.playout?.processing
    const box = logoImage.value && processing && logoBox(processing.logo_scale, '0:0', logoImage.value, frameSize.value)

    return box ? { width: box.width, height: box.height } : LOGO_BOX
})

function placeLogo(shares: Shares) {
    marks.value.logo_position = `${shares.x}%:${shares.y}%`
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
const invalid = computed(
    () =>
        trimInvalid.value ||
        MARK_KINDS.some((which) => {
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
    selectedSpan.value = null
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
        const loaded = props.entry?.marks
            ? cloneDeep(props.entry.marks)
            : await readLogoMarks(props.channelId, props.path)

        // An entry's cut is the part it plays.
        if (props.entry) {
            loaded.trim = { from: props.entry.from, to: props.entry.to }
        }

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
    await loadLShape()
}

/** The video's own L-shape graphic, else the channel's. */
async function loadLShape() {
    const channel = configStore.channels.find((entry) => entry.id === props.channelId)
    const graphic = marks.value.lshape_graphic?.trim() || configStore.playout?.processing?.lshape

    lshapeUrl.value = await storageImageUrl(channel, graphic).catch(() => '')
}

watch(() => marks.value.lshape_graphic, loadLShape)

const pickingGraphic = ref(false)

function pickGraphic(file: { path: string }) {
    marks.value.lshape_graphic = file.path
    pickingGraphic.value = false
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
        logoBox(processing.logo_scale, logoPosition.value, logoImage.value, frameSize.value)

    return box ? logoBoxStyle(box) : { top: '3%', right: '2%', height: '6%' }
})

const logoOpacity = computed(() =>
    hiddenNow.value || inLShape.value ? 0 : (configStore.playout?.processing?.logo_opacity ?? 1),
)

/** The channel's animation as the logo goes off or comes back (a fade lasts 1 s unless set). */
const logoTransition = computed(() => {
    const processing = configStore.playout?.processing

    // The L-shape's own order: off before the picture shrinks, back after it grew back.
    if (inLShape.value) {
        return 'opacity 0.4s linear'
    }

    if (leftLShape.value) {
        return 'opacity 0.4s linear 0.8s'
    }

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
        if (props.entry) {
            emit('saved', normalized(marks.value))
            emit('close')
            return
        }

        const saved = await saveLogoMarks(props.channelId, props.path, normalized(marks.value))

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
            class="outline-none w-full max-w-[1200px] h-full overflow-y-auto bg-base-100 border-l border-line-strong shadow-2xl flex flex-col"
        >
            <header
                class="sticky top-0 z-10 bg-base-100/95 backdrop-blur border-b border-line px-5 pt-4 pb-3 flex flex-col gap-3"
            >
                <div class="flex items-start gap-3">
                    <div class="flex-1 min-w-0">
                        <h2 id="logo-marks-title" class="m-0 text-lg font-semibold">
                            {{ t('storage.logoMarks.title') }}
                        </h2>
                        <p class="m-0 text-[13px] text-muted truncate font-mono" :title="path">{{ baseName(path) }}</p>
                        <p
                            v-if="entry"
                            class="m-0 mt-1.5 inline-flex items-center gap-1.5 rounded-full bg-accent/15 text-accent-text px-2.5 py-0.5 text-xs font-medium"
                        >
                            <i class="bi bi-pin-angle" aria-hidden="true" />{{ t('storage.marks.entryOnly') }}
                        </p>
                    </div>
                    <button type="button" class="btn-icon" :aria-label="t('storage.close')" @click="requestClose">
                        <i class="bi bi-x-lg" aria-hidden="true" />
                    </button>
                </div>

                <!-- One tab per kind of mark, with what this video has of each. -->
                <div
                    role="radiogroup"
                    :aria-label="t('storage.logoMarks.kind')"
                    class="grid grid-cols-2 sm:grid-cols-4 gap-1 bg-base-200 rounded-field p-1"
                >
                    <button
                        v-for="option in LANES"
                        :key="option"
                        type="button"
                        role="radio"
                        :aria-checked="kind === option"
                        class="min-h-11 px-3 rounded-md text-sm cursor-pointer inline-flex items-center justify-center gap-2"
                        :class="
                            kind === option
                                ? 'bg-base-100 font-semibold shadow-sm text-base-content'
                                : 'text-muted hover:text-base-content'
                        "
                        @click="selectKind(option)"
                    >
                        <i class="bi" :class="TAB_ICON[option]" aria-hidden="true" />{{ tabLabel(option) }}
                        <span
                            v-if="option !== 'trim'"
                            class="text-xs font-mono rounded-full px-1.5 min-w-5 text-center"
                            :class="marks[option].length ? LOOK[option].chip : 'bg-base-300 text-muted'"
                            >{{ marks[option].length }}</span
                        >
                        <i
                            v-else-if="normalized(marks).trim"
                            class="bi bi-check2-circle text-accent-text"
                            :aria-label="t('storage.trim.set')"
                        />
                    </button>
                </div>
            </header>

            <div v-if="loading" class="py-10 flex justify-center">
                <span class="loading loading-spinner" />
            </div>

            <div
                v-else
                class="px-5 py-4 grid grid-cols-1 lg:grid-cols-[minmax(0,1.4fr)_minmax(320px,1fr)] gap-5 items-start"
            >
                <!-- Left: the video as it airs, and the timeline of every mark. -->
                <div class="flex flex-col gap-3 lg:sticky lg:top-36">
                    <div
                        class="relative w-full bg-black rounded-field overflow-hidden"
                        :style="{ aspectRatio: `${frameSize.width} / ${frameSize.height}` }"
                    >
                        <!-- The L-shape graphic, under the picture while it is in the L-shape. -->
                        <img
                            v-if="lshapeUrl && marks.lshape.length"
                            :src="lshapeUrl"
                            alt=""
                            class="absolute inset-0 w-full h-full object-fill pointer-events-none"
                            aria-hidden="true"
                            @error="lshapeUrl = ''"
                        />
                        <video
                            v-if="videoUrl && !videoFailed"
                            ref="video"
                            :src="videoUrl"
                            controls
                            preload="auto"
                            playsinline
                            class="relative w-full h-full object-contain bg-black"
                            :style="lshapeWindow"
                            @timeupdate="timeUpdate"
                            @seeked="position = ($event.target as HTMLVideoElement).currentTime"
                            @pause="stopAt = null"
                            @loadedmetadata="duration = ($event.target as HTMLVideoElement).duration || duration"
                            @error="videoFailed = true"
                        />
                        <p
                            v-else
                            class="absolute inset-0 m-0 flex items-center justify-center p-6 text-center text-muted"
                        >
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

                    <!-- Where the play position is, and what airs there. -->
                    <div class="grid grid-cols-[1fr_auto_1fr] items-center gap-2 text-xs">
                        <span class="flex flex-col leading-tight">
                            <span class="text-muted">{{ t('storage.marks.position') }}</span>
                            <span class="font-mono tabular-nums text-sm">{{ formatTime(position) }}</span>
                        </span>
                        <span
                            v-if="kind === 'trim'"
                            class="inline-flex items-center gap-1.5 rounded-full px-2.5 py-1 font-medium whitespace-nowrap"
                            :class="trimCutNow ? 'bg-base-content/10 text-muted' : 'bg-ok/15 text-ok'"
                            role="status"
                        >
                            <i class="bi" :class="trimCutNow ? 'bi-scissors' : 'bi-broadcast'" aria-hidden="true" />{{
                                trimCutNow ? t('storage.trim.cutHere') : t('storage.trim.airsHere')
                            }}
                        </span>
                        <span
                            v-else
                            class="inline-flex items-center gap-1.5 rounded-full px-2.5 py-1 font-medium whitespace-nowrap"
                            :class="insideNow ? look.chip : 'bg-ok/15 text-ok'"
                            role="status"
                        >
                            <i class="bi" :class="insideNow ? look.inside : look.outside" aria-hidden="true" />{{
                                insideNow ? label('offHere') : label('onHere')
                            }}
                        </span>
                        <span class="flex flex-col leading-tight text-right">
                            <span class="text-muted">{{ t('storage.marks.length') }}</span>
                            <span class="font-mono tabular-nums text-sm">{{
                                duration ? formatTime(duration) : '—'
                            }}</span>
                        </span>
                    </div>

                    <!-- The bar being edited, with handles. -->
                    <TrimBar
                        v-if="kind === 'trim'"
                        v-model:from="trimFrom"
                        v-model:to="trimTo"
                        :duration="duration"
                        :position="position"
                        @seek="seek"
                    />
                    <SpanBar
                        v-else
                        :duration="duration"
                        :spans="spans"
                        :tone="look.tone"
                        :position="position"
                        :pending="pendingFrom"
                        :trim="normalized(marks).trim"
                        :selected="selectedSpan"
                        @change="changeSpan"
                        @select="selectedSpan = $event"
                        @seek="seek"
                    />

                    <!-- Every kind of mark on one timeline; a lane opens its tab. -->
                    <div
                        class="flex flex-col gap-1.5 rounded-field border border-line p-2.5"
                        role="group"
                        :aria-label="t('storage.marks.timeline')"
                    >
                        <button
                            v-for="option in LANES"
                            :key="option"
                            type="button"
                            class="grid grid-cols-[128px_minmax(0,1fr)] items-center gap-2 bg-transparent border-0 p-0 cursor-pointer text-left group"
                            :aria-label="t('storage.marks.lane', { name: tabLabel(option) })"
                            :aria-pressed="kind === option"
                            @click="selectKind(option)"
                        >
                            <span
                                class="text-xs truncate inline-flex items-center gap-1.5"
                                :class="
                                    kind === option
                                        ? 'text-base-content font-semibold'
                                        : 'text-muted group-hover:text-base-content'
                                "
                            >
                                <i class="bi" :class="TAB_ICON[option]" aria-hidden="true" />{{ tabLabel(option) }}
                            </span>
                            <span
                                class="relative h-2.5 rounded-full bg-field overflow-hidden"
                                :class="kind === option ? 'ring-1 ring-accent' : ''"
                            >
                                <span
                                    v-for="(span, index) in laneSpans(option)"
                                    :key="index"
                                    class="absolute inset-y-0 rounded-full"
                                    :class="LANE_BAR[option]"
                                    :style="{
                                        left: percent(span.from),
                                        width: `calc(${percent(span.to)} - ${percent(span.from)})`,
                                    }"
                                />
                                <span
                                    v-if="duration > 0"
                                    class="absolute inset-y-0 w-0.5 bg-base-content"
                                    :style="{ left: percent(position) }"
                                    aria-hidden="true"
                                />
                            </span>
                        </button>
                    </div>
                </div>

                <!-- Right: what the tab edits. -->
                <div class="flex flex-col gap-4 min-w-0">
                    <p class="m-0 text-[13px] text-muted leading-relaxed">
                        {{ kind === 'trim' ? t('storage.trim.intro') : label('intro') }}
                    </p>

                    <!-- Trim. -->
                    <template v-if="kind === 'trim'">
                        <div class="flex flex-wrap items-center gap-2">
                            <button
                                type="button"
                                class="btn-quiet"
                                :disabled="position >= trimTo"
                                @click="trimHere('from')"
                            >
                                <i class="bi bi-chevron-bar-left" aria-hidden="true" />{{ t('storage.trim.inHere') }}
                            </button>
                            <button
                                type="button"
                                class="btn-quiet"
                                :disabled="position <= trimFrom"
                                @click="trimHere('to')"
                            >
                                <i class="bi bi-chevron-bar-right" aria-hidden="true" />{{ t('storage.trim.outHere') }}
                            </button>
                            <button
                                type="button"
                                class="btn-quiet"
                                :disabled="!videoUrl || videoFailed"
                                @click="playTrim"
                            >
                                <i class="bi bi-play-fill" aria-hidden="true" />{{ t('storage.trim.play') }}
                            </button>
                            <button
                                type="button"
                                class="btn-quiet"
                                :disabled="!videoUrl || videoFailed"
                                @click="playOut"
                            >
                                <i class="bi bi-skip-end-fill" aria-hidden="true" />{{ t('storage.trim.playOut') }}
                            </button>
                        </div>

                        <div class="grid grid-cols-2 gap-3 rounded-field border border-line p-3">
                            <label class="field-label">
                                {{ t('storage.trim.in') }}
                                <span class="flex gap-1.5">
                                    <input
                                        :value="trimText('from')"
                                        class="field-input font-mono text-[13px] min-h-9"
                                        @input="setTrimText('from', ($event.target as HTMLInputElement).value)"
                                        @blur="settleTrim('from')"
                                    />
                                    <button
                                        type="button"
                                        class="btn-icon min-w-9 min-h-9 flex-none"
                                        :aria-label="t('storage.logoMarks.goTo', { time: formatTime(trimFrom) })"
                                        @click="seek(trimFrom)"
                                    >
                                        <i class="bi bi-skip-start" aria-hidden="true" />
                                    </button>
                                </span>
                            </label>
                            <label class="field-label">
                                {{ t('storage.trim.out') }}
                                <span class="flex gap-1.5">
                                    <input
                                        :value="trimText('to')"
                                        class="field-input font-mono text-[13px] min-h-9"
                                        @input="setTrimText('to', ($event.target as HTMLInputElement).value)"
                                        @blur="settleTrim('to')"
                                    />
                                    <button
                                        type="button"
                                        class="btn-icon min-w-9 min-h-9 flex-none"
                                        :aria-label="t('storage.logoMarks.goTo', { time: formatTime(trimTo) })"
                                        @click="seek(trimTo)"
                                    >
                                        <i class="bi bi-skip-end" aria-hidden="true" />
                                    </button>
                                </span>
                            </label>
                        </div>

                        <p v-if="trimInvalid" class="m-0 text-[13px] text-error" role="alert">
                            {{ t('storage.trim.invalid') }}
                        </p>
                        <div
                            v-else-if="duration > 0"
                            class="flex flex-col gap-1 rounded-field bg-base-200 px-3 py-2.5 text-[13px]"
                            role="status"
                        >
                            <span>
                                <i class="bi bi-broadcast me-1 text-ok" aria-hidden="true" />{{
                                    t('storage.trim.summaryAirs', {
                                        length: formatTime(trimTo - trimFrom),
                                        total: formatTime(duration),
                                    })
                                }}
                            </span>
                            <span v-if="trimFrom > 0 || trimTo < duration - 0.05" class="text-muted">
                                <i class="bi bi-scissors me-1" aria-hidden="true" />{{
                                    t('storage.trim.summaryCut', {
                                        start: formatTime(trimFrom),
                                        end: formatTime(Math.max(0, duration - trimTo)),
                                    })
                                }}
                            </span>
                            <span v-else class="text-muted">{{ t('storage.trim.summaryWhole') }}</span>
                            <button
                                v-if="marks.trim"
                                type="button"
                                class="self-start bg-transparent border-0 p-0 mt-1 text-error cursor-pointer hover:underline text-[13px]"
                                @click="clearTrim"
                            >
                                <i class="bi bi-arrow-counterclockwise me-1" aria-hidden="true" />{{
                                    t('storage.trim.clear')
                                }}
                            </button>
                        </div>
                        <p class="m-0 field-hint">{{ t('storage.trim.hint') }}</p>
                    </template>

                    <!-- Logo, breaks and L-shape stretches. -->
                    <template v-else>
                        <div class="flex flex-wrap items-center gap-2">
                            <button
                                type="button"
                                class="btn-quiet"
                                :disabled="spans.length >= MAX_SPANS || insideNow"
                                @click="markOff"
                            >
                                <i class="bi" :class="look.off" aria-hidden="true" />{{ label('markOff') }}
                            </button>
                            <button
                                type="button"
                                class="btn-quiet"
                                :disabled="pendingFrom === null || position <= pendingFrom"
                                @click="markOn"
                            >
                                <i class="bi" :class="look.on" aria-hidden="true" />{{ label('markOn') }}
                            </button>
                        </div>
                        <p
                            v-if="pendingFrom !== null"
                            class="m-0 text-[13px] text-after-clip rounded-field bg-after-clip/10 px-3 py-2"
                            role="status"
                        >
                            {{ label('pending', { time: formatTime(pendingFrom) }) }}
                            <button
                                type="button"
                                class="ms-1 bg-transparent border-0 p-0 text-muted cursor-pointer hover:underline"
                                @click="pendingFrom = null"
                            >
                                {{ t('cancel') }}
                            </button>
                        </p>

                        <!-- Watch a stretch as it airs, like the trim. -->
                        <div
                            v-if="watchedSpan"
                            class="flex flex-wrap items-center gap-2 rounded-field bg-base-200 px-3 py-2"
                        >
                            <button
                                type="button"
                                class="btn-quiet"
                                :disabled="!videoUrl || videoFailed"
                                @click="playSpan(watchedSpan, 'whole')"
                            >
                                <i class="bi bi-play-fill" aria-hidden="true" />{{
                                    t('storage.marks.watch', { n: watchedNumber })
                                }}
                            </button>
                            <button
                                type="button"
                                class="btn-quiet"
                                :disabled="!videoUrl || videoFailed"
                                @click="playSpan(watchedSpan, 'in')"
                            >
                                <i class="bi bi-box-arrow-in-right" aria-hidden="true" />{{
                                    t('storage.marks.watchIn')
                                }}
                            </button>
                            <button
                                type="button"
                                class="btn-quiet"
                                :disabled="!videoUrl || videoFailed"
                                @click="playSpan(watchedSpan, 'out')"
                            >
                                <i class="bi bi-box-arrow-right" aria-hidden="true" />{{ t('storage.marks.watchOut') }}
                            </button>
                            <span class="basis-full text-xs text-muted">{{
                                t('storage.marks.watchHint', { s: AROUND })
                            }}</span>
                        </div>

                        <section class="flex flex-col gap-2">
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
                                    class="rounded-field border px-2.5 py-2 flex flex-col gap-2"
                                    :class="
                                        selectedSpan === index ? 'border-accent bg-accent/5' : 'border-line bg-base-200'
                                    "
                                    @focusin="selectedSpan = index"
                                >
                                    <div class="flex items-center gap-2">
                                        <span
                                            class="text-xs font-mono rounded-full w-6 h-6 inline-flex items-center justify-center"
                                            :class="look.chip"
                                            >{{ index + 1 }}</span
                                        >
                                        <button
                                            type="button"
                                            class="btn-icon min-w-8 min-h-8"
                                            :disabled="!videoUrl || videoFailed"
                                            :aria-label="t('storage.marks.watch', { n: index + 1 })"
                                            :title="t('storage.marks.watch', { n: index + 1 })"
                                            @click="watchRow(index)"
                                        >
                                            <i class="bi bi-play-fill" aria-hidden="true" />
                                        </button>
                                        <span class="flex-1 text-xs text-muted font-mono">
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
                                    </div>
                                    <div class="grid grid-cols-2 gap-2">
                                        <label class="flex flex-col gap-1 text-xs text-muted">
                                            {{ label('from') }}
                                            <span class="flex gap-1">
                                                <input
                                                    :value="fieldText(index, 'from')"
                                                    class="field-input font-mono text-[13px] min-h-8 min-w-0"
                                                    @input="
                                                        setField(
                                                            index,
                                                            'from',
                                                            ($event.target as HTMLInputElement).value,
                                                        )
                                                    "
                                                    @blur="settleField(index, 'from')"
                                                />
                                                <button
                                                    type="button"
                                                    class="btn-icon min-w-8 min-h-8 flex-none"
                                                    :aria-label="
                                                        t('storage.logoMarks.goTo', { time: formatTime(span.from) })
                                                    "
                                                    @click="seek(span.from)"
                                                >
                                                    <i class="bi bi-skip-start" aria-hidden="true" />
                                                </button>
                                            </span>
                                        </label>
                                        <label class="flex flex-col gap-1 text-xs text-muted">
                                            {{ label('to') }}
                                            <span class="flex gap-1">
                                                <input
                                                    :value="fieldText(index, 'to')"
                                                    class="field-input font-mono text-[13px] min-h-8 min-w-0"
                                                    @input="
                                                        setField(index, 'to', ($event.target as HTMLInputElement).value)
                                                    "
                                                    @blur="settleField(index, 'to')"
                                                />
                                                <button
                                                    type="button"
                                                    class="btn-icon min-w-8 min-h-8 flex-none"
                                                    :aria-label="
                                                        t('storage.logoMarks.goTo', { time: formatTime(span.to) })
                                                    "
                                                    @click="seek(span.to)"
                                                >
                                                    <i class="bi bi-skip-end" aria-hidden="true" />
                                                </button>
                                            </span>
                                        </label>
                                    </div>
                                </li>
                            </ul>
                            <p v-if="invalid && !trimInvalid" class="m-0 text-[13px] text-error" role="alert">
                                {{ overlaps ? label('overlap') : label('reversed') }}
                            </p>
                        </section>

                        <!-- Logo: where it goes in this video. -->
                        <section
                            v-if="kind === 'hide'"
                            class="flex flex-col gap-3 rounded-field border border-line p-3"
                            :aria-label="t('storage.marks.logoPlace.title')"
                        >
                            <div class="flex flex-wrap items-center justify-between gap-2">
                                <h3 class="m-0 text-sm font-semibold">{{ t('storage.marks.logoPlace.title') }}</h3>
                                <div role="radiogroup" class="inline-flex bg-base-200 rounded-md p-0.5 text-xs">
                                    <button
                                        v-for="own in [false, true]"
                                        :key="String(own)"
                                        type="button"
                                        role="radio"
                                        :aria-checked="!!marks.logo_position === own"
                                        class="px-2.5 min-h-8 rounded cursor-pointer"
                                        :class="
                                            !!marks.logo_position === own
                                                ? 'bg-base-100 font-semibold shadow-sm'
                                                : 'text-muted'
                                        "
                                        @click="own ? placeLogo(logoShares) : (marks.logo_position = null)"
                                    >
                                        {{
                                            own
                                                ? t('storage.marks.logoPlace.own')
                                                : t('storage.marks.logoPlace.channel')
                                        }}
                                    </button>
                                </div>
                            </div>
                            <PositionPad
                                class="max-w-72"
                                :shares="logoShares"
                                :width="logoPadBox.width"
                                :height="logoPadBox.height"
                                :label="t('settings.logo.remote.logoPad')"
                                @place="placeLogo"
                            />
                            <p class="m-0 field-hint">{{ t('storage.marks.logoPlace.hint') }}</p>
                        </section>

                        <!-- L-shape: graphic, corner and size in this video. -->
                        <section
                            v-if="kind === 'lshape'"
                            class="flex flex-col gap-3 rounded-field border border-line p-3"
                            :aria-label="t('storage.marks.lshapeLook.title')"
                        >
                            <h3 class="m-0 text-sm font-semibold">{{ t('storage.marks.lshapeLook.title') }}</h3>
                            <div class="field-label">
                                {{ t('storage.lshapeMarks.graphic') }}
                                <div class="flex flex-wrap gap-2">
                                    <input
                                        :value="marks.lshape_graphic ?? ''"
                                        class="field-input font-mono text-[13px] flex-[1_1_200px] min-w-0"
                                        :placeholder="t('storage.lshapeMarks.graphicChannel')"
                                        :aria-label="t('storage.lshapeMarks.graphic')"
                                        spellcheck="false"
                                        @input="
                                            marks.lshape_graphic = ($event.target as HTMLInputElement).value || null
                                        "
                                    />
                                    <button type="button" class="btn-quiet" @click="pickingGraphic = true">
                                        {{ t('settings.choose') }}
                                    </button>
                                    <button
                                        v-if="marks.lshape_graphic"
                                        type="button"
                                        class="btn-quiet"
                                        @click="marks.lshape_graphic = null"
                                    >
                                        {{ t('storage.lshapeMarks.graphicReset') }}
                                    </button>
                                </div>
                            </div>
                            <div class="flex flex-wrap items-start gap-4">
                                <div class="flex flex-col gap-1.5">
                                    <span class="text-xs text-muted">{{ t('storage.marks.lshapeLook.corner') }}</span>
                                    <div
                                        role="radiogroup"
                                        :aria-label="t('storage.marks.lshapeLook.corner')"
                                        class="grid grid-cols-2 gap-1.5 aspect-video w-36 rounded-md border border-line-strong bg-field p-1.5"
                                    >
                                        <button
                                            v-for="corner in LSHAPE_CORNERS"
                                            :key="corner"
                                            type="button"
                                            role="radio"
                                            class="rounded-sm border cursor-pointer"
                                            :class="
                                                lshapeCorner === corner
                                                    ? marks.lshape_corner
                                                        ? 'bg-accent border-accent'
                                                        : 'bg-accent/40 border-accent'
                                                    : 'bg-base-100 border-line-strong hover:border-accent'
                                            "
                                            :aria-checked="lshapeCorner === corner"
                                            :aria-label="t(`lshape.corners.${corner}`)"
                                            :title="t(`lshape.corners.${corner}`)"
                                            @click="marks.lshape_corner = corner === channelCorner ? null : corner"
                                        />
                                    </div>
                                    <span class="text-xs text-muted">{{
                                        marks.lshape_corner
                                            ? t(`lshape.corners.${marks.lshape_corner}`)
                                            : `${t('storage.marks.lshapeLook.channel')} · ${t(`lshape.corners.${channelCorner}`)}`
                                    }}</span>
                                </div>
                                <label class="field-label flex-[1_1_160px]">
                                    {{ t('storage.marks.lshapeLook.size') }}
                                    <select
                                        class="field-input"
                                        :value="marks.lshape_scale ?? ''"
                                        @change="
                                            marks.lshape_scale = ($event.target as HTMLSelectElement).value
                                                ? Number(($event.target as HTMLSelectElement).value)
                                                : null
                                        "
                                    >
                                        <option value="">
                                            {{ t('storage.marks.lshapeLook.channel') }} ({{ channelScale }}%)
                                        </option>
                                        <option v-for="size in LSHAPE_SIZES" :key="size" :value="size">
                                            {{ t('storage.marks.lshapeLook.sizeValue', { n: size }) }}
                                        </option>
                                    </select>
                                </label>
                            </div>
                            <p class="m-0 field-hint">{{ t('storage.marks.lshapeLook.hint') }}</p>
                        </section>

                        <p class="m-0 field-hint">{{ label('hint', { preroll: prerollLabel }) }}</p>
                    </template>
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
                    <span class="flex-1 text-[13px] text-muted">
                        <template v-if="dirty"
                            ><span class="dot bg-accent me-1.5" aria-hidden="true" />{{
                                t('storage.logoMarks.unsavedShort')
                            }}</template
                        >
                    </span>
                    <button type="button" class="btn-quiet" @click="requestClose">{{ t('cancel') }}</button>
                    <button type="button" class="btn-main" :disabled="saving || !dirty || invalid" @click="save">
                        <span v-if="saving" class="loading loading-spinner loading-sm" />
                        {{ entry ? t('storage.marks.entryApply') : t('storage.logoMarks.saveAll') }}
                    </button>
                </template>
            </footer>
        </section>

        <GenericModal
            :show="pickingGraphic"
            :title="t('storage.lshapeMarks.graphic')"
            hide-buttons
            :modal-action="() => (pickingGraphic = false)"
        >
            <div class="w-[min(520px,90vw)] max-h-[60vh] flex flex-col">
                <StoragePicker @pick="pickGraphic" />
            </div>
        </GenericModal>
    </div>
</template>
