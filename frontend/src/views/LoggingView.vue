<script setup lang="ts">
import dayjs from 'dayjs'
import timezone from 'dayjs/plugin/timezone.js'
import utc from 'dayjs/plugin/utc.js'
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useHead } from '@unhead/vue'
import { useRouter } from 'vue-router'

import { authFetchRaw } from '@/composables/authFetch'
import { LEVEL_RANK, foldRepeats, parseLog, type LogLevel, type LogLine, type LogRow } from '@/composables/logLines'
import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'

dayjs.extend(utc)
dayjs.extend(timezone)

const { t } = useI18n()
const router = useRouter()
const configStore = useConfig()
const indexStore = useIndex()

useHead({ title: computed(() => t('log.title')) })

/** How often the open day is checked for new lines while live. */
const LIVE_SECONDS = 3
/** Rows drawn at once; earlier ones come on request. */
const PAGE = 1500
const LEVELS: LogLevel[] = ['ERROR', 'WARN', 'INFO', 'DEBUG']

const channel = computed(() => configStore.channels[configStore.i])
const today = () => dayjs().tz(configStore.timezone).format('YYYY-MM-DD')

const day = ref(today())
const lines = ref<LogLine[]>([])
const next = ref(0)
const loading = ref(false)
const level = ref<LogLevel>(readSetting('error_level', 'INFO') as LogLevel)
const search = ref('')
const fold = ref(readSetting('log-fold', 'true') === 'true')
const live = ref(readSetting('log-live', 'true') === 'true')
const openRuns = ref(new Set<number>())
const shown = ref(PAGE)
const now = ref(Date.now())
const scroller = ref<HTMLElement>()
let timer: ReturnType<typeof setInterval> | undefined

const isToday = computed(() => day.value === today())
const liveActive = computed(() => live.value && isToday.value)

function readSetting(key: string, fallback: string): string {
    try {
        return localStorage.getItem(key) ?? fallback
    } catch {
        return fallback
    }
}

function writeSetting(key: string, value: string) {
    try {
        localStorage.setItem(key, value)
    } catch {
        // Only a convenience: the choice is not kept.
    }
}

const counts = computed(() => ({
    errors: lines.value.filter((line) => line.level === 'ERROR' || line.level === 'FATAL').length,
    warnings: lines.value.filter((line) => line.level === 'WARN').length,
}))

const visible = computed(() => {
    const minimum = LEVEL_RANK[level.value] ?? 1
    const needle = search.value.trim().toLowerCase()

    return lines.value.filter(
        (line) =>
            (line.level === null || LEVEL_RANK[line.level] >= minimum) &&
            (!needle || line.plain.toLowerCase().includes(needle)),
    )
})

const rows = computed<LogRow[]>(() =>
    fold.value ? foldRepeats(visible.value, openRuns.value) : visible.value.map((line) => ({ line })),
)
const hiddenRows = computed(() => Math.max(0, rows.value.length - shown.value))
const drawnRows = computed(() => rows.value.slice(hiddenRows.value))

/** When the newest line was written, from its timestamp (channel time). */
const lastLineAt = computed(() => {
    const stamp = lines.value[lines.value.length - 1]?.stamp

    return stamp ? dayjs.tz(stamp.slice(0, 19), configStore.timezone).valueOf() : null
})

const lastLineAge = computed(() => {
    if (lastLineAt.value === null) {
        return ''
    }

    const seconds = Math.max(0, Math.round((now.value - lastLineAt.value) / 1000))

    return seconds < 60 ? `${seconds} s` : `${Math.round(seconds / 60)} min`
})

function logUrl(from: number, download = false): string {
    const date = isToday.value ? '' : day.value
    const query = new URLSearchParams({ date, timezone: configStore.timezone, from: String(from) })

    if (download) {
        query.set('download', 'true')
    }

    return `/api/log/${channel.value?.id}?${query}`
}

function atBottom(): boolean {
    const element = scroller.value

    return !element || element.scrollHeight - element.scrollTop - element.clientHeight < 60
}

function toEnd() {
    nextTick(() => {
        if (scroller.value) {
            scroller.value.scrollTop = scroller.value.scrollHeight
        }
    })
}

async function load() {
    if (!channel.value) {
        return
    }

    loading.value = true
    openRuns.value = new Set()
    shown.value = PAGE

    try {
        const response = await authFetchRaw(logUrl(0))

        if (!response.ok) {
            throw new Error(await response.text())
        }

        lines.value = parseLog(await response.text())
        next.value = Number(response.headers.get('x-log-next') ?? 0)
        toEnd()
    } catch (e) {
        lines.value = []
        next.value = 0
        indexStore.msgAlert('error', e instanceof Error ? e.message : String(e), 5)
    } finally {
        loading.value = false
    }
}

async function poll() {
    now.value = Date.now()

    if (!liveActive.value || loading.value || document.hidden || !channel.value) {
        return
    }

    try {
        const response = await authFetchRaw(logUrl(next.value))

        if (!response.ok) {
            return
        }

        const text = await response.text()
        const offset = Number(response.headers.get('x-log-next') ?? 0)

        if (offset < next.value) {
            // The file started over (midnight): read it again.
            await load()
            return
        }

        if (text) {
            const follow = atBottom()
            const lastId = lines.value[lines.value.length - 1]?.id ?? -1

            lines.value = [...lines.value, ...parseLog(text, lastId + 1)]

            if (follow) {
                toEnd()
            }
        }

        next.value = offset
    } catch {
        // Try again on the next tick.
    }
}

function shiftDay(days: number) {
    const target = dayjs(day.value).add(days, 'day').format('YYYY-MM-DD')

    if (target <= today()) {
        day.value = target
    }
}

async function download() {
    const response = await authFetchRaw(logUrl(0, true))

    if (!response.ok) {
        indexStore.msgAlert('error', await response.text(), 7)
        return
    }

    const url = URL.createObjectURL(await response.blob())
    const link = document.createElement('a')

    link.href = url
    link.download = `noar_${channel.value?.id}${isToday.value ? '' : `_${day.value}`}.log`
    document.body.appendChild(link)
    link.click()
    document.body.removeChild(link)
    URL.revokeObjectURL(url)
}

async function copyLine(line: LogLine) {
    try {
        await navigator.clipboard.writeText(`[${line.stamp}] [${line.level ?? ''}] ${line.plain}`)
        indexStore.msgAlert('success', t('log.copied'), 2)
    } catch {
        // Clipboard access can be refused; nothing else to do.
    }
}

/** The storage folder a line points at, when it names a file in the storage. */
function storageFolder(line: LogLine): string | null {
    const root = channel.value?.storage?.replace(/\/+$/, '')

    if (!root) {
        return null
    }

    for (const part of line.parts) {
        if (part.kind === 'log-addr' && part.text.startsWith(`${root}/`)) {
            const relative = part.text.slice(root.length + 1)

            return relative.includes('/') ? relative.slice(0, relative.lastIndexOf('/')) : ''
        }
    }

    return null
}

function openRun(row: LogRow) {
    openRuns.value = new Set([...openRuns.value, row.line.id])
}

function rowClass(line: LogLine): string {
    if (line.level === 'ERROR' || line.level === 'FATAL') {
        return 'bg-on-air-soft'
    }

    return line.level === 'WARN' ? 'bg-accent-soft' : ''
}

function badgeClass(line: LogLine): string {
    switch (line.level) {
        case 'ERROR':
        case 'FATAL':
            return 'bg-on-air text-white font-semibold'
        case 'WARN':
            return 'bg-[#e0b43c] text-[#17181a] font-semibold'
        case 'DEBUG':
            return 'bg-base-200 text-subtle'
        default:
            return 'bg-base-300 text-base-content/80'
    }
}

onMounted(() => {
    load()
    timer = setInterval(poll, LIVE_SECONDS * 1000)
})

onBeforeUnmount(() => clearInterval(timer))

watch([day, () => channel.value?.id], load)
watch(level, (value) => writeSetting('error_level', value))
watch(fold, (value) => writeSetting('log-fold', String(value)))
watch(live, (value) => {
    writeSetting('log-live', String(value))

    if (value) {
        poll()
    }
})
watch([level, search, fold], () => {
    shown.value = PAGE
    toEnd()
})
</script>
<template>
    <div class="max-w-[1440px] mx-auto px-4 sm:px-6 py-6 flex flex-col gap-4">
        <div class="flex flex-wrap items-end gap-x-4 gap-y-3">
            <div class="flex-[1_1_280px] min-w-0">
                <h1 class="m-0 text-2xl font-semibold">{{ t('log.title') }}</h1>
                <p class="mt-1 mb-0 text-muted">
                    {{ t('log.intro', { channel: channel?.name ?? '', timezone: configStore.timezone }) }}
                </p>
            </div>
            <div class="flex flex-wrap gap-2 text-[13px]">
                <button
                    type="button"
                    class="inline-flex items-center gap-2 rounded-full px-3 py-1.5 border-0 cursor-pointer"
                    :class="counts.errors ? 'bg-on-air-soft text-on-air-text' : 'bg-raised text-muted'"
                    @click="level = 'ERROR'"
                >
                    <span class="dot" :class="counts.errors ? 'bg-on-air' : 'bg-subtle'" aria-hidden="true" />
                    {{ t('log.errors', { count: counts.errors }, counts.errors) }}
                </button>
                <button
                    type="button"
                    class="inline-flex items-center gap-2 rounded-full px-3 py-1.5 border-0 cursor-pointer"
                    :class="counts.warnings ? 'bg-accent-soft text-accent-text' : 'bg-raised text-muted'"
                    @click="level = 'WARN'"
                >
                    <span class="dot" :class="counts.warnings ? 'bg-[#e0b43c]' : 'bg-subtle'" aria-hidden="true" />
                    {{ t('log.warnings', { count: counts.warnings }, counts.warnings) }}
                </button>
            </div>
        </div>

        <section :aria-label="t('log.filters')" class="card-box px-3.5 py-3 flex flex-wrap items-center gap-2.5">
            <div class="inline-flex items-center">
                <button
                    type="button"
                    class="btn-quiet rounded-e-none px-0 min-w-10"
                    :aria-label="t('log.previousDay')"
                    @click="shiftDay(-1)"
                >
                    <i class="bi bi-chevron-left" aria-hidden="true" />
                </button>
                <label class="relative">
                    <span class="sr-only">{{ t('log.day') }}</span>
                    <input
                        v-model="day"
                        type="date"
                        :max="today()"
                        required
                        class="field-input rounded-none border-x-0 font-mono w-auto min-w-40 bg-raised"
                    />
                </label>
                <button
                    type="button"
                    class="btn-quiet rounded-s-none px-0 min-w-10"
                    :aria-label="t('log.nextDay')"
                    :disabled="isToday"
                    @click="shiftDay(1)"
                >
                    <i class="bi bi-chevron-right" aria-hidden="true" />
                </button>
            </div>
            <button v-if="!isToday" type="button" class="btn-quiet" @click="day = today()">
                {{ t('log.today') }}
            </button>

            <div
                role="radiogroup"
                :aria-label="t('log.level')"
                class="inline-flex flex-wrap bg-base-200 border border-line-strong rounded-field p-[3px]"
            >
                <label
                    v-for="option in LEVELS"
                    :key="option"
                    class="inline-flex items-center px-3 min-h-8.5 rounded-md text-[13px] cursor-pointer focus-within:outline-2 focus-within:outline-accent"
                    :class="level === option ? 'bg-base-300 text-base-content' : 'text-muted'"
                >
                    <input v-model="level" type="radio" name="log-level" :value="option" class="sr-only" />
                    {{ t(`log.levels.${option}`) }}
                </label>
            </div>

            <label
                class="flex items-center gap-2 bg-base-200 border border-line-strong rounded-field px-3 min-h-10 text-muted flex-[1_1_220px] min-w-0"
            >
                <i class="bi bi-search text-xs" aria-hidden="true" />
                <input
                    v-model="search"
                    type="search"
                    :placeholder="t('log.search')"
                    :aria-label="t('log.search')"
                    class="bg-transparent border-0 text-base-content flex-1 min-w-0 outline-none"
                />
            </label>

            <label class="flex items-center gap-2.5 min-h-10 text-[13px] cursor-pointer">
                <input v-model="fold" type="checkbox" role="switch" class="toggle toggle-sm toggle-accent" />
                {{ t('log.fold') }}
            </label>
            <label class="flex items-center gap-2.5 min-h-10 text-[13px] cursor-pointer">
                <input
                    v-model="live"
                    type="checkbox"
                    role="switch"
                    class="toggle toggle-sm toggle-accent"
                    :disabled="!isToday"
                />
                <span class="inline-flex items-center gap-1.5">
                    <span class="dot" :class="liveActive ? 'bg-ok' : 'bg-subtle'" aria-hidden="true" />
                    {{ t('log.live') }}
                </span>
            </label>
            <button type="button" class="btn-quiet" @click="download">
                <i class="bi bi-download" aria-hidden="true" />{{ t('log.downloadShort') }}
            </button>
        </section>

        <section :aria-label="t('log.lines')" class="card-box overflow-hidden">
            <div ref="scroller" class="relative overflow-auto h-[max(420px,calc(100vh-330px))]">
                <div v-if="loading" class="absolute inset-0 z-10 flex justify-center pt-10 bg-base-100/60">
                    <span class="loading loading-spinner loading-lg" />
                </div>
                <table class="w-full table-fixed border-collapse text-[13px]">
                    <thead class="sticky top-0 z-[1] bg-base-200">
                        <tr class="text-left text-muted">
                            <th class="ps-3 sm:ps-4 pe-2 sm:pe-3 py-2.5 w-[76px] sm:w-24 font-medium">
                                {{ t('log.time') }}
                            </th>
                            <th class="px-2 sm:px-3 py-2.5 w-[68px] sm:w-20 font-medium">{{ t('log.levelColumn') }}</th>
                            <th class="ps-3 pe-4 py-2.5 font-medium">{{ t('log.message') }}</th>
                        </tr>
                    </thead>
                    <tbody class="font-mono">
                        <tr v-if="hiddenRows">
                            <td colspan="3" class="px-4 py-2 border-t border-raised">
                                <button type="button" class="link-accent font-sans" @click="shown += PAGE">
                                    {{ t('log.earlier', { count: Math.min(hiddenRows, PAGE) }) }}
                                </button>
                            </td>
                        </tr>
                        <template v-for="row in drawnRows" :key="row.folded ? `f${row.line.id}` : row.line.id">
                            <tr v-if="row.folded" class="border-t border-raised bg-base-200/60">
                                <td />
                                <td />
                                <td class="ps-3 pe-4 py-1.5">
                                    <button type="button" class="link-accent font-sans min-h-7" @click="openRun(row)">
                                        {{
                                            t(
                                                'log.folded',
                                                {
                                                    count: row.folded.lines.length,
                                                    first: row.folded.first,
                                                    last: row.folded.last,
                                                },
                                                row.folded.lines.length,
                                            )
                                        }}
                                    </button>
                                </td>
                            </tr>
                            <tr v-else class="border-t border-raised align-top" :class="rowClass(row.line)">
                                <td class="ps-3 sm:ps-4 pe-2 sm:pe-3 py-1.5 text-muted tabular" :title="row.line.stamp">
                                    {{ row.line.time }}
                                </td>
                                <td class="px-2 sm:px-3 py-1.5">
                                    <span
                                        v-if="row.line.level"
                                        class="inline-block text-[11px] rounded px-1.5 leading-5"
                                        :class="badgeClass(row.line)"
                                    >
                                        {{ t(`log.levelNames.${row.line.level}`) }}
                                    </span>
                                </td>
                                <td class="ps-3 pe-4 py-1.5 whitespace-pre-wrap break-words">
                                    <span
                                        v-for="(part, index) in row.line.parts"
                                        :key="index"
                                        :class="part.kind ?? ''"
                                        >{{ part.text }}</span
                                    >
                                    <div
                                        v-if="row.line.level === 'ERROR' || row.line.level === 'FATAL'"
                                        class="font-sans flex flex-wrap gap-3 mt-1"
                                    >
                                        <button
                                            v-if="storageFolder(row.line) !== null"
                                            type="button"
                                            class="link-accent text-on-air-text"
                                            @click="
                                                router.push({
                                                    path: '/media',
                                                    query: { path: storageFolder(row.line) },
                                                })
                                            "
                                        >
                                            {{ t('log.openStorage') }}
                                        </button>
                                        <button
                                            type="button"
                                            class="link-accent text-on-air-text"
                                            @click="copyLine(row.line)"
                                        >
                                            {{ t('log.copy') }}
                                        </button>
                                    </div>
                                </td>
                            </tr>
                        </template>
                    </tbody>
                </table>
                <p v-if="!loading && !lines.length" class="m-0 px-4 py-8 text-muted">{{ t('log.empty') }}</p>
                <p v-else-if="!loading && !rows.length" class="m-0 px-4 py-8 text-muted">{{ t('log.noMatch') }}</p>
            </div>
            <div
                class="flex flex-wrap items-center gap-x-3.5 gap-y-2 px-4 py-2.5 border-t border-line text-[13px] text-muted"
            >
                <span v-if="liveActive" class="inline-flex items-center gap-2" aria-live="polite">
                    <span class="dot bg-ok" aria-hidden="true" />
                    {{ lastLineAge ? t('log.liveStatus', { age: lastLineAge }) : t('log.liveIdle') }}
                </span>
                <span v-else>{{ isToday ? t('log.paused') : day }}</span>
                <span class="font-mono truncate">noar_{{ channel?.id }}{{ isToday ? '' : `_${day}` }}.log</span>
                <div class="flex-auto" />
                <button type="button" class="btn-quiet min-h-8.5 text-[13px]" @click="toEnd">
                    {{ t('log.toEnd') }}
                </button>
            </div>
        </section>
    </div>
</template>
<style>
.log-gray {
    color: var(--subtle);
}

.log-number {
    color: var(--accent-text);
}

.log-addr {
    color: var(--live);
    font-weight: 500;
}

.log-cmd {
    color: var(--after-clip);
}

.log-decoder,
.log-encoder,
.log-server,
.log-validate {
    color: var(--after-clip);
}
</style>
