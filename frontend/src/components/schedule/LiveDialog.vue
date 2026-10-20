<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import LiveFields from '@/components/live/LiveFields.vue'
import LiveTimes from '@/components/schedule/LiveTimes.vue'
import { EVERY_DAY, MONDAY_TO_FRIDAY, SCHEDULE_COLORS } from '@/composables/schedule'
import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'
import { useSchedule } from '@/stores/schedule'

/**
 * Schedule a live: a new one, described with the same fields as in Storage,
 * or one that already exists in the storage.
 */
const props = defineProps<{ draft: LiveDraft | null }>()
const emit = defineEmits<{ close: []; saved: [] }>()

const { t } = useI18n()
const configStore = useConfig()
const indexStore = useIndex()
const scheduleStore = useSchedule()

const date = ref('')
const start = ref('')
const end = ref('')
const repeatMode = ref<'once' | 'daily' | 'weekdays'>('once')
const repeatDays = ref(MONDAY_TO_FRIDAY)
const entryMode = ref<EntryMode>('interrupt')
const afterMode = ref<AfterMode>('clock')
const retrySeconds = ref(120)
const showInGuide = ref(true)
const signal = ref<SignalTest | null>(null)
const testing = ref(false)
const saving = ref(false)
const error = ref('')
/** A fresh form for every draft. */
const formKey = ref(0)
const form = ref<InstanceType<typeof LiveFields>>()

const existing = computed(() => props.draft?.live)
const channelId = computed(() => configStore.channels[configStore.i]?.id ?? 0)

watch(
    () => props.draft,
    (draft) => {
        if (!draft) {
            return
        }

        const event = draft.event
        date.value = draft.date
        start.value = draft.start
        end.value = draft.end
        repeatDays.value = event?.repeat_days || MONDAY_TO_FRIDAY
        repeatMode.value = !event?.repeat_days ? 'once' : event.repeat_days === EVERY_DAY ? 'daily' : 'weekdays'
        entryMode.value = event?.entry_mode ?? 'interrupt'
        afterMode.value = event?.after_mode ?? 'clock'
        retrySeconds.value = event?.retry_seconds ?? 120
        showInGuide.value = event?.show_in_guide ?? true
        signal.value = null
        error.value = ''
        formKey.value++
    },
    { immediate: true },
)

/** Test the signal of a live that already exists. */
async function testSignal() {
    const address = existing.value?.url.trim()

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

async function save(schedule: boolean) {
    error.value = existing.value ? '' : (form.value?.check() ?? '')

    if (error.value) {
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
        const live = existing.value ?? (await form.value?.submit())

        if (!live) {
            return
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
                    <div
                        v-if="signalText"
                        class="text-sm flex items-center gap-2"
                        :class="signal?.ok ? 'text-ok' : 'text-error'"
                        role="status"
                    >
                        <i class="bi" :class="signal?.ok ? 'bi-check-lg' : 'bi-exclamation-triangle'" />
                        {{ signalText }}
                    </div>

                    <LiveTimes
                        v-model:date="date"
                        v-model:start="start"
                        v-model:end="end"
                        v-model:repeat-mode="repeatMode"
                        v-model:repeat-days="repeatDays"
                        v-model:entry-mode="entryMode"
                        v-model:after-mode="afterMode"
                        v-model:retry-seconds="retrySeconds"
                    />

                    <div class="border-t border-base-300 pt-4 flex flex-col gap-3">
                        <label class="flex items-center justify-between cursor-pointer">
                            <span class="font-semibold">{{ t('schedule.inGuide') }}</span>
                            <input v-model="showInGuide" type="checkbox" class="toggle toggle-accent" />
                        </label>
                        <p class="text-xs text-subtle">{{ t('schedule.guideFromFolder') }}</p>
                    </div>
                </template>

                <LiveFields v-else ref="form" :key="formKey" :channel-id="channelId" :live="null" :sheet="null">
                    <template #between>
                        <LiveTimes
                            v-model:date="date"
                            v-model:start="start"
                            v-model:end="end"
                            v-model:repeat-mode="repeatMode"
                            v-model:repeat-days="repeatDays"
                            v-model:entry-mode="entryMode"
                            v-model:after-mode="afterMode"
                            v-model:retry-seconds="retrySeconds"
                        />
                    </template>
                    <template #guide-header>
                        <label class="flex items-center justify-between cursor-pointer">
                            <span class="font-semibold">{{ t('schedule.inGuide') }}</span>
                            <input v-model="showInGuide" type="checkbox" class="toggle toggle-accent" />
                        </label>
                    </template>
                </LiveFields>

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
