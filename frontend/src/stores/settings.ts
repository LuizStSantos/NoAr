import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import { cloneDeep } from 'es-toolkit/object'

import { authFetch } from '@/composables/authFetch'
import { useAuth } from '@/stores/auth'
import { countChanges } from '@/composables/diff'
import { useConfig } from '@/stores/config'

export type SettingsSectionName =
    | 'general'
    | 'output'
    | 'ingest'
    | 'playlist'
    | 'logo'
    | 'audio'
    | 'recording'
    | 'alerts'
    | 'advanced'
    | 'users'
    | 'server'
    | 'updates'

export interface SettingsSection {
    name: SettingsSectionName
    group: 'channel' | 'system'
    roles: string[]
    /** Parts of the channel configuration this page edits. */
    keys: string[]
    /** Saving these parts restarts the playout. */
    restarts?: boolean
}

const ALL = ['global_admin', 'channel_admin', 'user']
const ADMINS = ['global_admin', 'channel_admin']
const PROCESSING_LOGO = [
    'add_logo',
    'logo',
    'logo_scale',
    'logo_opacity',
    'logo_position',
    'vtt_enable',
    'vtt_dummy',
    'vtt_name',
    'vtt_language',
    'vtt_default',
].map((key) => `processing.${key}`)

export const SETTINGS_SECTIONS: SettingsSection[] = [
    { name: 'general', group: 'channel', roles: ALL, keys: [] },
    { name: 'output', group: 'channel', roles: ADMINS, keys: ['output'], restarts: true },
    { name: 'ingest', group: 'channel', roles: ADMINS, keys: ['ingest'] },
    {
        name: 'playlist',
        group: 'channel',
        roles: ADMINS,
        keys: ['playlist', 'storage', 'processing.mode'],
        restarts: true,
    },
    { name: 'logo', group: 'channel', roles: ADMINS, keys: PROCESSING_LOGO },
    { name: 'audio', group: 'channel', roles: ADMINS, keys: ['audio'] },
    { name: 'recording', group: 'channel', roles: ADMINS, keys: ['recording'] },
    { name: 'alerts', group: 'channel', roles: ADMINS, keys: ['mail', 'notification'] },
    { name: 'advanced', group: 'channel', roles: ADMINS, keys: ['general', 'logging', 'task'] },
    { name: 'users', group: 'system', roles: ALL, keys: [] },
    { name: 'server', group: 'system', roles: ['global_admin'], keys: [] },
    { name: 'updates', group: 'system', roles: ['global_admin'], keys: [] },
]

export function getPath(source: unknown, path: string): unknown {
    return path
        .split('.')
        .reduce<unknown>((value, key) => (value as Record<string, unknown> | undefined)?.[key], source)
}

function setPath(target: Record<string, unknown>, path: string, value: unknown) {
    const keys = path.split('.')
    const last = keys.pop() as string
    const parent = keys.reduce<Record<string, unknown>>((node, key) => node[key] as Record<string, unknown>, target)

    parent[last] = value
}

/** Shared state of the settings pages: what changed and how to save it. */
export const useSettings = defineStore('settings', () => {
    const configStore = useConfig()
    const authStore = useAuth()

    const channelDraft = ref<Channel | null>(null)
    const saving = ref(false)
    /** Saved changes that only take effect after a playout restart. */
    const restartPending = ref(false)

    const visibleSections = computed(() =>
        SETTINGS_SECTIONS.filter((section) => section.roles.includes(authStore.role)),
    )

    function sectionChanges(name: SettingsSectionName): number {
        if (name === 'general') {
            const saved = configStore.channels[configStore.i]

            return channelDraft.value && saved && channelDraft.value.id === saved.id
                ? countChanges(channelDraft.value, saved)
                : 0
        }

        const section = SETTINGS_SECTIONS.find((item) => item.name === name)

        if (!section || !configStore.playout?.output) {
            return 0
        }

        return section.keys.reduce(
            (sum, key) => sum + countChanges(getPath(configStore.playout, key), getPath(configStore.playoutSaved, key)),
            0,
        )
    }

    const changedSections = computed(() =>
        visibleSections.value.filter((section) => sectionChanges(section.name) > 0).map((section) => section.name),
    )

    function discardSection(name: SettingsSectionName) {
        if (name === 'general') {
            const saved = configStore.channels[configStore.i]
            channelDraft.value = saved ? cloneDeep(saved) : null
            return
        }

        const section = SETTINGS_SECTIONS.find((item) => item.name === name)

        for (const key of section?.keys ?? []) {
            setPath(
                configStore.playout as unknown as Record<string, unknown>,
                key,
                cloneDeep(getPath(configStore.playoutSaved, key)),
            )
        }
    }

    /** Save the whole channel configuration; every playout page shares it. */
    async function savePlayout(restartAfter = false): Promise<void> {
        saving.value = true

        try {
            const update = await configStore.setPlayoutConfig(configStore.playout)
            const id = configStore.channels[configStore.i]?.id
            const status = await authFetch<string>(`/api/control/${id}/process`, {
                method: 'POST',
                headers: { ...configStore.contentType, ...authStore.authHeader },
                body: JSON.stringify({ command: 'status' }),
            }).catch(() => '')

            await configStore.getPlayoutConfig()
            await configStore.getPlayoutOutputs()

            if (status === 'active' && update.requires_restart) {
                if (restartAfter) {
                    await configStore.restart(true)
                    restartPending.value = false
                } else {
                    restartPending.value = true
                }
            }
        } finally {
            saving.value = false
        }
    }

    async function restartPlayout() {
        await configStore.restart(true)
        restartPending.value = false
    }

    return {
        channelDraft,
        saving,
        restartPending,
        visibleSections,
        changedSections,
        sectionChanges,
        discardSection,
        savePlayout,
        restartPlayout,
    }
})
