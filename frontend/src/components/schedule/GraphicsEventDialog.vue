<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import TemplatePreview from '@/components/graphics/TemplatePreview.vue'
import { useGraphicsApi, type GraphicsTemplate } from '@/composables/graphicsTemplates'
import { sourceBindings, sourceValues, useGraphicsSources } from '@/composables/graphicsSources'
import { useConfig } from '@/stores/config'
import SceneActionsFields from '@/components/graphics/SceneActionsFields.vue'
import StoragePicker from '@/components/schedule/StoragePicker.vue'
import GenericModal from '@/components/utils/GenericModal.vue'
import { EVERY_DAY, MONDAY_TO_FRIDAY, WEEKDAY_BITS, useScheduleText } from '@/composables/schedule'
import { useIndex } from '@/stores/index'
import { useSchedule } from '@/stores/schedule'
import type { GraphicsScene } from '@/types/playout_config'

/**
 * Schedule Composer models without interrupting programme playback. Existing
 * scene-action events retain their compatible editor until explicitly changed.
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
        lshape: 'keep',
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
const graphicsApi = useGraphicsApi()
const configStore = useConfig()
const { sources } = useGraphicsSources(() => props.draft !== null)
const templates = ref<GraphicsTemplate[]>([])
const permanentIds = ref<number[]>([])
const templateId = ref<number | null>(null)
const fieldValues = ref<Record<string, string>>({})
const records = ref<Record<string, number>>({})
const legacy = ref(false)
const loaded = ref(false)
let loadVersion = 0
const selected = computed(() => templates.value.find((template) => template.id === templateId.value))
const isPermanent = computed(() => selected.value && permanentIds.value.includes(selected.value.id))
const mappings = computed(() => sourceBindings(selected.value))
const mappedSources = computed(() =>
    sources.value.filter((source) => Object.values(mappings.value).some((binding) => binding.source_id === source.id)),
)
const automaticValues = computed(() => sourceValues(selected.value, sources.value, records.value))
const previewValues = computed(() => ({
    ...automaticValues.value,
    ...Object.fromEntries(Object.entries(fieldValues.value).filter(([, value]) => value.length > 0)),
}))
const saving = ref(false)
const error = ref('')

watch(
    () => [props.draft, configStore.channels[configStore.i]?.id] as const,
    async ([draft]) => {
        const version = ++loadVersion

        if (!draft) {
            return
        }

        const event = draft.event
        title.value = event?.title ?? ''
        date.value = event?.start_date ?? draft.date
        start.value = (event?.start_time ?? draft.time).slice(0, 8)
        end.value = event?.end_time?.slice(0, 8) ?? ''
        repeatDays.value = event?.repeat_days || MONDAY_TO_FRIDAY
        repeatMode.value = !event?.repeat_days ? 'once' : event.repeat_days === EVERY_DAY ? 'daily' : 'weekdays'
        actions.value = event ? { ...event.actions } : emptyActions()
        error.value = ''
        legacy.value = Boolean(event && !event.template)
        templateId.value = event?.template?.template_id ?? null
        fieldValues.value = { ...event?.template?.values }
        records.value = { ...event?.template?.records }
        loaded.value = false

        try {
            const [models, channelModels] = await Promise.all([graphicsApi.templates(), graphicsApi.channel()])

            if (version !== loadVersion) {
                return
            }

            const scenes = channelModels.filter((model) => model.kind === 'scene').map((model) => model.template.id)
            templates.value = models.filter((model) => !scenes.includes(model.id))
            permanentIds.value = channelModels
                .filter((model) => ['logo', 'ticker', 'permanent'].includes(model.kind))
                .map((model) => model.template.id)
            loaded.value = true
        } catch (failure) {
            if (version === loadVersion) {
                error.value = failure instanceof Error ? failure.message : String(failure)
            }
        }
    },
    { immediate: true },
)

function chooseTemplate() {
    fieldValues.value = {}
    records.value = {}
}

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
                actions: legacy.value ? actions.value : emptyActions(),
                template: legacy.value
                    ? null
                    : {
                          template_id: selected.value!.id,
                          values: isPermanent.value ? {} : fieldValues.value,
                          records: isPermanent.value ? {} : records.value,
                      },
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
        :ok-disabled="saving || !date || !start || !loaded || (!legacy && !selected)"
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
                    <input v-model="start" type="time" step="1" class="field-input" />
                </label>
                <label class="field-label">
                    {{ t('schedule.graphics.end') }}
                    <input v-model="end" type="time" step="1" class="field-input" />
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

            <label v-if="draft?.event && !draft.event.template" class="flex items-center gap-2 text-sm">
                <input v-model="legacy" type="checkbox" class="checkbox checkbox-sm" />
                {{ t('schedule.graphics.legacy') }}
            </label>
            <SceneActionsFields v-if="legacy" v-model="actions" @pick="picking = $event" />
            <template v-else>
                <label class="field-label">
                    {{ t('schedule.graphics.template') }}
                    <select
                        v-model="templateId"
                        :aria-label="t('schedule.graphics.template')"
                        class="field-input"
                        :disabled="!loaded"
                        @change="chooseTemplate"
                    >
                        <option :value="null">{{ t('schedule.graphics.chooseTemplate') }}</option>
                        <option v-if="templateId && !selected" :value="templateId" disabled>
                            {{ t('schedule.graphics.missingTemplate', { id: templateId }) }}
                        </option>
                        <option v-for="model in templates" :key="model.id" :value="model.id">
                            {{ model.group ? model.group + ' / ' : '' }}{{ model.name }}
                        </option>
                    </select>
                </label>
                <p v-if="loaded && !templates.length" class="field-hint">{{ t('schedule.graphics.noTemplates') }}</p>
                <template v-if="selected">
                    <figure class="m-0 w-full max-w-[480px]">
                        <figcaption class="mb-1 text-xs font-semibold text-success">
                            {{ t('graphics.onAir.preview') }}
                        </figcaption>
                        <TemplatePreview
                            :template="selected"
                            :values="previewValues"
                            :records="records"
                            class="rounded-field border border-line-strong bg-[linear-gradient(180deg,#38596d_0%,#38596d_60%,#31543e_60%,#31543e_100%)]"
                        />
                    </figure>
                    <p v-if="isPermanent" class="field-hint">{{ t('schedule.graphics.permanentHint') }}</p>
                    <template v-else>
                        <label v-for="source in mappedSources" :key="source.id" class="field-label">
                            {{ t('schedule.graphics.record', { name: source.name }) }}
                            <select v-model="records[String(source.id)]" class="field-input">
                                <option :value="undefined">{{ t('schedule.graphics.savedRecord') }}</option>
                                <option v-for="(row, index) in source.rows" :key="index" :value="index">
                                    {{ index + 1 }} · {{ Object.values(row).slice(0, 2).join(' · ') }}
                                </option>
                            </select>
                        </label>
                        <label v-for="field in selected.fields" :key="field" class="field-label">
                            {{ '{' + field + '}' }}
                            <input
                                v-model="fieldValues[field]"
                                class="field-input"
                                maxlength="1000"
                                :placeholder="automaticValues[field] ?? ''"
                            />
                        </label>
                    </template>
                </template>
            </template>

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
