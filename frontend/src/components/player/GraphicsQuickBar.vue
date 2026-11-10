<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'

import { useGraphicsDisplayNames } from '@/composables/graphicsDisplayNames'
import { useChannelGraphics } from '@/composables/channelGraphics'
import { useConfig } from '@/stores/config'

/** The Player reports model state; all graphics operations live in Graphics. */
const { t } = useI18n()
const { templateName } = useGraphicsDisplayNames()
const config = useConfig()
const { models, onAir, unavailable } = useChannelGraphics()
const channelId = computed(() => config.channels[config.i]?.id)
const active = computed(() => models.value.filter((model) => model.on && model.ready))
</script>

<template>
    <section
        class="card-box px-4 py-3 flex flex-wrap items-center gap-3 text-sm"
        :aria-label="t('graphics.quick.label')"
        :title="t('graphics.onAir.permanentRequested')"
    >
        <strong>{{ t('graphics.quick.label') }}</strong>
        <span v-if="unavailable" class="text-muted">{{ t('graphics.onAir.statusUnavailable') }}</span>
        <template v-else>
            <span v-for="model in active" :key="model.template.id" class="inline-flex items-center gap-1.5 text-muted">
                <span class="dot bg-success" aria-hidden="true" />{{ templateName(model.template) }}
            </span>
            <span v-for="graphic in onAir" :key="graphic.take_id" class="text-muted">{{ graphic.name }}</span>
            <span v-if="!active.length && !onAir.length" class="text-muted">{{ t('graphics.onAir.noGraphic') }}</span>
        </template>
        <RouterLink
            :to="{ name: 'graphics-onair', query: { channel: channelId } }"
            class="ms-auto link-accent min-h-11 inline-flex items-center gap-1"
        >
            {{ t('graphics.quick.open') }} <i class="bi bi-arrow-right" aria-hidden="true" />
        </RouterLink>
    </section>
</template>
