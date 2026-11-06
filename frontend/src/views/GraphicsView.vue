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
 * Channel graphics are managed from On air and Templates. Assigned models
 * have dedicated editors rather than separate navigation pages.
 */
const PAGES = [
    { name: 'onair', icon: 'bi-record-circle', admins: false, group: 'main' },
    { name: 'templates', icon: 'bi-grid-1x2', admins: false, group: 'main' },
    { name: 'subtitles', icon: 'bi-badge-cc', admins: true, group: 'main' },
    { name: 'api', icon: 'bi-plug', admins: true, group: 'main' },
] as const

const GROUPS = ['main'] as const

type PageName = (typeof PAGES)[number]['name'] | 'composer'

const { t } = useI18n()
const route = useRoute()
const router = useRouter()
const authStore = useAuth()
const configStore = useConfig()
const settings = useSettings()

const pendingChannel = ref<number | null>(null)

const channelId = computed(() => configStore.channels[configStore.i]?.id)
const current = computed(() =>
    route.name === 'graphics-channel-editor'
        ? 'templates'
        : (String(route.name ?? '').replace('graphics-', '') as PageName),
)
const pages = computed(() =>
    PAGES.filter((page) => !page.admins || ['global_admin', 'channel_admin'].includes(authStore.role)),
)
const groupedPages = computed(() =>
    GROUPS.map((group) => ({ group, pages: pages.value.filter((page) => page.group === group) })).filter(
        (entry) => entry.pages.length,
    ),
)

useHead({
    title: computed(() => {
        const title =
            route.name === 'graphics-channel-editor'
                ? t('graphics.editor.title', {
                      name: t(`graphics.assigned.${route.params.kind === 'scene' ? 'scenes' : route.params.kind}`),
                  })
                : t(`graphics.sections.${current.value}`)

        return `${title} · ${t('nav.graphics')}`
    }),
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
    <div
        class="mx-auto px-4 sm:px-6 py-6 flex flex-wrap gap-6 items-start"
        :class="current === 'composer' ? 'max-w-[1920px]' : 'max-w-[1440px]'"
    >
        <aside
            v-if="current !== 'composer'"
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
                <select
                    class="field-input bg-base-100!"
                    :aria-label="t('graphics.page')"
                    :value="current"
                    @change="selectPage"
                >
                    <optgroup
                        v-for="entry in groupedPages"
                        :key="entry.group"
                        :label="t(`graphics.groups.${entry.group}`)"
                    >
                        <option v-for="page in entry.pages" :key="page.name" :value="page.name">
                            {{ t(`graphics.sections.${page.name}`) }}
                        </option>
                    </optgroup>
                </select>
            </label>

            <nav class="hidden lg:flex flex-col gap-0.5" :aria-label="t('nav.graphics')">
                <template v-for="entry in groupedPages" :key="entry.group">
                    <div
                        class="text-[11px] text-subtle tracking-[0.08em] uppercase px-2.5 pb-1.5"
                        :class="entry.group === 'main' ? 'pt-1.5' : 'pt-5'"
                    >
                        {{ t(`graphics.groups.${entry.group}`) }}
                    </div>
                    <RouterLink
                        v-for="page in entry.pages"
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
                    </RouterLink>
                </template>
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
