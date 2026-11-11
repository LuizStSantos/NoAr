<script setup lang="ts">
import dayjs from 'dayjs'
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { watchDebounced } from '@vueuse/core'

import { authFetch } from '@/composables/authFetch'
import { save } from '@/composables/reportExport'
import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'

const { t, te, locale } = useI18n()
const configStore = useConfig()
const indexStore = useIndex()

/** The groups of actions, as the server names them (`playlist.save`). */
const GROUPS = [
    'auth',
    'player',
    'playlist',
    'schedule',
    'graphics',
    'storage',
    'settings',
    'user',
    'system',
    'external',
] as const
const PAGE = 200

const filters = ref({
    from: dayjs().subtract(6, 'day').format('YYYY-MM-DD'),
    to: dayjs().format('YYYY-MM-DD'),
    user: '',
    channel: '',
    group: '',
    q: '',
})
const entries = ref<ActivityEntry[]>([])
const more = ref(false)
const loading = ref(false)
const usernames = ref<string[]>([])
/** Only the newest request may fill the list. */
let version = 0

function query(before?: number): string {
    const params = new URLSearchParams({ limit: String(PAGE) })
    const { from, to, user, channel, group, q } = filters.value

    if (from) params.set('from', String(dayjs(from).startOf('day').unix()))
    if (to) params.set('to', String(dayjs(to).endOf('day').unix()))
    if (user) params.set('user', user)
    if (channel) params.set('channel', channel)
    if (group) params.set('action', group)
    if (q.trim()) params.set('q', q.trim())
    if (before !== undefined) params.set('before', String(before))

    return params.toString()
}

async function load(older = false) {
    const current = ++version
    const before = older ? entries.value.at(-1)?.id : undefined

    loading.value = true
    try {
        const page = await authFetch<ActivityPage>(`/api/activity?${query(before)}`)

        if (current !== version) return
        entries.value = older ? [...entries.value, ...page.entries] : page.entries
        more.value = page.more
    } catch {
        if (current === version) indexStore.msgAlert('error', t('settings.activity.loadFailed'), 4)
    } finally {
        if (current === version) loading.value = false
    }
}

watchDebounced(filters, () => load(), { debounce: 300, deep: true })

onMounted(async () => {
    load()
    try {
        const users = await authFetch<User[]>('/api/users')
        usernames.value = users.map((user) => user.username).sort((a, b) => a.localeCompare(b))
    } catch {
        // The filter then lists no names; the history still loads.
    }
})

function when(entry: ActivityEntry): string {
    return new Date(entry.at * 1000).toLocaleString(locale.value, { dateStyle: 'short', timeStyle: 'medium' })
}

function who(entry: ActivityEntry): string {
    if (entry.username) return entry.username
    return entry.action.startsWith('external.') ? t('settings.activity.external') : '—'
}

function channelName(entry: ActivityEntry): string {
    if (entry.channel_id === null) return '—'
    return configStore.channels.find((channel) => channel.id === entry.channel_id)?.name ?? `#${entry.channel_id}`
}

function what(entry: ActivityEntry): string {
    const key = `settings.activity.actions.${entry.action}`
    return te(key) ? t(key) : `${entry.method} ${entry.route}`
}

type Result = 'done' | 'refused' | 'blocked' | 'rejected' | 'failed'

function result(entry: ActivityEntry): Result {
    if (entry.status < 400) return 'done'
    if (entry.status === 403) return 'refused'
    if (entry.status === 429) return 'blocked'
    if (entry.status < 500) return 'rejected'
    return 'failed'
}

const RESULT_CLASS: Record<Result, string> = {
    done: 'text-ok',
    refused: 'text-error',
    blocked: 'text-error',
    rejected: 'text-warning',
    failed: 'text-error',
}

const countLabel = computed(
    () =>
        t('settings.activity.count', { n: entries.value.length }, entries.value.length) +
        (more.value ? ` · ${t('settings.activity.moreOlder')}` : ''),
)

/** The list on screen as a spreadsheet, in the style of the Report's. */
function exportCsv() {
    const separator = locale.value.startsWith('pt') ? ';' : ','
    const cell = (value: string) => `"${value.replace(/"/g, '""')}"`
    const header = ['when', 'who', 'role', 'channel', 'what', 'detail', 'result', 'ip'].map((column) =>
        t(`settings.activity.columns.${column}`),
    )
    const lines = entries.value.map((entry) => [
        dayjs.unix(entry.at).format('YYYY-MM-DD HH:mm:ss'),
        who(entry),
        entry.role,
        channelName(entry),
        what(entry),
        entry.target,
        `${t(`settings.activity.results.${result(entry)}`)} (${entry.status})`,
        entry.ip,
    ])
    const text = [header, ...lines].map((line) => line.map(cell).join(separator)).join('\r\n')

    save(
        new Blob(['﻿', text, '\r\n'], { type: 'text/csv;charset=utf-8' }),
        `noar-activity-${filters.value.from}-${filters.value.to}.csv`,
    )
}
</script>
<template>
    <div>
        <h1 class="m-0 text-xl font-semibold">{{ t('settings.sections.activity') }}</h1>
        <p class="mt-1 mb-0 text-muted">{{ t('settings.activity.intro') }}</p>
    </div>

    <section
        :aria-label="t('settings.activity.filters')"
        class="card-box px-5 py-4.5 grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3.5"
    >
        <label class="field-label">
            <span id="activity-from">{{ t('settings.activity.from') }}</span>
            <input
                v-model="filters.from"
                aria-labelledby="activity-from"
                type="date"
                class="field-input"
                :max="filters.to || undefined"
            />
        </label>
        <label class="field-label">
            <span id="activity-to">{{ t('settings.activity.to') }}</span>
            <input
                v-model="filters.to"
                aria-labelledby="activity-to"
                type="date"
                class="field-input"
                :min="filters.from || undefined"
            />
        </label>
        <label class="field-label">
            <span id="activity-user">{{ t('settings.activity.user') }}</span>
            <select v-model="filters.user" aria-labelledby="activity-user" class="field-input">
                <option value="">{{ t('settings.activity.allUsers') }}</option>
                <option v-for="name in usernames" :key="name" :value="name">{{ name }}</option>
            </select>
        </label>
        <label class="field-label">
            <span id="activity-channel">{{ t('settings.activity.channel') }}</span>
            <select v-model="filters.channel" aria-labelledby="activity-channel" class="field-input">
                <option value="">{{ t('settings.activity.allChannels') }}</option>
                <option v-for="channel in configStore.channels" :key="channel.id" :value="String(channel.id)">
                    {{ channel.name }}
                </option>
            </select>
        </label>
        <label class="field-label">
            <span id="activity-type">{{ t('settings.activity.type') }}</span>
            <select v-model="filters.group" aria-labelledby="activity-type" class="field-input">
                <option value="">{{ t('settings.activity.allTypes') }}</option>
                <option v-for="group in GROUPS" :key="group" :value="group">
                    {{ t(`settings.activity.groups.${group}`) }}
                </option>
            </select>
        </label>
        <label class="field-label">
            <span id="activity-search">{{ t('settings.activity.search') }}</span>
            <input
                v-model="filters.q"
                aria-labelledby="activity-search"
                type="search"
                class="field-input"
                :placeholder="t('settings.activity.searchHint')"
            />
        </label>
    </section>

    <div class="flex flex-wrap items-center gap-3">
        <p class="m-0 text-sm text-muted flex-auto" aria-live="polite">
            {{ loading ? t('settings.activity.loading') : countLabel }}
        </p>
        <button type="button" class="btn-quiet" :disabled="!entries.length" @click="exportCsv">
            <i class="bi bi-filetype-csv" aria-hidden="true" />{{ t('settings.activity.export') }}
        </button>
    </div>

    <section :aria-label="t('settings.sections.activity')" class="card-box relative overflow-x-auto">
        <table class="w-full min-w-[820px] border-collapse text-[14px]">
            <caption class="sr-only">
                {{
                    t('settings.sections.activity')
                }}
            </caption>
            <thead>
                <tr class="text-muted text-left text-xs">
                    <th scope="col" class="px-4.5 py-3 font-medium">{{ t('settings.activity.columns.when') }}</th>
                    <th scope="col" class="px-2 py-3 font-medium">{{ t('settings.activity.columns.who') }}</th>
                    <th scope="col" class="px-2 py-3 font-medium">{{ t('settings.activity.columns.channel') }}</th>
                    <th scope="col" class="px-2 py-3 font-medium">{{ t('settings.activity.columns.what') }}</th>
                    <th scope="col" class="px-2 py-3 font-medium">{{ t('settings.activity.columns.detail') }}</th>
                    <th scope="col" class="px-4.5 py-3 font-medium">{{ t('settings.activity.columns.result') }}</th>
                </tr>
            </thead>
            <tbody>
                <tr v-for="entry in entries" :key="entry.id" class="border-t border-line align-top">
                    <td class="px-4.5 py-2.5 whitespace-nowrap tabular-nums">{{ when(entry) }}</td>
                    <td class="px-2 py-2.5">
                        <div class="font-semibold">{{ who(entry) }}</div>
                        <div v-if="entry.ip" class="text-[13px] text-muted font-mono">{{ entry.ip }}</div>
                    </td>
                    <td class="px-2 py-2.5 text-muted">{{ channelName(entry) }}</td>
                    <td class="px-2 py-2.5">{{ what(entry) }}</td>
                    <td class="px-2 py-2.5 text-muted break-words max-w-[320px]">{{ entry.target || '—' }}</td>
                    <td class="px-4.5 py-2.5 whitespace-nowrap" :class="RESULT_CLASS[result(entry)]">
                        {{ t(`settings.activity.results.${result(entry)}`) }}
                    </td>
                </tr>
                <tr v-if="!entries.length && !loading">
                    <td colspan="6" class="px-4.5 py-8 text-muted">{{ t('settings.activity.empty') }}</td>
                </tr>
            </tbody>
        </table>
    </section>

    <button v-if="more" type="button" class="btn-quiet self-start" :disabled="loading" @click="load(true)">
        {{ t('settings.activity.older') }}
    </button>
</template>
