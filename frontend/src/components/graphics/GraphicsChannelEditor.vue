<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'

import GraphicsScenes from '@/components/graphics/GraphicsScenes.vue'
import { useGraphicsApi } from '@/composables/graphicsTemplates'
import { useConfig } from '@/stores/config'

const { t } = useI18n()
const route = useRoute()
const router = useRouter()
const config = useConfig()
const api = useGraphicsApi()
const kind = computed(() => String(route.params.kind))
const channelId = computed(() => config.channels[config.i]?.id)
const error = ref('')
let requestVersion = 0

async function openModel() {
    const version = ++requestVersion
    const requestedChannel = channelId.value
    const requestedKind = kind.value
    const requestedRoute = route.fullPath
    error.value = ''

    if (requestedKind === 'scene' || !requestedChannel) {
        return
    }

    try {
        const models = await api.channel()

        if (version !== requestVersion || requestedRoute !== route.fullPath || requestedChannel !== channelId.value) {
            return
        }

        const model = models.find((model) => model.kind === requestedKind)

        if (!model) {
            await router.replace({ name: 'graphics-templates', query: { channel: requestedChannel } })
            return
        }

        await router.replace({
            name: 'graphics-composer',
            query: { channel: requestedChannel, template: model.template.id },
        })
    } catch {
        if (version === requestVersion) {
            error.value = t('graphics.templates.loadFailed')
        }
    }
}

watch([kind, channelId], openModel, { immediate: true })
onBeforeUnmount(() => {
    ++requestVersion
})
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
            <template v-if="kind === 'scene'">
                <h1 class="m-0 text-xl font-semibold">
                    {{ t('graphics.editor.title', { name: t('graphics.assigned.scenes') }) }}
                </h1>
                <p class="mt-1 mb-0 text-muted">{{ t('graphics.editor.hint') }}</p>
            </template>
        </header>
        <GraphicsScenes v-if="kind === 'scene'" embedded />
        <div v-else-if="error" role="alert" class="flex flex-col items-start gap-3">
            <p>{{ error }}</p>
            <button class="btn-quiet" @click="openModel">{{ t('graphics.composer.retry') }}</button>
        </div>
    </div>
</template>
