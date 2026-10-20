<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { onClickOutside } from '@vueuse/core'

import { duration, type PickGroups, type PickOption } from '@/composables/report'

/**
 * Narrow the report to a file, a program, a live or a scheduled event: a
 * field with the chosen ones as chips and a list of what airs in the range.
 */
const props = defineProps<{
    groups: PickGroups
    /** `day` or `period`, for the wording of the list. */
    range: 'day' | 'period'
}>()

const picks = defineModel<ReportPick[]>({ required: true })

const { t } = useI18n()

const root = ref<HTMLElement>()
const input = ref<HTMLInputElement>()
const open = ref(false)
const query = ref('')

onClickOutside(root, () => (open.value = false))

const columns = computed(() => {
    const needle = query.value.trim().toLowerCase()
    const match = (option: PickOption) => !needle || `${option.label} ${option.detail}`.toLowerCase().includes(needle)
    const group = (key: keyof PickGroups, limit: number) => {
        const options = props.groups[key].filter(match)

        return { key, options: needle ? options : options.slice(0, limit), more: needle ? 0 : options.length - limit }
    }

    return [
        [group('programs', 6), group('lives', 4)],
        [group('commercials', 8), group('events', 6)],
    ].map((column) => column.filter((entry) => entry.options.length))
})

const empty = computed(() => columns.value.every((column) => !column.length))

function key(pick: ReportPick): string {
    return `${pick.type}:${pick.value}`
}

function isPicked(option: PickOption): boolean {
    return picks.value.some((pick) => key(pick) === key(option))
}

function toggle(option: PickOption) {
    const { type, value, label, group } = option

    picks.value = isPicked(option)
        ? picks.value.filter((pick) => key(pick) !== key(option))
        : [...picks.value, { type, value, label, group }]
    query.value = ''
    input.value?.focus()
}

function remove(pick: ReportPick) {
    picks.value = picks.value.filter((other) => key(other) !== key(pick))
}

function chipClass(group: ReportPick['group']): string {
    switch (group) {
        case 'commercial':
        case 'folder':
            return 'bg-after-clip/15 text-after-clip border-after-clip/40'
        case 'live':
            return 'bg-live/15 text-live border-live/40'
        default:
            return 'bg-raised text-base-content border-line-strong'
    }
}

function onBackspace() {
    if (!query.value && picks.value.length) {
        picks.value = picks.value.slice(0, -1)
    }
}
</script>

<template>
    <div ref="root" class="relative flex-[1_1_320px] min-w-0 max-w-[480px]" @keydown.esc="open = false">
        <div
            class="flex flex-wrap items-center gap-1.5 bg-base-200 border rounded-field ps-3 pe-1.5 py-1 min-h-10"
            :class="open ? 'border-accent' : 'border-line-strong'"
        >
            <i class="bi bi-search text-xs text-muted" aria-hidden="true" />
            <span
                v-for="pick in picks"
                :key="key(pick)"
                class="inline-flex items-center gap-1 border rounded-md ps-2 text-[13px] max-w-full"
                :class="chipClass(pick.group)"
            >
                <span class="truncate">{{ pick.label }}</span>
                <button
                    type="button"
                    class="inline-flex items-center justify-center min-w-7 min-h-7 bg-transparent border-0 cursor-pointer text-inherit"
                    :aria-label="t('report.picker.remove', { name: pick.label })"
                    @click="remove(pick)"
                >
                    <i class="bi bi-x text-base" aria-hidden="true" />
                </button>
            </span>
            <input
                ref="input"
                v-model="query"
                type="search"
                :placeholder="picks.length ? t('report.picker.addMore') : t('report.picker.placeholder')"
                :aria-label="t('report.picker.label')"
                :aria-expanded="open"
                class="bg-transparent border-0 text-base-content flex-1 min-w-30 min-h-8 outline-none"
                @focus="open = true"
                @input="open = true"
                @keydown.backspace="onBackspace"
            />
        </div>

        <section
            v-if="open"
            :aria-label="t('report.picker.listLabel')"
            class="absolute z-30 end-0 top-full mt-2 w-[min(900px,calc(100vw-32px))] bg-base-100 border border-line-strong rounded-box shadow-xl p-3.5 flex flex-col gap-3"
        >
            <div class="flex flex-wrap items-center gap-2 text-[13px] text-muted">
                <span class="flex-[1_1_300px]">
                    {{ range === 'day' ? t('report.picker.introDay') : t('report.picker.introPeriod') }}
                </span>
                <span>{{ t('report.picker.many') }}</span>
            </div>
            <p v-if="empty" class="m-0 text-muted text-sm">{{ t('report.picker.none') }}</p>
            <div v-else class="flex flex-wrap gap-3.5">
                <div
                    v-for="(column, index) in columns"
                    :key="index"
                    class="flex-[1_1_380px] min-w-0 flex flex-col gap-3"
                >
                    <div
                        v-for="entry in column"
                        :key="entry.key"
                        role="group"
                        :aria-label="t(`report.picker.groups.${entry.key}`)"
                        class="flex flex-col gap-0.5"
                    >
                        <div class="text-[11px] text-muted tracking-[0.08em] uppercase px-2 pb-1">
                            {{ t(`report.picker.groups.${entry.key}`) }}
                        </div>
                        <button
                            v-for="option in entry.options"
                            :key="key(option)"
                            type="button"
                            class="flex items-center gap-2.5 bg-transparent border-0 rounded-lg px-2 py-1 min-h-10 text-start cursor-pointer hover:bg-base-200"
                            :class="isPicked(option) ? 'bg-base-300' : ''"
                            :aria-pressed="isPicked(option)"
                            @click="toggle(option)"
                        >
                            <span class="flex-1 min-w-0 flex flex-col">
                                <span class="truncate">
                                    {{ option.label }}
                                    <span
                                        v-if="option.group === 'folder'"
                                        class="ms-1 text-[11px] border rounded-md px-1.5 bg-after-clip/15 text-after-clip border-after-clip/40"
                                    >
                                        {{ t('report.picker.folderTag') }}
                                    </span>
                                </span>
                                <span v-if="option.detail" class="font-mono text-xs text-muted truncate">
                                    {{ option.detail }}
                                </span>
                            </span>
                            <span class="text-xs text-muted whitespace-nowrap tabular">
                                {{ option.count }}× · {{ duration(option.seconds) }}
                            </span>
                            <i v-if="isPicked(option)" class="bi bi-check2 text-ok" aria-hidden="true" />
                        </button>
                        <span v-if="entry.more > 0" class="px-2 text-xs text-muted">
                            {{ t('report.picker.more', { count: entry.more }) }}
                        </span>
                    </div>
                </div>
            </div>
            <div class="flex justify-end">
                <button type="button" class="btn-quiet min-h-9" @click="open = false">
                    {{ t('report.picker.done') }}
                </button>
            </div>
        </section>
    </div>
</template>
