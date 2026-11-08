<script setup lang="ts">
import { useI18n } from 'vue-i18n'

import type { PlaylistDayMap } from '@/composables/playlistRows'

/** Overview only: the playlist page owns group selection and scrolling. */
defineProps<{ map: PlaylistDayMap }>()
const emit = defineEmits<{ jump: [groupId: string] }>()
const { t } = useI18n()
</script>
<template>
    <div
        :aria-label="t('onair.dayMap')"
        role="group"
        class="px-4 pt-3 pb-3.5 flex flex-col gap-1.5 border-b border-line"
    >
        <div class="relative h-8.5 rounded-lg overflow-hidden bg-base-200">
            <button
                v-for="(block, position) in map.blocks"
                :key="block.id"
                type="button"
                class="absolute top-0 bottom-0 border-0 border-e-2 border-base-100 px-1.5 text-left text-xs truncate cursor-pointer"
                :class="[
                    block.status === 'current'
                        ? 'bg-accent-soft text-accent-text outline-2 -outline-offset-2 outline-accent'
                        : position % 2
                          ? 'bg-base-300 text-base-content/80'
                          : 'bg-raised text-base-content/80',
                    block.status === 'past' ? 'opacity-55' : '',
                ]"
                :style="{ left: `${block.left}%`, width: `${block.width}%` }"
                :title="`${block.title} · ${block.range}`"
                :aria-label="`${block.title} · ${block.range}`"
                @click="emit('jump', block.id)"
            >
                {{ block.width > 4 ? block.title : '' }}
            </button>
            <span
                v-for="live in map.liveBlocks"
                :key="live.key"
                class="absolute top-0 bottom-0 bg-live text-base-100 text-[10px] font-semibold px-1 flex items-center overflow-hidden whitespace-nowrap pointer-events-none"
                :style="{ left: `${live.left}%`, width: `${live.width}%` }"
                :title="`${live.title} · ${live.range}`"
                >LIVE</span
            >
            <span
                v-for="marker in map.markers"
                :key="marker.key"
                class="absolute top-0 w-0 h-0 border-x-[6px] border-x-transparent border-t-8 border-t-accent -ms-1.5 pointer-events-none"
                :style="{ left: `${marker.left}%` }"
                :title="marker.title"
                aria-hidden="true"
            />
            <span
                v-if="map.now !== null"
                class="absolute -top-0.5 -bottom-0.5 w-0.5 bg-on-air pointer-events-none"
                :style="{ left: `${map.now}%` }"
                aria-hidden="true"
            />
        </div>
        <div
            class="flex justify-between font-mono text-[11px] text-subtle whitespace-nowrap max-sm:[&>:nth-child(even)]:hidden"
            aria-hidden="true"
        >
            <span v-for="(tick, index) in map.ticks" :key="index">{{ tick }}</span>
        </div>
    </div>
</template>
