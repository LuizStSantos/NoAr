<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useHead } from '@unhead/vue'
import { RouterView, useRoute, useRouter } from 'vue-router'

import PlayoutCard from '@/components/settings/PlayoutCard.vue'
import GenericModal from '@/components/utils/GenericModal.vue'
import { useAuth } from '@/stores/auth'
import { useConfig } from '@/stores/config'
import { useSettings } from '@/stores/settings'

/**
 * The channel's graphics: what is on air now, the logo, the L-shape, the
 * subtitles, quick scenes and control from outside NoAr.
 */
const PAGES = [
    { name: 'live', icon: 'bi-broadcast', admins: false },
    { name: 'logo', icon: 'bi-badge-tm', admins: true },
    { name: 'lshape', icon: 'bi-aspect-ratio', admins: true },
    { name: 'subtitles', icon: 'bi-badge-cc', admins: true },
    { name: 'scenes', icon: 'bi-collection-play', admins: true },
    { name: 'api', icon: 'bi-plug', admins: true },
] as const

type PageName = (typeof PAGES)[number]['name']

const { t } = useI18n()
const route = useRoute()
const router = useRouter()
const authStore = useAuth()
const configStore = useConfig()
const settings = useSettings()

const pendingChannel = ref<number | null>(null)

const channelId = computed(() => configStore.channels[configStore.i]?.id)
const current = computed(() => String(route.name ?? '').replace('graphics-', '') as PageName)
const pages = computed(() =>
    PAGES.filter((page) => !page.admins || ['global_admin', 'channel_admin'].includes(authStore.role)),
)

useHead({
    title: computed(() => `${t(`graphics.sections.${current.value}`)} · ${t('nav.graphics')}`),
})

function link(name: PageName) {
    return { name: `graphics-${name}`, query: { channel: channelId.value } }
}

function selectPage(event: Event) {
    router.push(link((event.target as HTMLSelectElement).value as PageName))
}

function changeChannel(event: Event) {
    const id = Number((event.target as HTMLSelectElement).value)

    if (settings.changedSections.length) {
        pendingChannel.value = id
        ;(event.target as HTMLSelectElement).value = String(channelId.value)
        return
    }

    router.push({ path: route.path, query: { ...route.query, channel: id } })
}

function confirmChannel(discard: boolean) {
    const id = pendingChannel.value
    pendingChannel.value = null

    if (discard && id) {
        for (const name of settings.changedSections) {
            settings.discardSection(name)
        }

        router.push({ path: route.path, query: { ...route.query, channel: id } })
    }
}
</script>
<template>
    <div class="max-w-[1440px] mx-auto px-4 sm:px-6 py-6 flex flex-wrap gap-6 items-start">
        <aside
            :aria-label="t('graphics.sectionsLabel')"
            class="flex-[1_1_240px] w-full lg:max-w-[280px] flex flex-col gap-4 lg:sticky lg:top-16"
        >
            <label class="field-label">
                {{ t('nav.channel') }}
                <select
                    class="field-input bg-base-100!"
                    :value="channelId"
                    :disabled="configStore.channels.length < 2"
                    @change="changeChannel"
                >
                    <option v-for="channel in configStore.channels" :key="channel.id" :value="channel.id">
                        {{ channel.name }}
                    </option>
                </select>
            </label>

            <PlayoutCard />

            <label class="field-label lg:hidden">
                {{ t('graphics.page') }}
                <select class="field-input bg-base-100!" :value="current" @change="selectPage">
                    <option v-for="page in pages" :key="page.name" :value="page.name">
                        {{ t(`graphics.sections.${page.name}`) }}
                    </option>
                </select>
            </label>

            <nav class="hidden lg:flex flex-col gap-0.5">
                <div class="text-[11px] text-subtle tracking-[0.08em] uppercase px-2.5 pt-1.5 pb-1.5">
                    {{ t('nav.graphics') }}
                </div>
                <RouterLink
                    v-for="page in pages"
                    :key="page.name"
                    :to="link(page.name)"
                    :aria-current="current === page.name ? 'page' : undefined"
                    class="flex items-center gap-2.5 px-2.5 py-2 rounded-field"
                    :class="
                        current === page.name
                            ? 'bg-base-300 text-base-content'
                            : 'text-muted hover:bg-raised hover:text-base-content'
                    "
                >
                    <i class="bi" :class="page.icon" aria-hidden="true" />
                    <span class="flex-auto">{{ t(`graphics.sections.${page.name}`) }}</span>
                    <span
                        v-if="page.admins && settings.changedSections.includes('logo')"
                        class="dot bg-accent"
                        :title="t('settings.hasChanges')"
                    />
                </RouterLink>
                <p class="m-0 mt-3 px-2.5 text-xs text-muted leading-relaxed">{{ t('graphics.marksHint') }}</p>
            </nav>
        </aside>

        <main class="flex-[999_1_640px] min-w-0 flex flex-col gap-4">
            <RouterView :key="channelId" />
        </main>

        <GenericModal
            :show="pendingChannel !== null"
            :title="t('settings.unsavedTitle')"
            :ok-label="t('settings.discardAndSwitch')"
            danger
            :modal-action="confirmChannel"
        >
            <p class="m-0 max-w-sm">{{ t('settings.unsavedSwitch') }}</p>
        </GenericModal>
    </div>
</template>
