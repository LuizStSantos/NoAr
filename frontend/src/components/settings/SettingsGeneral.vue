<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'
import { cloneDeep } from 'es-toolkit/object'

import SaveBar from '@/components/settings/SaveBar.vue'
import SectionCard from '@/components/settings/SectionCard.vue'
import GenericModal from '@/components/utils/GenericModal.vue'
import { authFetch } from '@/composables/authFetch'
import { useAuth } from '@/stores/auth'
import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'
import { useMedia } from '@/stores/media'
import { usePlaylist } from '@/stores/playlist'
import { useSettings } from '@/stores/settings'

const { t } = useI18n()
const route = useRoute()
const router = useRouter()
const authStore = useAuth()
const configStore = useConfig()
const indexStore = useIndex()
const mediaStore = useMedia()
const playlistStore = usePlaylist()
const settings = useSettings()

const saving = ref(false)
const showNew = ref(false)
const newName = ref('')
const showDelete = ref(false)

const draft = computed(() => settings.channelDraft)
const canEdit = computed(() => authStore.role !== 'user')
const isGlobal = computed(() => authStore.role === 'global_admin')
const changes = computed(() => settings.sectionChanges('general'))

const extensions = computed({
    get: () => {
        const value = draft.value?.extra_extensions ?? ''

        return (Array.isArray(value) ? value : value.split(','))
            .map((item) => item.trim())
            .filter(Boolean)
            .join(', ')
    },
    set: (value: string) => {
        if (draft.value) {
            draft.value.extra_extensions = value
                .split(/[,;\s]+/)
                .map((item) => item.trim().replace(/^\./, ''))
                .filter(Boolean)
                .join(',')
        }
    },
})

let zoneCache: { zone: string; label: string }[] | null = null

const zones = computed(() => {
    if (!zoneCache) {
        const now = new Date()

        zoneCache = Intl.supportedValuesOf('timeZone').map((zone) => {
            let offset = ''

            try {
                offset =
                    new Intl.DateTimeFormat('en', { timeZone: zone, timeZoneName: 'shortOffset' })
                        .formatToParts(now)
                        .find((part) => part.type === 'timeZoneName')
                        ?.value.replace('GMT', 'UTC')
                        .replace('-', '−') ?? ''
            } catch {
                // Some engines know zones they cannot format.
            }

            return { zone, label: offset ? `${zone} (${offset === 'UTC' ? 'UTC' : offset})` : zone }
        })
    }

    return zoneCache
})

async function save() {
    const channel = draft.value

    if (!channel) {
        return
    }

    saving.value = true

    try {
        const oldTimezone = configStore.timezone

        await authFetch(`/api/channel/${channel.id}`, {
            method: 'PATCH',
            headers: { ...configStore.contentType, ...authStore.authHeader },
            body: JSON.stringify(channel),
        })

        configStore.channels[configStore.i] = cloneDeep(channel)
        configStore.timezone = channel.timezone || 'UTC'

        if (oldTimezone !== configStore.timezone && playlistStore.playoutIsRunning) {
            settings.restartPending = true
        }

        await configStore.getPlayoutConfig()
        indexStore.msgAlert('success', t('settings.saved'), 2)
    } catch (e: any) {
        indexStore.msgAlert('error', e?.data ?? t('config.updateChannelFailed'), 4)
    } finally {
        saving.value = false
    }
}

function withoutId(path: string) {
    return path.replace(/\/\d+\/?$/, '')
}

async function addChannel(confirm: boolean) {
    showNew.value = false

    const base = configStore.channels[0]

    if (!confirm || !base || !newName.value.trim()) {
        newName.value = ''
        return
    }

    const id = Math.max(...configStore.channels.map((channel) => channel.id)) + 1
    const channel = {
        name: newName.value.trim(),
        preview_url: `${window.location.protocol}//${window.location.host}/public/${id}/live/stream.m3u8`,
        extra_extensions: base.extra_extensions,
        active: false,
        public: `${withoutId(base.public)}/${id}`,
        playlists: `${withoutId(base.playlists)}/${id}`,
        storage: `${withoutId(base.storage)}/${id}`,
        last_date: null,
        time_shift: 0,
        timezone: base.timezone ?? configStore.timezone,
    }

    newName.value = ''

    try {
        const created = await authFetch<Channel>('/api/channel', {
            method: 'POST',
            headers: { ...configStore.contentType, ...authStore.authHeader },
            body: JSON.stringify(channel),
        })

        await configStore.getChannelConfig()
        await router.replace({ path: route.path, query: { ...route.query, channel: created.id } })
        indexStore.msgAlert('success', t('settings.channelCreated', { name: created.name }), 2)
    } catch (e: any) {
        indexStore.msgAlert('error', e?.data ?? t('config.updateChannelFailed'), 4)
    }
}

async function deleteChannel(confirm: boolean) {
    showDelete.value = false

    const id = draft.value?.id

    if (!confirm || !id || id === configStore.channels[0]?.id) {
        return
    }

    try {
        await authFetch(`/api/channel/${id}`, { method: 'DELETE', headers: authStore.authHeader })
        await configStore.getChannelConfig()

        const first = configStore.channels[0]?.id
        await router.replace({ path: route.path, query: { ...route.query, channel: first } })
        await mediaStore.getTree('')
        indexStore.msgAlert('success', t('config.deleteChannelSuccess'), 2)
    } catch {
        indexStore.msgAlert('error', t('config.deleteChannelFailed'), 3)
    }
}

function channelStatus(channel: Channel & { active?: boolean }) {
    const running =
        channel.id === configStore.channels[configStore.i]?.id ? playlistStore.playoutIsRunning : channel.active

    return running
}
</script>
<template>
    <template v-if="draft">
        <div>
            <h1 class="m-0 text-xl font-semibold">{{ t('settings.sections.general') }}</h1>
            <p class="mt-1 mb-0 text-muted">{{ t('settings.general.intro', { name: draft.name }) }}</p>
        </div>

        <SectionCard :title="t('settings.general.channel')">
            <div class="grid grid-cols-1 md:grid-cols-2 gap-3.5">
                <label class="field-label">
                    {{ t('settings.general.name') }}
                    <input v-model="draft.name" class="field-input" :disabled="!canEdit" />
                </label>
                <label class="field-label">
                    {{ t('settings.general.timezone') }}
                    <select v-model="draft.timezone" class="field-input" :disabled="!canEdit">
                        <option v-for="zone in zones" :key="zone.zone" :value="zone.zone">{{ zone.label }}</option>
                    </select>
                    <span class="field-hint">{{ t('settings.general.timezoneHint') }}</span>
                </label>
                <label class="field-label">
                    {{ t('settings.general.previewUrl') }}
                    <input
                        v-model="draft.preview_url"
                        class="field-input font-mono text-[13px]"
                        :disabled="!canEdit"
                        spellcheck="false"
                    />
                    <span class="field-hint">{{ t('settings.general.previewUrlHint') }}</span>
                </label>
                <label class="field-label">
                    {{ t('settings.general.extensions') }}
                    <input v-model.lazy="extensions" class="field-input" :disabled="!canEdit" />
                    <span class="field-hint">{{ t('settings.general.extensionsHint') }}</span>
                </label>
            </div>
        </SectionCard>

        <SectionCard v-if="canEdit" :title="t('settings.general.folders')">
            <template #aside>
                <span class="text-xs bg-raised text-muted rounded-md px-2 py-0.5">{{
                    t('settings.globalAdminOnly')
                }}</span>
            </template>
            <p v-if="configStore.playout.storage?.shared_storage" class="m-0 text-[13px] text-accent-text">
                <i class="bi bi-exclamation-triangle me-1" aria-hidden="true" />{{ t('config.sharedStorage') }}
            </p>
            <div class="grid grid-cols-1 md:grid-cols-3 gap-3.5">
                <label class="field-label">
                    {{ t('settings.general.storage') }}
                    <input
                        v-model="draft.storage"
                        class="field-input font-mono text-[13px]"
                        :disabled="!isGlobal"
                        spellcheck="false"
                    />
                </label>
                <label class="field-label">
                    {{ t('settings.general.playlists') }}
                    <input
                        v-model="draft.playlists"
                        class="field-input font-mono text-[13px]"
                        :disabled="!isGlobal"
                        spellcheck="false"
                    />
                </label>
                <label class="field-label">
                    {{ t('settings.general.public') }}
                    <input
                        v-model="draft.public"
                        class="field-input font-mono text-[13px]"
                        :disabled="!isGlobal"
                        spellcheck="false"
                    />
                </label>
            </div>
        </SectionCard>

        <SectionCard v-if="isGlobal" :title="t('settings.general.channels')">
            <template #aside>
                <button class="btn-quiet" @click="showNew = true">+ {{ t('settings.general.newChannel') }}</button>
            </template>
            <ul class="flex flex-col">
                <li
                    v-for="channel in configStore.channels"
                    :key="channel.id"
                    class="flex flex-wrap items-center gap-3 py-2.5 border-t border-line first:border-t-0"
                >
                    <RouterLink
                        :to="{ path: route.path, query: { ...route.query, channel: channel.id } }"
                        class="font-semibold flex-auto hover:text-accent-text"
                    >
                        {{ channel.name }}
                    </RouterLink>
                    <span class="text-xs text-muted">
                        {{ t('settings.general.channelNumber', { id: channel.id })
                        }}<template v-if="channel.id === draft.id"> · {{ t('settings.general.thisOne') }}</template>
                    </span>
                    <span
                        class="inline-flex items-center gap-1.5 text-[13px]"
                        :class="channelStatus(channel) ? 'text-on-air-text' : 'text-muted'"
                    >
                        <span
                            class="dot"
                            :class="channelStatus(channel) ? 'bg-[#ff5a5f]' : 'bg-subtle'"
                            aria-hidden="true"
                        />
                        {{ channelStatus(channel) ? t('settings.general.onAir') : t('settings.general.stopped') }}
                    </span>
                </li>
            </ul>
        </SectionCard>

        <SectionCard v-if="isGlobal" danger>
            <div class="flex flex-wrap items-center gap-x-5 gap-y-3">
                <div class="flex-[1_1_320px]">
                    <h2 class="m-0 text-base font-semibold">{{ t('settings.general.deleteTitle') }}</h2>
                    <p class="mt-0.5 mb-0 text-muted text-[13px]">{{ t('settings.general.deleteHint') }}</p>
                </div>
                <button
                    class="btn-quiet border-[#5a2a2d]! text-[#ff9da0]!"
                    :disabled="draft.id === configStore.channels[0]?.id"
                    @click="showDelete = true"
                >
                    {{ t('settings.general.deleteButton') }}
                </button>
            </div>
        </SectionCard>

        <SaveBar
            v-if="canEdit"
            :changes="changes"
            :saving="saving"
            @save="save"
            @discard="settings.discardSection('general')"
        />

        <GenericModal
            :show="showNew"
            :title="t('settings.general.newChannel')"
            :ok-label="t('settings.general.create')"
            :ok-disabled="!newName.trim()"
            :modal-action="addChannel"
        >
            <div class="flex flex-col gap-3 w-[min(380px,85vw)]">
                <label class="field-label">
                    {{ t('settings.general.name') }}
                    <input v-model="newName" class="field-input" @keydown.enter="addChannel(true)" />
                </label>
                <p class="m-0 text-[13px] text-muted">{{ t('settings.general.newChannelHint') }}</p>
            </div>
        </GenericModal>

        <GenericModal
            :show="showDelete"
            :title="t('settings.general.deleteTitle')"
            :ok-label="t('settings.general.deleteButton')"
            danger
            :modal-action="deleteChannel"
        >
            <p class="m-0 max-w-sm">{{ t('settings.general.deleteQuestion', { name: draft.name }) }}</p>
        </GenericModal>
    </template>
</template>
