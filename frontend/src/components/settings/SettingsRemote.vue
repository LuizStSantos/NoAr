<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'

import GraphicsApi from '@/components/graphics/GraphicsApi.vue'
import LiveRemote from '@/components/settings/LiveRemote.vue'
import Scte35Remote from '@/components/settings/Scte35Remote.vue'

/**
 * Every way to control NoAr from outside without a login, one tab per
 * area: the live on air, the graphics and the SCTE-35 breaks, each with its
 * key, its commands and a tutorial.
 */
const { t } = useI18n()
const route = useRoute()
const router = useRouter()

const AREAS = ['live', 'graphics', 'scte35'] as const
type Area = (typeof AREAS)[number]

/** The area in the address (`#graphics`), so links can open the right one. */
const area = computed<Area>(() => {
    const hash = route.hash.replace('#', '')
    return hash === 'live-api' ? 'live' : AREAS.includes(hash as Area) ? (hash as Area) : 'live'
})

function open(name: Area) {
    router.replace({ ...route, hash: `#${name}` })
}
</script>
<template>
    <div>
        <h1 class="m-0 text-xl font-semibold">{{ t('settings.sections.remote') }}</h1>
        <p class="mt-1 mb-0 text-muted">{{ t('settings.remote.intro') }}</p>
    </div>
    <div role="tablist" :aria-label="t('settings.remote.areas')" class="flex flex-wrap gap-2">
        <button
            v-for="name in AREAS"
            :id="`remote-tab-${name}`"
            :key="name"
            type="button"
            role="tab"
            class="min-h-11 px-4 rounded-field border text-left cursor-pointer flex flex-col"
            :class="area === name ? 'border-accent bg-accent/10' : 'border-line-strong hover:border-accent'"
            :aria-selected="area === name"
            :aria-controls="`remote-panel-${name}`"
            @click="open(name)"
        >
            <span class="font-semibold">{{ t(`settings.remote.${name}.title`) }}</span>
            <span class="text-xs text-muted">{{ t(`settings.remote.${name}.what`) }}</span>
        </button>
    </div>
    <div
        :id="`remote-panel-${area}`"
        role="tabpanel"
        :aria-labelledby="`remote-tab-${area}`"
        class="flex flex-col gap-4 min-w-0"
    >
        <div v-if="area === 'live'" id="live-api" class="scroll-mt-20">
            <LiveRemote :open="route.hash === '#live-api'" />
        </div>
        <GraphicsApi v-else-if="area === 'graphics'" embedded />
        <Scte35Remote v-else />
    </div>
</template>
