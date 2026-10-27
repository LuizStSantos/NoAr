<script setup lang="ts">
import { ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import SceneActionsFields from '@/components/graphics/SceneActionsFields.vue'
import StoragePicker from '@/components/schedule/StoragePicker.vue'
import GenericModal from '@/components/utils/GenericModal.vue'
import { EVERY_DAY, MONDAY_TO_FRIDAY, WEEKDAY_BITS, useScheduleText } from '@/composables/schedule'
import { useIndex } from '@/stores/index'
import { useSchedule } from '@/stores/schedule'
import type { GraphicsScene } from '@/types/playout_config'

/**
 * Schedules graphics at a time: what happens to the logo (the channel's
 * too) and the L-shape (its corner and size) when it starts, undone when it
 * ends. The programme goes on as it is.
 */
const props = defineProps<{
    /** The event to change, or the day and time a new one starts. */
    draft: { event: GraphicsEvent | null; date: string; time: string } | null
}>()
const emit = defineEmits<{ close: []; saved: [] }>()

const { t } = useI18n()
const { weekdayLetters } = useScheduleText()
const indexStore = useIndex()
const scheduleStore = useSchedule()

function emptyActions(): GraphicsScene {
    return {
        name: '',
        logo: 'keep',
        logo_image: null,
        logo_position: null,
        lshape: 'on',
        lshape_graphic: null,
        lshape_corner: null,
        lshape_scale: null,
    }
}

const title = ref('')
const date = ref('')
const start = ref('')
const end = ref('')
const repeatMode = ref<'once' | 'daily' | 'weekdays'>('once')
const repeatDays = ref(MONDAY_TO_FRIDAY)
const actions = ref<GraphicsScene>(emptyActions())
const picking = ref<'logo_image' | 'lshape_graphic' | null>(null)
const saving = ref(false)
const error = ref('')

watch(
    () => props.draft,
    (draft) => {
        if (!draft) {
            return
        }

        const event = draft.event
        title.value = event?.title ?? ''
        date.value = event?.start_date ?? draft.date
        start.value = (event?.start_time ?? draft.time).slice(0, 5)
        end.value = event?.end_time?.slice(0, 5) ?? ''
        repeatDays.value = event?.repeat_days || MONDAY_TO_FRIDAY
        repeatMode.value = !event?.repeat_days ? 'once' : event.repeat_days === EVERY_DAY ? 'daily' : 'weekdays'
        actions.value = event ? { ...event.actions } : emptyActions()
        error.value = ''
    },
    { immediate: true },
)

function toggleDay(bit: number) {
    repeatDays.value ^= bit
}

function pick(file: { path: string }) {
    if (picking.value) {
        actions.value[picking.value] = file.path
    }

    picking.value = null
}

async function save(ok: boolean) {
    if (!ok) {
        emit('close')
        return
    }

    if (repeatMode.value === 'weekdays' && !repeatDays.value) {
        error.value = t('schedule.needWeekday')
        return
    }

    saving.value = true
    error.value = ''

    try {
        await scheduleStore.saveGraphics(
            {
                title: title.value.trim() || t('schedule.graphics.defaultTitle'),
                actions: actions.value,
                start_date: date.value,
                start_time: start.value,
                end_time: end.value || null,
                repeat_days:
                    repeatMode.value === 'once' ? 0 : repeatMode.value === 'daily' ? EVERY_DAY : repeatDays.value,
                enabled: props.draft?.event?.enabled ?? true,
            },
            props.draft?.event?.id,
        )
        indexStore.msgAlert('success', t('schedule.graphics.saved'), 2)
        emit('saved')
    } catch (err: any) {
        error.value = err?.data ?? (err instanceof Error ? err.message : String(err))
    } finally {
        saving.value = false
    }
}
</script>
<template>
    <GenericModal
        :show="draft !== null"
        :title="draft?.event ? t('schedule.graphics.edit') : t('schedule.graphics.new')"
        :ok-label="t('schedule.graphics.save')"
        :ok-disabled="saving || !date || !start"
        :modal-action="save"
    >
        <div class="w-[min(760px,92vw)] flex flex-col gap-4">
            <p class="m-0 text-[13px] text-muted">{{ t('schedule.graphics.intro') }}</p>

            <label class="field-label">
                {{ t('graphics.scenes.name') }}
                <input
                    v-model="title"
                    class="field-input"
                    maxlength="80"
                    :placeholder="t('schedule.graphics.titlePlaceholder')"
                />
            </label>

            <div class="grid grid-cols-[repeat(auto-fit,minmax(150px,1fr))] gap-3">
                <label class="field-label">
                    {{ t('schedule.day') }}
                    <input v-model="date" type="date" class="field-input" />
                </label>
                <label class="field-label">
                    {{ t('schedule.graphics.start') }}
                    <input v-model="start" type="time" class="field-input" />
                </label>
                <label class="field-label">
                    {{ t('schedule.graphics.end') }}
                    <input v-model="end" type="time" class="field-input" />
                </label>
            </div>
            <span class="field-hint -mt-2">{{ t('schedule.graphics.endHint') }}</span>

            <div class="flex flex-col gap-2">
                <div class="text-xs text-muted">{{ t('schedule.repeat') }}</div>
                <div class="flex flex-wrap items-center gap-1.5">
                    <button
                        v-for="mode in ['once', 'daily', 'weekdays'] as const"
                        :key="mode"
                        type="button"
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
                            type="button"
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
            </div>

            <SceneActionsFields v-model="actions" @pick="picking = $event" />

            <p v-if="error" class="m-0 text-sm text-error" role="alert">{{ error }}</p>
        </div>
    </GenericModal>

    <GenericModal
        :show="picking !== null"
        :title="t('settings.choose')"
        hide-buttons
        :modal-action="() => (picking = null)"
    >
        <div class="w-[min(520px,90vw)] max-h-[60vh] flex flex-col">
            <StoragePicker @pick="pick" />
        </div>
    </GenericModal>
</template>
