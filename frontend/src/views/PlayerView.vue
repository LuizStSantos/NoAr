<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useDebounceFn } from '@vueuse/core'
import { useHead } from '@unhead/vue'

import LibraryPanel from '@/components/player/LibraryPanel.vue'
import LivePreview from '@/components/player/LivePreview.vue'
import NowPanel from '@/components/player/NowPanel.vue'
import PlaylistGenerator from '@/components/player/PlaylistGenerator.vue'
import PlaylistPanel from '@/components/player/PlaylistPanel.vue'
import TransportBar from '@/components/player/TransportBar.vue'
import GenericModal from '@/components/utils/GenericModal.vue'
import VideoPlayer from '@/components/utils/VideoPlayer.vue'

import { createFilePreviewUrl } from '@/composables/fileAccess'
import { stringFormatter } from '@/composables/helper'
import { fileName, isRemoteSource, type LibraryFile } from '@/composables/player'
import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'
import { useMedia } from '@/stores/media'
import { usePlaylist } from '@/stores/playlist'
import { useProgrammes } from '@/stores/programmes'
import { useSchedule } from '@/stores/schedule'

const { t } = useI18n()
const { mediaType } = stringFormatter()
const configStore = useConfig()
const indexStore = useIndex()
const mediaStore = useMedia()
const playlistStore = usePlaylist()
const programmes = useProgrammes()
const scheduleStore = useSchedule()

const playlistPanel = ref<InstanceType<typeof PlaylistPanel>>()
const showGenerator = ref(false)
const preview = ref<{ name: string; url: string; options: Record<string, unknown> | null } | null>(null)

let statusTimer: ReturnType<typeof setInterval> | undefined
let dayTimer: ReturnType<typeof setInterval> | undefined

useHead({
    title: computed(() => t('nav.player')),
})

const playlistMode = computed(() => configStore.playout.processing?.mode !== 'folder')

// The guide lookup runs a moment after the last change, not on every keystroke.
const refreshProgrammes = useDebounceFn(() => programmes.refresh(), 400)

watch(() => playlistStore.playlist, refreshProgrammes, { deep: true, immediate: true })

async function loadStatus() {
    try {
        await scheduleStore.loadStatus()
    } catch {
        // The panels stay quiet when the scheduler cannot be reached.
    }
}

async function loadDay() {
    try {
        await scheduleStore.loadDay(playlistStore.listDate)
    } catch {
        // Lives and events are extra information on the player.
    }
}

onMounted(() => {
    loadStatus()
    loadDay()
    scheduleStore.loadLives().catch(() => undefined)
    statusTimer = setInterval(loadStatus, 3000)
    dayTimer = setInterval(loadDay, 60000)
})

onBeforeUnmount(() => {
    clearInterval(statusTimer)
    clearInterval(dayTimer)
})

watch(() => [playlistStore.listDate, configStore.i], loadDay)

function addFile(file: LibraryFile) {
    playlistPanel.value?.addFile(file)
}

function openGenerator() {
    mediaStore.getTree('', true)
    showGenerator.value = true
}

/** Path for the file access token: relative to the folder above the storage root. */
function accessPath(source: string): string {
    const storage = configStore.channels[configStore.i]?.storage ?? ''
    const parent = storage.slice(0, storage.lastIndexOf('/'))

    return parent && source.startsWith(parent) ? source.slice(parent.length) : source
}

async function openPreview(source: string) {
    const name = fileName(source)
    let url = source

    if (!isRemoteSource(source)) {
        try {
            url = await createFilePreviewUrl(configStore.channels[configStore.i]?.id, accessPath(source))
        } catch (error) {
            indexStore.msgAlert('error', error instanceof Error ? error.message : String(error), 5)
            return
        }
    }

    const kind = mediaType(name)
    const extension = name.split('.').pop()?.toLowerCase() ?? ''
    const type =
        kind === 'audio'
            ? `audio/${extension}`
            : kind === 'live' || url.includes('.m3u8')
              ? 'application/x-mpegURL'
              : `video/${extension === 'mov' ? 'mp4' : extension}`

    preview.value = {
        name,
        url,
        options:
            kind === 'image'
                ? null
                : {
                      liveui: false,
                      controls: true,
                      suppressNotSupportedError: true,
                      autoplay: true,
                      preload: 'auto',
                      sources: [{ type, src: url }],
                  },
    }
}
</script>
<template>
    <div class="max-w-[1440px] mx-auto px-4 sm:px-6 pt-5 pb-8 flex flex-col gap-4">
        <div class="flex flex-wrap gap-4">
            <LivePreview class="flex-[999_1_640px] min-w-0" />
            <NowPanel class="flex-[1_1_380px] min-w-0" />
        </div>

        <TransportBar />

        <div class="flex flex-wrap gap-4 items-start">
            <LibraryPanel
                class="flex-[1_1_280px] w-full lg:max-w-[340px] lg:sticky lg:top-[72px] max-h-[70vh] lg:max-h-[calc(100vh-96px)]"
                :allow-add="playlistMode"
                @preview="openPreview"
                @add="addFile"
            />
            <PlaylistPanel
                v-if="playlistMode"
                ref="playlistPanel"
                class="flex-[999_1_640px]"
                @preview="openPreview"
                @generate="openGenerator"
            />
            <section v-else class="card-box flex-[999_1_640px] p-6 flex flex-col gap-2">
                <h2 class="m-0 text-[15px] font-semibold">{{ t('onair.folderModeTitle') }}</h2>
                <p class="m-0 text-muted">{{ t('onair.folderModeText') }}</p>
                <RouterLink
                    :to="{ path: '/configure/playlist', query: { channel: configStore.channels[configStore.i]?.id } }"
                    class="link-accent self-start"
                >
                    {{ t('onair.folderModeLink') }}
                </RouterLink>
            </section>
        </div>

        <GenericModal
            :show="preview !== null"
            :title="preview ? `${t('onair.preview')}: ${preview.name}` : ''"
            hide-buttons
            :modal-action="() => (preview = null)"
        >
            <div v-if="preview" class="w-5xl max-w-full aspect-video bg-black rounded-field overflow-hidden">
                <VideoPlayer v-if="preview.options" reference="previewPlayer" :options="preview.options" />
                <img v-else :src="preview.url" class="w-full h-full object-contain" :alt="preview.name" />
            </div>
        </GenericModal>

        <PlaylistGenerator v-if="showGenerator" :close="() => (showGenerator = false)" />
    </div>
</template>
