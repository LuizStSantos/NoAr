<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'

import { useConfig } from '@/stores/config'
import { useUpdates } from '@/stores/updates'

/** A new version is out: shown to global admins until they hide it. */
const { t, locale } = useI18n()
const configStore = useConfig()
const updates = useUpdates()

const latest = computed(() => updates.status?.latest ?? null)
const channelId = computed(() => configStore.channels[configStore.i]?.id)

const detail = computed(() => {
    const parts: string[] = []
    const published = latest.value?.published_at
    const count = latest.value?.change_count

    if (published) {
        const date = new Date(published)

        parts.push(
            t('settings.updates.noticeDetail', {
                when: t('settings.updates.dateTime', {
                    date: date.toLocaleDateString(locale.value, { day: '2-digit', month: '2-digit' }),
                    time: date.toLocaleTimeString(locale.value, { hour: '2-digit', minute: '2-digit' }),
                }),
            }),
        )
    }

    if (count) {
        parts.push(t('settings.updates.noticeChanges', { count }, count))
    }

    return parts.join(' · ')
})
</script>
<template>
    <div
        v-if="updates.showNotice"
        role="status"
        class="bg-accent-soft border border-accent/60 rounded-xl px-4 py-3 flex flex-wrap items-center gap-x-3.5 gap-y-2.5"
    >
        <i class="bi bi-download text-accent-text" aria-hidden="true" />
        <span class="flex-[1_1_400px] min-w-0">
            <span class="font-semibold">{{ t('settings.updates.notice') }}</span>
            <span v-if="detail" class="text-base-content/80"> · {{ detail }}</span>
        </span>
        <RouterLink :to="{ name: 'settings-updates', query: { channel: channelId } }" class="btn-main">
            {{ t('settings.updates.noticeOpen') }}
        </RouterLink>
        <button
            type="button"
            class="btn-icon"
            :aria-label="t('settings.updates.noticeHide')"
            :title="t('settings.updates.noticeHide')"
            @click="updates.dismiss()"
        >
            <i class="bi bi-x-lg" aria-hidden="true" />
        </button>
    </div>
</template>
