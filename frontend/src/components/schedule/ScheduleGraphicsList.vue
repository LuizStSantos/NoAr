<script setup lang="ts">
import { useI18n } from 'vue-i18n'

import { useScheduleText } from '@/composables/schedule'
import {
    SCHEDULE_GRAPHICS_COLOR as GRAPHICS_COLOR,
    type ScheduleGraphicsBlock,
    useScheduledGraphicsText,
} from '@/composables/schedulePresentation'

/** Scheduled graphics remain event records; their edit/delete decisions belong to the page. */
defineProps<{ blocks: ScheduleGraphicsBlock[] }>()
const emit = defineEmits<{ create: []; edit: [event: GraphicsEvent]; remove: [event: GraphicsEvent] }>()
const { t } = useI18n()
const { repeatLabel } = useScheduleText()
const { graphicsSummary } = useScheduledGraphicsText()
</script>
<template>
    <section :aria-label="t('schedule.graphics.list')" class="card-box">
        <div class="flex flex-wrap items-center justify-between gap-2 px-4.5 py-3.5 border-b border-line">
            <h2 class="m-0 text-base font-semibold inline-flex items-center gap-2">
                <span class="w-3 h-3 rounded-[3px]" :style="{ background: GRAPHICS_COLOR }" />
                {{ t('schedule.graphics.list') }}
            </h2>
            <button class="btn-quiet" @click="emit('create')">
                <i class="bi bi-plus-lg" aria-hidden="true" />{{ t('schedule.graphics.new') }}
            </button>
        </div>
        <ul class="m-0 p-0 list-none">
            <li
                v-for="block in blocks"
                :key="`grl-${block.event.id}`"
                class="flex items-center gap-3 px-4.5 py-3 border-t border-line first:border-t-0"
            >
                <span class="flex-1 min-w-0 flex flex-wrap items-center gap-x-3 gap-y-1">
                    <span class="font-mono text-sm w-28">
                        {{ block.event.start_time.slice(0, 5) }}
                        –
                        {{ block.event.end_time?.slice(0, 5) ?? t('schedule.graphics.untilChanged') }}
                    </span>
                    <span class="flex-1 min-w-40">
                        <strong class="font-semibold">{{ block.event.title }}</strong>
                        <span class="block text-[13px] text-muted">
                            {{ graphicsSummary(block.event) }} · {{ repeatLabel(block.event.repeat_days) }}
                        </span>
                    </span>
                </span>
                <span class="flex gap-2 shrink-0">
                    <button
                        class="btn-icon min-w-10 min-h-10 border border-line-strong"
                        :aria-label="t('schedule.edit')"
                        :title="t('schedule.edit')"
                        @click="emit('edit', block.event)"
                    >
                        <i class="bi bi-pencil" aria-hidden="true" />
                    </button>
                    <button
                        class="btn-icon min-w-10 min-h-10 border border-line-strong text-error"
                        :aria-label="t('schedule.graphics.remove')"
                        :title="t('schedule.graphics.remove')"
                        @click="emit('remove', block.event)"
                    >
                        <i class="bi bi-trash" aria-hidden="true" />
                    </button>
                </span>
            </li>
            <li v-if="!blocks.length" class="px-4.5 py-6 text-center text-muted">
                {{ t('schedule.graphics.none') }}
            </li>
        </ul>
    </section>
</template>
