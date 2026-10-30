<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import { LIVES_FOLDER, emptyFields, imageUrl, saveSheet, storageJoin, uploadArtwork } from '@/composables/storage'
import { useIndex } from '@/stores/index'
import { useSchedule } from '@/stores/schedule'

/**
 * The fields of a live: name, signal and what the guide shows (poster, wide
 * image, title, description, genre and rating). Storage and the Scheduler
 * both use it, so a live is described the same way wherever it is made.
 *
 * The page that holds it asks `check()` before saving and `submit()` to save.
 */
const props = defineProps<{
    channelId: number
    /** The live to change; a new one is created in `Lives/` when missing. */
    live: LiveSource | null
    /** The live folder's sheet, to start from when changing it. */
    sheet: MediaSheet | null
}>()

const { t } = useI18n()
const indexStore = useIndex()
const scheduleStore = useSchedule()

type Protocol = 'SRT' | 'RTMP' | 'HLS' | 'UDP'

/** The two guide images: the poster (upright) and the wide image (fanart). */
type GuideImage = 'poster' | 'backdrop'

const PLACEHOLDERS: Record<Protocol, string> = {
    SRT: 'srt://203.0.113.10:9000?mode=caller&latency=200',
    RTMP: 'rtmp://203.0.113.10/live/stream-key',
    HLS: 'https://cdn.example.org/live/index.m3u8',
    UDP: 'udp://239.10.10.10:5000',
}
const RATINGS = ['Livre', '10', '12', '14', '16', '18']
const GUIDE_IMAGES: GuideImage[] = ['poster', 'backdrop']

const fields = props.sheet?.fields

const name = ref(props.live?.name ?? '')
const protocol = ref<Protocol>(protocolOf(props.live?.protocol))
const url = ref(props.live?.url ?? '')
const guideTitle = ref(fields?.title ?? '')
const plot = ref(fields?.plot ?? '')
const genre = ref(fields?.genres.join(', ') ?? '')
const rating = ref(fields?.rating || 'Livre')
const chosen = reactive<Record<GuideImage, File | null>>({ poster: null, backdrop: null })
const previews = reactive<Record<GuideImage, string>>({ poster: '', backdrop: '' })
const saved = reactive<Record<GuideImage, string>>({ poster: '', backdrop: '' })
const signal = ref<SignalTest | null>(null)
const testing = ref(false)
const testError = ref('')

const editing = computed(() => Boolean(props.live))
const safeName = computed(() => name.value.trim().replace(/[\\/:*?"<>|]/g, ''))
const ratingOptions = computed(() => (RATINGS.includes(rating.value) ? RATINGS : [...RATINGS, rating.value]))

function protocolOf(value?: string): Protocol {
    return value === 'RTMP' || value === 'HLS' || value === 'UDP' ? value : 'SRT'
}

// The tabs follow an address typed or pasted in full.
watch(url, (value) => {
    const scheme = value.trim().split('://')[0]?.toLowerCase()

    if (scheme === 'srt') {
        protocol.value = 'SRT'
    } else if (scheme === 'rtmp' || scheme === 'rtmps') {
        protocol.value = 'RTMP'
    } else if (scheme === 'http' || scheme === 'https') {
        protocol.value = 'HLS'
    } else if (scheme === 'udp') {
        protocol.value = 'UDP'
    }
})

function folderOf(path: string): string {
    return path.split('/').slice(0, -1).join('/')
}

onMounted(async () => {
    for (const kind of GUIDE_IMAGES) {
        const path = props.sheet?.[kind]

        if (path) {
            try {
                saved[kind] = await imageUrl(props.channelId, path)
            } catch {
                // The drop area stays empty.
            }
        }
    }
})

function choose(kind: GuideImage, file: File | null) {
    if (previews[kind]) {
        URL.revokeObjectURL(previews[kind])
    }

    chosen[kind] = file
    previews[kind] = file ? URL.createObjectURL(file) : ''
}

function pickImage(kind: GuideImage, event: Event) {
    const file = (event.target as HTMLInputElement).files?.[0]

    if (file?.type.startsWith('image/')) {
        choose(kind, file)
    }
}

function dropImage(kind: GuideImage, event: DragEvent) {
    const file = event.dataTransfer?.files?.[0]

    if (file?.type.startsWith('image/')) {
        choose(kind, file)
    }
}

onBeforeUnmount(() => GUIDE_IMAGES.forEach((kind) => choose(kind, null)))

async function testSignal() {
    const address = url.value.trim()

    testError.value = ''

    if (!address) {
        testError.value = t('schedule.needAddress')
        return
    }

    testing.value = true
    signal.value = null

    try {
        signal.value = await scheduleStore.testSignal(address)
    } catch (err) {
        testError.value = err instanceof Error ? err.message : String(err)
    } finally {
        testing.value = false
    }
}

const signalText = computed(() => {
    const test = signal.value

    if (!test) {
        return ''
    }

    if (!test.ok) {
        return t('schedule.signalFailed', { error: test.error ?? '' })
    }

    return [
        t('schedule.signalOk'),
        test.width && test.height ? `${test.width}×${test.height}` : '',
        test.fps ? `${test.fps} fps` : '',
        [test.video_codec, test.audio_codec].filter(Boolean).join(' / ').toUpperCase(),
    ]
        .filter(Boolean)
        .join(' · ')
})

function genres(): string[] {
    return genre.value
        .split(',')
        .map((item) => item.trim())
        .filter(Boolean)
}

/** What is missing before the live can be saved, or nothing. */
function check(): string {
    return safeName.value && url.value.trim() ? '' : t('schedule.needNameAndAddress')
}

/** Save the live, its guide data and the images picked; returns the live. */
async function submit(): Promise<LiveSource> {
    let live: LiveSource

    if (props.live) {
        live =
            url.value.trim() === props.live.url
                ? props.live
                : await scheduleStore.updateLive(props.live.path, url.value.trim())

        // Through the sheet, so fields this form does not show are kept.
        await saveSheet(props.channelId, folderOf(live.path), {
            kind: 'program',
            fields: {
                ...(fields ?? emptyFields()),
                title: guideTitle.value.trim() || props.live.name,
                plot: plot.value.trim(),
                genres: genres(),
                rating: rating.value,
            },
        })
    } else {
        live = await scheduleStore.saveLive(safeName.value, url.value.trim(), {
            title: guideTitle.value.trim(),
            plot: plot.value.trim(),
            genre: genres().join(', '),
            rating: rating.value,
        })
    }

    for (const kind of GUIDE_IMAGES) {
        const file = chosen[kind]

        if (!file) {
            continue
        }

        try {
            await uploadArtwork(props.channelId, folderOf(live.path), kind, file)
        } catch (err) {
            indexStore.msgAlert('warning', t('schedule.posterFailed', { error: String(err) }), 4)
        }
    }

    return live
}

defineExpose({ check, submit })
</script>

<template>
    <div v-if="editing" class="flex flex-col gap-1">
        <span class="text-xs text-muted">{{ t('schedule.name') }}</span>
        <span class="font-semibold">{{ live?.name }}</span>
        <span class="text-xs text-subtle">{{ t('storage.live.renameHint') }}</span>
    </div>
    <label v-else class="field-label">
        {{ t('schedule.name') }}
        <input v-model="name" class="field-input" :placeholder="t('schedule.liveNamePlaceholder')" />
    </label>

    <div class="flex flex-col gap-2">
        <div class="text-xs text-muted">{{ t('schedule.signal') }}</div>
        <div class="flex w-fit" role="group" :aria-label="t('schedule.protocol')">
            <button
                v-for="option in ['SRT', 'RTMP', 'HLS', 'UDP'] as Protocol[]"
                :key="option"
                type="button"
                class="border min-h-9 px-4 cursor-pointer first:rounded-s-field last:rounded-e-field -ms-px first:ms-0"
                :class="
                    protocol === option
                        ? 'bg-live border-live text-[#1c1c1c] font-semibold'
                        : 'bg-transparent border-line-strong text-base-content'
                "
                :aria-pressed="protocol === option"
                @click="protocol = option"
            >
                {{ option }}
            </button>
        </div>
        <div class="flex gap-2">
            <input
                v-model="url"
                class="field-input flex-1 min-w-0 font-mono"
                :aria-label="t('schedule.address')"
                :placeholder="PLACEHOLDERS[protocol]"
            />
            <button type="button" class="btn-quiet" :disabled="testing" @click="testSignal">
                <span v-if="testing" class="loading loading-spinner loading-xs" />
                {{ t('schedule.testSignal') }}
            </button>
        </div>
        <div
            v-if="signalText || testError"
            class="text-sm flex items-center gap-2"
            :class="signal?.ok ? 'text-ok' : 'text-error'"
            role="status"
        >
            <i class="bi" :class="signal?.ok ? 'bi-check-lg' : 'bi-exclamation-triangle'" aria-hidden="true" />
            {{ testError || signalText }}
        </div>
    </div>

    <slot name="between" />

    <div class="border-t border-base-300 pt-4 flex flex-col gap-3">
        <slot name="guide-header">
            <span class="font-semibold">{{ t('schedule.inGuide') }}</span>
        </slot>
        <div class="flex gap-4">
            <div
                v-for="kind in GUIDE_IMAGES"
                :key="kind"
                class="flex flex-col gap-1.5"
                :class="kind === 'poster' ? 'w-[132px] flex-none' : 'flex-1 min-w-0'"
            >
                <span class="text-xs text-muted">{{ t(`storage.live.image.${kind}`) }}</span>
                <label
                    class="h-44 border border-dashed border-base-content/40 rounded-md bg-base-200 flex flex-col items-center justify-center gap-1 text-xs text-subtle text-center p-2 cursor-pointer overflow-hidden"
                    @dragover.prevent
                    @drop.prevent="dropImage(kind, $event)"
                >
                    <img
                        v-if="previews[kind] || saved[kind]"
                        :src="previews[kind] || saved[kind]"
                        alt=""
                        class="w-full h-full rounded"
                        :class="kind === 'poster' ? 'object-cover' : 'object-contain'"
                    />
                    <template v-else>
                        <i class="bi bi-image text-2xl" aria-hidden="true" />
                        {{ t(`storage.live.image.${kind}Drop`) }}
                    </template>
                    <input
                        type="file"
                        accept="image/jpeg,image/png,image/webp"
                        class="sr-only"
                        :aria-label="t(`storage.live.image.${kind}`)"
                        @change="pickImage(kind, $event)"
                    />
                </label>
            </div>
        </div>
        <div class="flex flex-col gap-2.5">
            <label class="field-label">
                {{ t('schedule.guideTitle') }}
                <input v-model="guideTitle" class="field-input" :placeholder="name" />
            </label>
            <label class="field-label">
                {{ t('schedule.description') }}
                <textarea v-model="plot" rows="2" class="field-input py-2 resize-none" />
            </label>
            <div class="grid grid-cols-2 gap-2.5">
                <label class="field-label">
                    {{ t('schedule.genre') }}
                    <input v-model="genre" class="field-input" />
                </label>
                <label class="field-label">
                    {{ t('schedule.rating') }}
                    <select v-model="rating" class="field-input">
                        <option v-for="value in ratingOptions" :key="value" :value="value">
                            {{
                                value === 'Livre'
                                    ? t('storage.sheet.ratingFree')
                                    : t('storage.sheet.ratingAge', { age: value })
                            }}
                        </option>
                    </select>
                </label>
            </div>
        </div>
    </div>

    <div v-if="!editing && safeName" class="bg-base-200 rounded-md px-3 py-2.5 flex flex-col gap-1">
        <div class="text-xs text-subtle">{{ t('storage.live.savedAs') }}</div>
        <pre
            class="m-0 font-mono text-xs leading-relaxed whitespace-pre-wrap">{{ storageJoin(LIVES_FOLDER, safeName) }}/
   signal.live   <span class="text-base-content/50">← {{ t('schedule.fileAddress') }}</span>
   tvshow.nfo    <span class="text-base-content/50">← {{ t('schedule.fileGuide') }}</span>
   poster.jpg    <span class="text-base-content/50">← {{ t('schedule.filePoster') }}</span>
   fanart.jpg    <span class="text-base-content/50">← {{ t('storage.live.fileBackdrop') }}</span></pre>
    </div>
</template>
