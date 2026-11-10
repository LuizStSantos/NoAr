<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import ScheduleEventList from '@/components/schedule/ScheduleEventList.vue'
import ScheduleGraphicsList from '@/components/schedule/ScheduleGraphicsList.vue'
import { secondsFromDay } from '@/composables/schedule'
import type { ScheduleEventBlock, ScheduleGraphicsBlock } from '@/composables/schedulePresentation'

/**
 * The day's agenda: scheduled materials, lives and graphics in one card,
 * filtered by kind. It is as tall as its rows and scrolls inside on a long
 * day; the lists keep their own actions.
 */
const props = defineProps<{
    eventBlocks: ScheduleEventBlock[]
    graphicsBlocks: ScheduleGraphicsBlock[]
    lives: LiveSource[]
    date: string
    today: string
    nowSeconds: number
    activeEventId?: number
}>()
const emit = defineEmits<{
    edit: [occurrence: ScheduleOccurrence]
    trigger: [occurrence: ScheduleOccurrence]
    remove: [event: ScheduleEvent]
    editGraphics: [event: GraphicsEvent]
    removeGraphics: [event: GraphicsEvent]
    createGraphics: []
}>()
const { t } = useI18n()

type Filter = 'all' | 'materials' | 'lives' | 'graphics'
const filter = ref<Filter>('all')

const materials = computed(() => props.eventBlocks.filter((block) => block.occurrence.event.kind !== 'live'))
const lives = computed(() => props.eventBlocks.filter((block) => block.occurrence.event.kind === 'live'))
const shownEvents = computed(() =>
    filter.value === 'materials' ? materials.value : filter.value === 'lives' ? lives.value : props.eventBlocks,
)
const filters = computed(() =>
    (
        [
            ['all', props.eventBlocks.length + props.graphicsBlocks.length],
            ['materials', materials.value.length],
            ['lives', lives.value.length],
            ['graphics', props.graphicsBlocks.length],
        ] as const
    ).map(([name, count]) => ({ name, count })),
)
const isToday = computed(() => props.date === props.today)
const nextUp = computed(() =>
    props.eventBlocks
        .map((block) => block.occurrence)
        .find(
            (occurrence) =>
                !occurrence.status && isToday.value && secondsFromDay(occurrence.start, props.date) >= props.nowSeconds,
        ),
)
/** The event table shows its own empty row; only the graphics filter needs one. */
const noGraphics = computed(() => filter.value === 'graphics' && !props.graphicsBlocks.length)
</script>
<template>
    <section :aria-label="t('schedule.agenda.title')" class="card-box flex flex-col min-h-0">
        <div class="flex flex-wrap items-center gap-x-3 gap-y-2 px-4.5 py-3 border-b border-line">
            <h2 class="m-0 text-base font-semibold">
                {{ isToday ? t('schedule.todayEvents') : t('schedule.dayEvents') }}
            </h2>
            <div role="tablist" :aria-label="t('schedule.agenda.filter')" class="flex flex-wrap gap-1">
                <button
                    v-for="item in filters"
                    :key="item.name"
                    type="button"
                    role="tab"
                    class="min-h-8 px-2.5 rounded-field text-[13px] cursor-pointer"
                    :class="filter === item.name ? 'bg-base-300 font-semibold' : 'text-muted hover:bg-raised'"
                    :aria-selected="filter === item.name"
                    @click="filter = item.name"
                >
                    {{ t('schedule.agenda.' + item.name) }}
                    <span class="font-mono text-xs opacity-75">{{ item.count }}</span>
                </button>
            </div>
            <span v-if="nextUp" class="ms-auto text-[13px] text-muted">
                {{
                    t('schedule.agenda.next', {
                        title: nextUp.event.title,
                        minutes: Math.max(0, Math.ceil((secondsFromDay(nextUp.start, date) - nowSeconds) / 60)),
                    })
                }}
            </span>
        </div>
        <div class="schedule-agenda-body min-h-0 overflow-y-auto">
            <ScheduleEventList
                v-if="filter !== 'graphics'"
                bare
                :blocks="shownEvents"
                :lives="props.lives"
                :date="date"
                :today="today"
                :now-seconds="nowSeconds"
                :active-event-id="activeEventId"
                @trigger="emit('trigger', $event)"
                @edit="emit('edit', $event)"
                @remove="emit('remove', $event)"
            />
            <ScheduleGraphicsList
                v-if="(filter === 'all' || filter === 'graphics') && graphicsBlocks.length"
                bare
                :blocks="graphicsBlocks"
                @create="emit('createGraphics')"
                @edit="emit('editGraphics', $event)"
                @remove="emit('removeGraphics', $event)"
            />
            <p v-if="noGraphics" class="m-0 px-4 py-7 text-center text-muted">
                {{ t('schedule.graphics.none') }}
            </p>
        </div>
    </section>
</template>

<style scoped>
/* A long day scrolls inside the card instead of stretching the page. */
@media (min-width: 1024px) {
    .schedule-agenda-body {
        max-height: calc(100dvh - 28rem);
        min-height: 8rem;
    }
}
</style>
