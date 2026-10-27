<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import { authFetch } from '@/composables/authFetch'
import { useAuth } from '@/stores/auth'
import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'
import type { GraphicsScene } from '@/types/playout_config'

/** The channel's saved quick scenes, one button each, applied at once. */
const props = defineProps<{ compact?: boolean; disabled?: boolean }>()
const emit = defineEmits<{ applied: [] }>()

const { t } = useI18n()
const authStore = useAuth()
const configStore = useConfig()
const indexStore = useIndex()

const busy = ref<string | null>(null)

const channelId = computed(() => configStore.channels[configStore.i]?.id)
/** Saved scenes only: one being edited is not on the server yet. */
const scenes = computed<GraphicsScene[]>(() => configStore.playoutSaved?.processing?.graphics_scenes ?? [])

/** What a scene does, in a few words. */
function summary(scene: GraphicsScene): string {
    const parts: string[] = []

    if (scene.logo !== 'keep') {
        parts.push(t(`graphics.scenes.summary.logo.${scene.logo}`))
    }

    if (scene.logo_position) {
        parts.push(t('graphics.scenes.summary.logoMoves'))
    }

    if (scene.lshape !== 'keep') {
        parts.push(t(`graphics.scenes.summary.lshape.${scene.lshape}`))
    }

    if (scene.lshape_graphic) {
        parts.push(t('graphics.scenes.summary.graphic', { graphic: scene.lshape_graphic.split('/').pop() }))
    }

    if (scene.lshape_corner && scene.lshape_corner !== 'default') {
        parts.push(t(`lshape.corners.${scene.lshape_corner}`).toLowerCase())
    }

    return parts.join(' · ') || t('graphics.scenes.summary.nothing')
}

async function apply(scene: GraphicsScene) {
    busy.value = scene.name

    try {
        await authFetch(`/api/control/${channelId.value}/scene`, {
            method: 'POST',
            headers: { ...configStore.contentType, ...authStore.authHeader },
            body: JSON.stringify({ name: scene.name }),
        })
        indexStore.msgAlert('success', t('graphics.scenes.applied', { name: scene.name }), 2)
        emit('applied')
    } catch (error: any) {
        indexStore.msgAlert('error', error?.data ?? t('graphics.scenes.failed'), 4)
    } finally {
        busy.value = null
    }
}
</script>
<template>
    <div
        v-if="scenes.length"
        role="group"
        :aria-label="t('graphics.sections.scenes')"
        :class="props.compact ? 'contents' : 'grid grid-cols-[repeat(auto-fill,minmax(180px,1fr))] gap-2'"
    >
        <button
            v-for="scene in scenes"
            :key="scene.name"
            type="button"
            class="cursor-pointer border border-line-strong bg-base-100 hover:border-accent rounded-field text-left disabled:opacity-60 disabled:cursor-not-allowed"
            :class="props.compact ? 'min-h-11 px-3 text-sm' : 'min-h-16 px-3 py-2.5 flex flex-col gap-0.5'"
            :disabled="busy !== null || props.disabled"
            :title="props.compact ? summary(scene) : undefined"
            @click="apply(scene)"
        >
            <strong class="font-semibold text-sm">{{ scene.name }}</strong>
            <span v-if="!props.compact" class="text-xs text-muted">{{ summary(scene) }}</span>
        </button>
    </div>
</template>
