<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useHead } from '@unhead/vue'

import { useConfig } from '@/stores/config'

import SystemStats from '@/components/SystemStats.vue'
import UpdateNotice from '@/components/UpdateNotice.vue'

const { t } = useI18n()
const configStore = useConfig()

useHead({
    title: computed(() => t('nav.home')),
})
</script>
<template>
    <div class="max-w-[1440px] mx-auto px-4 sm:px-6 pt-5 pb-8 flex flex-col gap-4">
        <div>
            <h1 class="m-0 text-xl font-semibold">{{ t('home.title') }}</h1>
            <p class="mt-1 text-muted">
                {{ t('home.subtitle', { channel: configStore.channels[configStore.i]?.name ?? '' }) }}
            </p>
        </div>
        <UpdateNotice />
        <SystemStats v-if="configStore.channels.length > 0" />
    </div>
</template>
