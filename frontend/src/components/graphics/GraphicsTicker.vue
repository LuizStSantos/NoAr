<script setup lang="ts">
import { computed, nextTick, ref, toRaw, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import SectionCard from '@/components/settings/SectionCard.vue'
import { type GraphicsTemplate, type TemplateLayer, useGraphicsApi } from '@/composables/graphicsTemplates'
import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'

const { t } = useI18n()
const api = useGraphicsApi()
const config = useConfig()
const channelId = computed(() => config.channels[config.i]?.id)
const index = useIndex()
const model = ref<GraphicsTemplate | null>(null)
const text = ref('')
const savedText = ref('')
const saving = ref(false)

watch(
    channelId,
    async (requestedChannel) => {
        model.value = null
        try {
            const ticker = (await api.channel()).find((view) => view.kind === 'ticker')
            if (requestedChannel !== channelId.value) {
                return
            }

            model.value = ticker?.template ?? null
            text.value = model.value?.layers.find((layer) => layer.kind === 'text')?.text ?? ''
            savedText.value = text.value
            await nextTick()

            if (location.hash === '#ticker') {
                document.getElementById('ticker')?.scrollIntoView({ block: 'center' })
            }
        } catch {
            index.msgAlert('error', t('graphics.templates.loadFailed'), 4)
        }
    },
    { immediate: true },
)

async function saveTicker() {
    if (!model.value) {
        return
    }

    saving.value = true

    try {
        const draft = structuredClone(toRaw(model.value))
        let layer = draft.layers.find((layer) => layer.kind === 'text')

        if (!layer) {
            layer = {
                id: 'ticker-text',
                name: 'Ticker',
                kind: 'text',
                visible: true,
                x: 5,
                y: 85,
                width: 90,
                height: 10,
                opacity: 1,
                text: '',
                font_family: null,
                font_size: 4,
                bold: false,
                color: '#ffffff',
                align: 'left',
                image: null,
                fill: '#151b28',
                radius: 0,
                animation_in: 'fade',
                animation_out: 'fade',
                animation_ms: 400,
                delay_ms: 0,
            } satisfies TemplateLayer
            draft.layers.push(layer)
        }

        layer.text = text.value
        model.value = await api.save(draft, draft.id)
        savedText.value = text.value
        index.msgAlert('success', t('settings.saved'), 2)
    } catch {
        index.msgAlert('error', t('graphics.composer.saveFailed'), 4)
    } finally {
        saving.value = false
    }
}
</script>
<template>
    <SectionCard v-if="model" id="ticker" :title="t('graphics.assigned.ticker')">
        <label class="field-label">
            {{ t('graphics.assigned.tickerText') }}
            <textarea v-model="text" class="field-input min-h-24" />
        </label>
        <div class="flex justify-end">
            <button class="btn-main" :disabled="saving || text === savedText" @click="saveTicker">
                {{ t('graphics.assigned.saveTicker') }}
            </button>
        </div>
    </SectionCard>
</template>
