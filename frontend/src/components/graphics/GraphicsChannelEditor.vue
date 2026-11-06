<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute } from 'vue-router'

import GraphicsScenes from '@/components/graphics/GraphicsScenes.vue'
import GraphicsTicker from '@/components/graphics/GraphicsTicker.vue'
import SettingsLogo from '@/components/settings/SettingsLogo.vue'

const { t } = useI18n()
const route = useRoute()
const kind = computed(() => String(route.params.kind))
const modelName = computed(() => t(`graphics.assigned.${kind.value === 'scene' ? 'scenes' : kind.value}`))
</script>
<template>
    <div class="flex flex-col gap-4" data-testid="channel-graphics-editor">
        <header>
            <RouterLink
                :to="{ name: 'graphics-templates', query: { channel: route.query.channel } }"
                class="inline-flex items-center gap-2 text-sm text-accent mb-3"
            >
                <i class="bi bi-arrow-left" aria-hidden="true" />
                {{ t('graphics.editor.back') }}
            </RouterLink>
            <h1 class="m-0 text-xl font-semibold">{{ t('graphics.editor.title', { name: modelName }) }}</h1>
            <p class="mt-1 mb-0 text-muted">{{ t('graphics.editor.hint') }}</p>
        </header>

        <SettingsLogo
            v-if="kind === 'logo' || kind === 'lshape'"
            :key="kind"
            :part="kind"
            embedded
            :include-ticker="false"
        />
        <GraphicsTicker v-else-if="kind === 'ticker'" />
        <GraphicsScenes v-else-if="kind === 'scene'" embedded />
    </div>
</template>
