<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'

import LogoRemote from '@/components/settings/LogoRemote.vue'
import SectionCard from '@/components/settings/SectionCard.vue'
import { copyText } from '@/composables/browser'
import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'

/** Grafismo › External control: the graphics key, the tutorial and the scene addresses. */
const { t } = useI18n()
const configStore = useConfig()
const indexStore = useIndex()

const channelId = computed(() => configStore.channels[configStore.i]?.id)
const key = computed(
    () => configStore.playout?.processing?.logo_key?.trim() || t('settings.logo.remote.keyPlaceholder'),
)
const scenes = computed(() => configStore.playoutSaved?.processing?.graphics_scenes ?? [])

/** A scene's name as written in its address. */
function slug(name: string): string {
    return name
        .trim()
        .toLowerCase()
        .replace(/[^\p{L}\p{N}]+/gu, '-')
        .replace(/^-+|-+$/g, '')
}

function sceneUrl(name: string): string {
    return `${window.location.origin}/scene/${channelId.value}/${encodeURIComponent(slug(name))}?key=${encodeURIComponent(key.value)}`
}

async function copy(text: string) {
    try {
        await copyText(text)
        indexStore.msgAlert('success', t('settings.logo.remote.copied'), 2)
    } catch {
        indexStore.msgAlert('error', t('settings.logo.remote.copyFailed'), 3)
    }
}
</script>
<template>
    <div>
        <h1 class="m-0 text-xl font-semibold">{{ t('graphics.sections.api') }}</h1>
        <p class="mt-1 mb-0 text-muted">{{ t('graphics.intro.api') }}</p>
    </div>

    <template v-if="configStore.playout?.processing">
        <LogoRemote part="api" />

        <SectionCard :title="t('graphics.api.scenesTitle')" :description="t('graphics.api.scenesIntro')">
            <div v-for="scene in scenes" :key="scene.name" class="flex flex-col gap-1">
                <span class="text-xs text-muted">{{ scene.name }}</span>
                <div class="flex gap-2 items-start">
                    <code
                        class="flex-1 min-w-0 break-all font-mono text-xs bg-field border border-line rounded-md px-2.5 py-1.5"
                        >{{ sceneUrl(scene.name) }}</code
                    >
                    <button
                        class="btn-icon min-w-8 min-h-8 flex-none"
                        :aria-label="t('settings.logo.remote.copy')"
                        @click="copy(sceneUrl(scene.name))"
                    >
                        <i class="bi bi-clipboard" aria-hidden="true" />
                    </button>
                </div>
            </div>
            <p v-if="!scenes.length" class="m-0 text-muted text-sm">{{ t('graphics.scenes.none') }}</p>
        </SectionCard>
    </template>
</template>
