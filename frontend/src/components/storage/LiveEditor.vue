<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import { emptyFields, imageUrl, saveSheet, storageJoin, uploadArtwork } from '@/composables/storage'
import { useIndex } from '@/stores/index'
import { useSchedule } from '@/stores/schedule'

/**
 * Create a live from Storage, or change an existing one: its address and the
 * guide data of its folder. Scheduling stays in the Scheduler.
 */
const props = defineProps<{
    channelId: number
    /** The live to change; a new one is created in `Lives/` when missing. */
    live: LiveSource | null
    /** The live folder's sheet, to start from when changing it. */
    sheet: MediaSheet | null
}>()

const emit = defineEmits<{
    close: []
    saved: [live: LiveSource, schedule: boolean]
}>()

const { t } = useI18n()
const indexStore = useIndex()
const scheduleStore = useSchedule()

type Protocol = 'SRT' | 'RTMP' | 'HLS'

const PLACEHOLDERS: Record<Protocol, string> = {
    SRT: 'srt://203.0.113.10:9000?mode=caller&latency=200',
    RTMP: 'rtmp://203.0.113.10/live/stream-key',
    HLS: 'https://cdn.example.org/live/index.m3u8',
}
const RATINGS = ['Livre', '10', '12', '14', '16', '18']

const fields = props.sheet?.fields

const name = ref(props.live?.name ?? '')
const protocol = ref<Protocol>(protocolOf(props.live?.protocol))
const url = ref(props.live?.url ?? '')
const guideTitle = ref(fields?.title ?? '')
const plot = ref(fields?.plot ?? '')
const genre = ref(fields?.genres.join(', ') ?? '')
const rating = ref(fields?.rating || 'Livre')
const poster = ref<File | null>(null)
const posterPreview = ref('')
const currentPoster = ref('')
const signal = ref<SignalTest | null>(null)
const testing = ref(false)
const saving = ref(false)
const error = ref('')

const editing = computed(() => Boolean(props.live))
const safeName = computed(() => name.value.trim().replace(/[\\/:*?"<>|]/g, ''))
const ratingOptions = computed(() => (RATINGS.includes(rating.value) ? RATINGS : [...RATINGS, rating.value]))

function protocolOf(value?: string): Protocol {
    return value === 'RTMP' || value === 'HLS' ? value : 'SRT'
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
    }
})

function folderOf(path: string): string {
    return path.split('/').slice(0, -1).join('/')
}

onMounted(async () => {
    if (props.sheet?.poster) {
        try {
            currentPoster.value = await imageUrl(props.channelId, props.sheet.poster)
        } catch {
            // The drop area stays empty.
        }
    }
})

function setPoster(file: File | null) {
    if (posterPreview.value) {
        URL.revokeObjectURL(posterPreview.value)
    }

    poster.value = file
    posterPreview.value = file ? URL.createObjectURL(file) : ''
}

function pickPoster(event: Event) {
    const file = (event.target as HTMLInputElement).files?.[0]

    if (file?.type.startsWith('image/')) {
        setPoster(file)
    }
}

function dropPoster(event: DragEvent) {
    const file = event.dataTransfer?.files?.[0]

    if (file?.type.startsWith('image/')) {
        setPoster(file)
    }
}

onBeforeUnmount(() => setPoster(null))

async function testSignal() {
    const address = url.value.trim()

    if (!address) {
        error.value = t('schedule.needAddress')
        return
    }

    testing.value = true
    signal.value = null
    error.value = ''

    try {
        signal.value = await scheduleStore.testSignal(address)
    } catch (err) {
        error.value = err instanceof Error ? err.message : String(err)
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

async function save(schedule: boolean) {
    error.value = ''

    if (!safeName.value || !url.value.trim()) {
        error.value = t('schedule.needNameAndAddress')
        return
    }

    saving.value = true

    try {
        let live: LiveSource

        if (props.live) {
            live =
                url.value.trim() === props.live.url
                    ? props.live
                    : await scheduleStore.updateLive(props.live.path, url.value.trim())

            // Through the sheet, so fields the dialog does not show are kept.
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

        if (poster.value) {
            try {
                await uploadArtwork(props.channelId, folderOf(live.path), 'poster', poster.value)
            } catch (err) {
                indexStore.msgAlert('warning', t('schedule.posterFailed', { error: String(err) }), 4)
            }
        }

        indexStore.msgAlert('success', t('storage.live.saved'), 2)
        emit('saved', live, schedule)
    } catch (err) {
        error.value = err instanceof Error ? err.message : String(err)
    } finally {
        saving.value = false
    }
}
</script>

<template>
    <div class="z-50 fixed inset-0 bg-black/50 overflow-auto flex justify-center items-start py-8 px-3">
        <div
            role="dialog"
            aria-modal="true"
            aria-labelledby="live-editor-title"
            class="w-full max-w-170 bg-base-100 border border-base-content/20 rounded-lg shadow-xl flex flex-col"
            @keydown.esc="emit('close')"
        >
            <div class="flex items-center justify-between px-5 py-3 border-b border-base-300">
                <h2 id="live-editor-title" class="text-lg font-semibold flex items-center gap-2 m-0">
                    <i class="bi bi-broadcast text-live" aria-hidden="true" />
                    {{ editing ? t('storage.live.editTitle') : t('storage.live.newTitle') }}
                </h2>
                <button type="button" class="btn-icon" :aria-label="t('schedule.close')" @click="emit('close')">
                    <i class="bi bi-x-lg" aria-hidden="true" />
                </button>
            </div>

            <div class="px-5 py-4 flex flex-col gap-5">
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
                            v-for="option in ['SRT', 'RTMP', 'HLS'] as Protocol[]"
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
                        v-if="signalText"
                        class="text-sm flex items-center gap-2"
                        :class="signal?.ok ? 'text-ok' : 'text-error'"
                        role="status"
                    >
                        <i class="bi" :class="signal?.ok ? 'bi-check-lg' : 'bi-exclamation-triangle'" />
                        {{ signalText }}
                    </div>
                </div>

                <div class="border-t border-base-300 pt-4 flex flex-col gap-3">
                    <span class="font-semibold">{{ t('schedule.inGuide') }}</span>
                    <div class="grid grid-cols-[132px_minmax(0,1fr)] gap-4">
                        <label
                            class="h-44 border border-dashed border-base-content/40 rounded-md bg-base-200 flex flex-col items-center justify-center gap-1 text-xs text-subtle text-center p-2 cursor-pointer overflow-hidden"
                            @dragover.prevent
                            @drop.prevent="dropPoster"
                        >
                            <img
                                v-if="posterPreview || currentPoster"
                                :src="posterPreview || currentPoster"
                                alt=""
                                class="w-full h-full object-cover rounded"
                            />
                            <template v-else>
                                <i class="bi bi-image text-2xl" aria-hidden="true" />
                                {{ t('schedule.posterDrop') }}
                            </template>
                            <input
                                type="file"
                                accept="image/jpeg,image/png,image/webp"
                                class="sr-only"
                                :aria-label="t('storage.live.poster')"
                                @change="pickPoster"
                            />
                        </label>
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
                </div>

                <div v-if="!editing && safeName" class="bg-base-200 rounded-md px-3 py-2.5 flex flex-col gap-1">
                    <div class="text-xs text-subtle">{{ t('storage.live.savedAs') }}</div>
                    <pre class="m-0 font-mono text-xs leading-relaxed whitespace-pre-wrap">{{
                        storageJoin('Lives', safeName)
                    }}/
   signal.live   <span class="text-base-content/50">← {{ t('schedule.fileAddress') }}</span>
   tvshow.nfo    <span class="text-base-content/50">← {{ t('schedule.fileGuide') }}</span>
   poster.jpg    <span class="text-base-content/50">← {{ t('schedule.filePoster') }}</span></pre>
                </div>
                <p v-if="!editing" class="m-0 text-[13px] text-muted">{{ t('storage.live.scheduleLater') }}</p>

                <p v-if="error" class="m-0 text-error text-sm" role="alert">{{ error }}</p>
            </div>

            <div class="flex flex-wrap justify-end gap-2 px-5 py-3 border-t border-base-300">
                <button type="button" class="btn-quiet" @click="emit('close')">{{ t('cancel') }}</button>
                <button type="button" class="btn-quiet" :disabled="saving" @click="save(true)">
                    {{ t('storage.live.saveAndSchedule') }}
                </button>
                <button type="button" class="btn-main" :disabled="saving" @click="save(false)">
                    <span v-if="saving" class="loading loading-spinner loading-sm" />
                    {{ editing ? t('storage.live.save') : t('storage.live.saveNew') }}
                </button>
            </div>
        </div>
    </div>
</template>
