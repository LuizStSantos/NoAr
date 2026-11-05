<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'

import SceneButtons from '@/components/graphics/SceneButtons.vue'
import LogoRemote from '@/components/settings/LogoRemote.vue'
import SectionCard from '@/components/settings/SectionCard.vue'
import { useAuth } from '@/stores/auth'
import { useConfig } from '@/stores/config'

/** Grafismo › Live now: the logo, the L-shape and the quick scenes on air. */
const { t } = useI18n()
const authStore = useAuth()
const configStore = useConfig()

const remote = ref<InstanceType<typeof LogoRemote> | null>(null)
const isAdmin = ['global_admin', 'channel_admin'].includes(authStore.role)
</script>
<template>
    <div>
        <h1 class="m-0 text-xl font-semibold">{{ t('graphics.sections.live') }}</h1>
        <p class="mt-1 mb-0 text-muted">{{ t('graphics.intro.live') }}</p>
    </div>

    <template v-if="configStore.playout?.processing">
        <LogoRemote ref="remote" part="live" />

        <SectionCard :title="t('graphics.sections.scenes')" :description="t('graphics.scenes.liveIntro')">
            <SceneButtons @applied="remote?.load()" />
            <p v-if="!configStore.playoutSaved?.processing?.graphics_scenes?.length" class="m-0 text-muted text-sm">
                {{ t('graphics.scenes.none') }}
                <RouterLink
                    v-if="isAdmin"
                    :to="{ name: 'graphics-scenes', query: { channel: configStore.channels[configStore.i]?.id } }"
                    class="link-accent"
                >
                    {{ t('graphics.scenes.create') }}
                </RouterLink>
            </p>
        </SectionCard>
    </template>
</template>
