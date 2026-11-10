<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import { useRoute } from 'vue-router'

import LiveRemote from '@/components/settings/LiveRemote.vue'
import { useConfig } from '@/stores/config'

/**
 * Control from outside NoAr: the live key for encoders and scripts, and
 * where the graphics and SCTE-35 keys are.
 */
const { t } = useI18n()
const configStore = useConfig()
const route = useRoute()
</script>
<template>
    <div>
        <h1 class="m-0 text-xl font-semibold">{{ t('settings.sections.remote') }}</h1>
        <p class="mt-1 mb-0 text-muted">{{ t('settings.remote.intro') }}</p>
    </div>
    <div id="live-api" class="scroll-mt-20">
        <LiveRemote :open="route.hash === '#live-api'" />
    </div>
    <p class="m-0 text-sm text-muted">
        {{ t('settings.remote.otherKeys') }}
        <RouterLink
            class="link-accent"
            :to="{ name: 'graphics-api', query: { channel: configStore.channels[configStore.i]?.id } }"
            >{{ t('settings.remote.graphicsKey') }}</RouterLink
        >
        ·
        <RouterLink
            class="link-accent"
            :to="{ name: 'settings-breaks', query: { channel: configStore.channels[configStore.i]?.id } }"
            >{{ t('settings.remote.scte35Key') }}</RouterLink
        >
    </p>
</template>
