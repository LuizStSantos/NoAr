<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useHead } from '@unhead/vue'
import { RouterView, useRoute, useRouter } from 'vue-router'
import { cloneDeep } from 'es-toolkit/object'

import GenericModal from '@/components/utils/GenericModal.vue'
import { useConfig } from '@/stores/config'
import { useSettings, type SettingsSectionName } from '@/stores/settings'

const { t } = useI18n()
const route = useRoute()
const router = useRouter()
const configStore = useConfig()
const settings = useSettings()

const search = ref('')
const pendingChannel = ref<number | null>(null)

const current = computed(() => String(route.name ?? '').replace('settings-', '') as SettingsSectionName)
const channelId = computed(() => configStore.channels[configStore.i]?.id)

useHead({
    title: computed(() => `${t(`settings.sections.${current.value}`)} · ${t('nav.configure')}`),
})

function matches(name: SettingsSectionName): boolean {
    const needle = search.value.trim().toLowerCase()

    if (!needle) {
        return true
    }

    return `${t(`settings.sections.${name}`)} ${t(`settings.keywords.${name}`)}`.toLowerCase().includes(needle)
}

const groups = computed(() =>
    (['channel', 'system'] as const)
        .map((group) => ({
            group,
            sections: settings.visibleSections.filter((section) => section.group === group && matches(section.name)),
        }))
        .filter((entry) => entry.sections.length),
)

function link(name: SettingsSectionName) {
    return { name: `settings-${name}`, query: { channel: channelId.value } }
}

function openFirstMatch() {
    const first = groups.value[0]?.sections[0]

    if (first) {
        router.push(link(first.name))
    }
}

function selectSection(event: Event) {
    router.push(link((event.target as HTMLSelectElement).value as SettingsSectionName))
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

// The general page edits a copy of the selected channel.
watch(
    () => configStore.channels[configStore.i],
    (channel) => {
        if (!settings.channelDraft || settings.channelDraft.id !== channel?.id) {
            settings.channelDraft = channel ? cloneDeep(channel) : null
        }
    },
    { immediate: true },
)
</script>
<template>
    <div class="max-w-[1440px] mx-auto px-4 sm:px-6 py-6 flex flex-wrap gap-6 items-start">
        <aside
            :aria-label="t('settings.sectionsLabel')"
            class="flex-[1_1_240px] w-full lg:max-w-[280px] flex flex-col gap-4 lg:sticky lg:top-[80px]"
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

            <label
                class="hidden lg:flex items-center gap-2 bg-base-100 border border-line-strong rounded-field px-2.5 min-h-10 text-muted"
            >
                <i class="bi bi-search text-xs" aria-hidden="true" />
                <input
                    v-model="search"
                    type="search"
                    :placeholder="t('settings.search')"
                    class="bg-transparent border-0 text-base-content flex-1 min-w-0 outline-none"
                    @keydown.enter="openFirstMatch"
                />
            </label>

            <label class="field-label lg:hidden">
                {{ t('settings.page') }}
                <select class="field-input bg-base-100!" :value="current" @change="selectSection">
                    <option v-for="section in settings.visibleSections" :key="section.name" :value="section.name">
                        {{ t(`settings.sections.${section.name}`)
                        }}{{ settings.changedSections.includes(section.name) ? ' •' : '' }}
                    </option>
                </select>
            </label>

            <nav class="hidden lg:flex flex-col gap-0.5">
                <template v-for="entry in groups" :key="entry.group">
                    <div class="text-[11px] text-subtle tracking-[0.08em] uppercase px-2.5 pt-3.5 pb-1.5 first:pt-1.5">
                        {{ t(`settings.groups.${entry.group}`) }}
                    </div>
                    <RouterLink
                        v-for="section in entry.sections"
                        :key="section.name"
                        :to="link(section.name)"
                        :aria-current="current === section.name ? 'page' : undefined"
                        class="flex items-center gap-2 px-2.5 py-2 rounded-field"
                        :class="
                            current === section.name
                                ? 'bg-base-300 text-base-content'
                                : 'text-muted hover:bg-raised hover:text-base-content'
                        "
                    >
                        <span class="flex-auto">{{ t(`settings.sections.${section.name}`) }}</span>
                        <span
                            v-if="settings.changedSections.includes(section.name)"
                            class="dot bg-accent"
                            :title="t('settings.hasChanges')"
                        />
                    </RouterLink>
                </template>
                <p v-if="!groups.length" class="text-sm text-muted px-2.5 py-2">{{ t('settings.noMatch') }}</p>
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
