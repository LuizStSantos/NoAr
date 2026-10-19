<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import { sendFileInChunks } from '@/composables/chunkedUpload'
import { EVERY_DAY, MONDAY_TO_FRIDAY, SCHEDULE_COLORS, WEEKDAY_BITS, useScheduleText } from '@/composables/schedule'
import { useAuth } from '@/stores/auth'
import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'
import { useSchedule } from '@/stores/schedule'

const props = defineProps<{ draft: LiveDraft | null }>()
const emit = defineEmits<{ close: []; saved: [] }>()

const { t } = useI18n()
const { weekdayLetters } = useScheduleText()
const authStore = useAuth()
const configStore = useConfig()
const indexStore = useIndex()
const scheduleStore = useSchedule()

type Protocol = 'SRT' | 'RTMP' | 'HLS'

const PLACEHOLDERS: Record<Protocol, string> = {
    SRT: 'srt://203.0.113.10:9000?mode=caller&latency=200',
    RTMP: 'rtmp://203.0.113.10/live/stream-key',
    HLS: 'https://cdn.example.org/live/index.m3u8',
}
const RATINGS = ['L', '10', '12', '14', '16', '18']
const RETRY_OPTIONS = [60, 120, 300, 600]

const name = ref('')
const protocol = ref<Protocol>('SRT')
const url = ref('')
const date = ref('')
const start = ref('')
const end = ref('')
const repeatMode = ref<'once' | 'daily' | 'weekdays'>('once')
const repeatDays = ref(MONDAY_TO_FRIDAY)
const entryMode = ref<EntryMode>('interrupt')
const afterMode = ref<AfterMode>('clock')
const retrySeconds = ref(120)
const showInGuide = ref(true)
const guideTitle = ref('')
const plot = ref('')
const genre = ref('')
const rating = ref('L')
const poster = ref<File | null>(null)
const posterPreview = ref('')
const signal = ref<SignalTest | null>(null)
const testing = ref(false)
const saving = ref(false)
const error = ref('')

const existing = computed(() => props.draft?.live)
const safeName = computed(() => name.value.trim().replace(/[\\/:*?"<>|]/g, ''))

watch(
    () => props.draft,
    (draft) => {
        if (!draft) {
            return
        }

        const event = draft.event
        name.value = draft.live?.name ?? ''
        url.value = draft.live?.url ?? ''
        protocol.value = (
            ['SRT', 'RTMP', 'HLS'].includes(draft.live?.protocol ?? '') ? draft.live?.protocol : 'SRT'
        ) as Protocol
        date.value = draft.date
        start.value = draft.start
        end.value = draft.end
        repeatDays.value = event?.repeat_days || MONDAY_TO_FRIDAY
        repeatMode.value = !event?.repeat_days ? 'once' : event.repeat_days === EVERY_DAY ? 'daily' : 'weekdays'
        entryMode.value = event?.entry_mode ?? 'interrupt'
        afterMode.value = event?.after_mode ?? 'clock'
        retrySeconds.value = event?.retry_seconds ?? 120
        showInGuide.value = event?.show_in_guide ?? true
        guideTitle.value = draft.live?.title ?? ''
        plot.value = ''
        genre.value = ''
        rating.value = 'L'
        setPoster(null)
        signal.value = null
        error.value = ''
    },
    { immediate: true },
)

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
    const address = (existing.value?.url ?? url.value).trim()

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
        test.sample_rate ? `${test.sample_rate / 1000} kHz` : '',
    ]
        .filter(Boolean)
        .join(' · ')
})

function toggleDay(bit: number) {
    repeatDays.value = repeatDays.value & bit ? repeatDays.value & ~bit : repeatDays.value | bit
}

async function uploadPoster(folder: string) {
    const file = poster.value

    if (!file) {
        return
    }

    const extension = (file.name.split('.').pop() || 'jpg').toLowerCase()
    const renamed = new File([file], `poster.${extension}`, { type: file.type })
    const id = configStore.channels[configStore.i]?.id

    await sendFileInChunks(renamed, {
        url: `/api/file/${id}/upload?path=${encodeURIComponent(folder)}`,
        headers: authStore.authHeader,
        method: 'PUT',
    })
}

async function save(schedule: boolean) {
    error.value = ''

    let live = existing.value

    if (!live && (!safeName.value || !url.value.trim())) {
        error.value = t('schedule.needNameAndAddress')
        return
    }

    if (schedule && (!date.value || !start.value || !end.value)) {
        error.value = t('schedule.needLiveTimes')
        return
    }

    if (schedule && repeatMode.value === 'weekdays' && !repeatDays.value) {
        error.value = t('schedule.needWeekday')
        return
    }

    saving.value = true

    try {
        if (!live) {
            live = await scheduleStore.saveLive(safeName.value, url.value.trim(), {
                title: guideTitle.value.trim(),
                plot: plot.value.trim(),
                genre: genre.value.trim(),
                rating: rating.value ? `BR:${rating.value}` : '',
            })

            try {
                await uploadPoster(live.path.split('/').slice(0, -1).join('/'))
            } catch (err) {
                indexStore.msgAlert('warning', t('schedule.posterFailed', { error: String(err) }), 4)
            }
        }

        if (schedule) {
            const withSeconds = (value: string) => (value.length === 5 ? `${value}:00` : value)
            const bits = repeatMode.value === 'once' ? 0 : repeatMode.value === 'daily' ? EVERY_DAY : repeatDays.value

            await scheduleStore.saveEvent(
                {
                    kind: 'live',
                    title: props.draft?.event?.title ?? '',
                    sources: [live.path],
                    start_date: date.value,
                    start_time: withSeconds(start.value),
                    end_time: withSeconds(end.value),
                    repeat_days: bits,
                    entry_mode: entryMode.value,
                    after_mode: afterMode.value,
                    show_in_guide: showInGuide.value,
                    retry_seconds: retrySeconds.value,
                    enabled: true,
                },
                props.draft?.event?.id,
            )
        }

        indexStore.msgAlert('success', schedule ? t('schedule.saved') : t('schedule.liveSaved'), 2)
        await scheduleStore.loadLives()
        emit('saved')
    } catch (err) {
        error.value = err instanceof Error ? err.message : String(err)
    } finally {
        saving.value = false
    }
}
</script>

<template>
    <div v-if="draft" class="z-50 fixed inset-0 bg-black/50 overflow-auto flex justify-center items-start py-8 px-3">
        <div
            role="dialog"
            aria-modal="true"
            aria-labelledby="live-dialog-title"
            class="w-full max-w-170 bg-base-100 border border-base-content/20 rounded-lg shadow-xl flex flex-col"
        >
            <div class="flex items-center justify-between px-5 py-3 border-b border-base-300">
                <h2 id="live-dialog-title" class="text-lg font-semibold flex items-center gap-2">
                    <i class="bi bi-broadcast" :style="{ color: SCHEDULE_COLORS.live }" />
                    {{ existing ? t('schedule.scheduleLive') : t('schedule.newLive') }}
                </h2>
                <button class="btn-icon" :aria-label="t('schedule.close')" @click="emit('close')">
                    <i class="bi bi-x-lg" />
                </button>
            </div>

            <div class="px-5 py-4 flex flex-col gap-5">
                <template v-if="existing">
                    <div class="flex items-center gap-3 bg-base-200 rounded-md px-3 py-2">
                        <i class="bi bi-broadcast" :style="{ color: SCHEDULE_COLORS.live }" />
                        <div class="flex-1 min-w-0">
                            <div class="font-semibold truncate">{{ existing.title || existing.name }}</div>
                            <div class="font-mono text-xs text-muted truncate">{{ existing.url }}</div>
                        </div>
                        <span
                            class="text-xs border rounded-md px-1.5 leading-5"
                            :style="{ borderColor: SCHEDULE_COLORS.live }"
                        >
                            {{ existing.protocol }}
                        </span>
                        <button class="btn-quiet" :disabled="testing" @click="testSignal">
                            <span v-if="testing" class="loading loading-spinner loading-xs" />
                            {{ t('schedule.testSignal') }}
                        </button>
                    </div>
                </template>
                <template v-else>
                    <label class="field-label">
                        {{ t('schedule.name') }}
                        <input v-model="name" class="field-input" :placeholder="t('schedule.liveNamePlaceholder')" />
                    </label>

                    <div class="flex flex-col gap-2">
                        <div class="text-xs text-muted">{{ t('schedule.signal') }}</div>
                        <div class="flex w-fit" role="group" :aria-label="t('schedule.protocol')">
                            <button
                                v-for="option in ['SRT', 'RTMP', 'HLS'] as Protocol[]"
                                :key="option"
                                class="border min-h-9 px-4 cursor-pointer first:rounded-s-field last:rounded-e-field -ms-px first:ms-0"
                                :class="
                                    protocol === option
                                        ? 'text-[#1c1c1c] font-semibold'
                                        : 'bg-transparent border-line-strong text-base-content'
                                "
                                :style="
                                    protocol === option
                                        ? { background: SCHEDULE_COLORS.live, borderColor: SCHEDULE_COLORS.live }
                                        : {}
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
                            <button class="btn-quiet" :disabled="testing" @click="testSignal">
                                <span v-if="testing" class="loading loading-spinner loading-xs" />
                                {{ t('schedule.testSignal') }}
                            </button>
                        </div>
                    </div>
                </template>

                <div
                    v-if="signalText"
                    class="text-sm flex items-center gap-2"
                    :class="signal?.ok ? '' : 'text-error'"
                    :style="signal?.ok ? { color: indexStore.darkMode ? '#8fd18f' : '#1f7a1f' } : {}"
                >
                    <i class="bi" :class="signal?.ok ? 'bi-check-lg' : 'bi-exclamation-triangle'" />
                    {{ signalText }}
                </div>

                <div class="grid grid-cols-3 gap-3">
                    <label class="field-label">
                        {{ t('schedule.day') }}
                        <input v-model="date" type="date" class="field-input" />
                    </label>
                    <label class="field-label">
                        {{ t('schedule.starts') }}
                        <input v-model="start" type="time" step="1" class="field-input font-mono" />
                    </label>
                    <label class="field-label">
                        {{ t('schedule.ends') }}
                        <input v-model="end" type="time" step="1" class="field-input font-mono" />
                    </label>
                </div>

                <div class="flex flex-wrap items-center gap-1.5">
                    <button
                        v-for="mode in ['once', 'daily', 'weekdays'] as const"
                        :key="mode"
                        class="rounded-field border min-h-9 px-3.5 cursor-pointer"
                        :class="
                            repeatMode === mode
                                ? 'bg-accent text-accent-content border-accent font-semibold'
                                : 'bg-transparent border-line-strong text-base-content'
                        "
                        :aria-pressed="repeatMode === mode"
                        @click="repeatMode = mode"
                    >
                        {{ t(`schedule.repeat_${mode}`) }}
                    </button>
                    <template v-if="repeatMode === 'weekdays'">
                        <span class="w-2" />
                        <button
                            v-for="(bit, index) in WEEKDAY_BITS"
                            :key="bit"
                            class="rounded-full border w-9 h-9 cursor-pointer"
                            :class="
                                repeatDays & bit
                                    ? 'bg-accent text-accent-content border-accent font-semibold'
                                    : 'bg-transparent border-line-strong text-base-content'
                            "
                            :aria-pressed="Boolean(repeatDays & bit)"
                            :aria-label="t('schedule.weekdayShort').split(',')[index]"
                            @click="toggleDay(bit)"
                        >
                            {{ weekdayLetters()[index] }}
                        </button>
                    </template>
                </div>

                <div class="grid grid-cols-2 gap-4">
                    <fieldset class="flex flex-col gap-2">
                        <legend class="text-xs text-muted mb-2">{{ t('schedule.whenStarting') }}</legend>
                        <label class="flex gap-2 items-center cursor-pointer">
                            <input v-model="entryMode" type="radio" value="interrupt" class="radio radio-sm" />
                            {{ t('schedule.liveOnTime') }}
                        </label>
                        <label class="flex gap-2 items-center cursor-pointer">
                            <input v-model="entryMode" type="radio" value="after_clip" class="radio radio-sm" />
                            {{ t('schedule.afterClip') }}
                        </label>
                    </fieldset>
                    <fieldset class="flex flex-col gap-2">
                        <legend class="text-xs text-muted mb-2">{{ t('schedule.whenEnding') }}</legend>
                        <label class="flex gap-2 items-center cursor-pointer">
                            <input v-model="afterMode" type="radio" value="clock" class="radio radio-sm" />
                            {{ t('schedule.liveBackOnClock') }}
                        </label>
                        <label class="flex gap-2 items-center cursor-pointer">
                            <input v-model="afterMode" type="radio" value="continue" class="radio radio-sm" />
                            {{ t('schedule.liveBackWhereStopped') }}
                        </label>
                    </fieldset>
                </div>

                <label class="field-label">
                    {{ t('schedule.ifSignalDrops') }}
                    <select v-model.number="retrySeconds" class="field-input">
                        <option v-for="seconds in RETRY_OPTIONS" :key="seconds" :value="seconds">
                            {{ t('schedule.retryFor', { minutes: seconds / 60 }) }}
                        </option>
                    </select>
                </label>

                <div class="border-t border-base-300 pt-4 flex flex-col gap-3">
                    <label class="flex items-center justify-between cursor-pointer">
                        <span class="font-semibold">{{ t('schedule.inGuide') }}</span>
                        <input v-model="showInGuide" type="checkbox" class="toggle toggle-accent" />
                    </label>
                    <p v-if="existing" class="text-xs text-subtle">{{ t('schedule.guideFromFolder') }}</p>
                    <div v-else class="grid grid-cols-[132px_minmax(0,1fr)] gap-4">
                        <label
                            class="h-44 border border-dashed border-base-content/40 rounded-md bg-base-200 flex flex-col items-center justify-center gap-1 text-xs text-subtle text-center p-2 cursor-pointer overflow-hidden"
                            @dragover.prevent
                            @drop.prevent="dropPoster"
                        >
                            <img
                                v-if="posterPreview"
                                :src="posterPreview"
                                alt=""
                                class="w-full h-full object-cover rounded"
                            />
                            <template v-else>
                                <i class="bi bi-image text-2xl" />
                                {{ t('schedule.posterDrop') }}
                            </template>
                            <input
                                type="file"
                                accept="image/jpeg,image/png,image/webp"
                                class="sr-only"
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
                                        <option v-for="value in RATINGS" :key="value" :value="value">
                                            {{ value === 'L' ? t('schedule.ratingFree') : value }}
                                        </option>
                                    </select>
                                </label>
                            </div>
                        </div>
                    </div>
                </div>

                <div v-if="!existing && safeName" class="bg-base-200 rounded-md px-3 py-2.5 flex flex-col gap-1">
                    <div class="text-xs text-subtle">{{ t('schedule.savedAs') }}</div>
                    <pre class="font-mono text-xs leading-relaxed whitespace-pre-wrap">Lives/{{ safeName }}/
   signal.live   <span class="text-base-content/50">← {{ t('schedule.fileAddress') }}</span>
   tvshow.nfo    <span class="text-base-content/50">← {{ t('schedule.fileGuide') }}</span>
   poster.jpg    <span class="text-base-content/50">← {{ t('schedule.filePoster') }}</span></pre>
                </div>

                <p v-if="error" class="text-error text-sm" role="alert">{{ error }}</p>
            </div>

            <div class="flex justify-end gap-2 px-5 py-3 border-t border-base-300">
                <button class="btn-quiet" @click="emit('close')">{{ t('cancel') }}</button>
                <button v-if="!existing" class="btn-quiet" :disabled="saving" @click="save(false)">
                    {{ t('schedule.saveOnly') }}
                </button>
                <button class="btn-main" :disabled="saving" @click="save(true)">
                    <span v-if="saving" class="loading loading-spinner loading-sm" />
                    {{ existing ? t('schedule.schedule') : t('schedule.saveAndSchedule') }}
                </button>
            </div>
        </div>
    </div>
</template>
