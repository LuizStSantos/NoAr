<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useDebounceFn } from '@vueuse/core'
import { useHead } from '@unhead/vue'

import AddProgramDialog from '@/components/player/AddProgramDialog.vue'
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
import { fileName, isRemoteSource, type AddPlace, type LibraryFile } from '@/composables/player'
import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'
import { useOnAir } from '@/composables/onAir'
import { usePlaylist } from '@/stores/playlist'
import { useProgrammes } from '@/stores/programmes'
import { useSchedule } from '@/stores/schedule'

const { t } = useI18n()
const { mediaType } = stringFormatter()
const configStore = useConfig()
const indexStore = useIndex()
const playlistStore = usePlaylist()
const programmes = useProgrammes()
const scheduleStore = useSchedule()

const playlistPanel = ref<InstanceType<typeof PlaylistPanel>>()
const showGenerator = ref(false)
/** A program folder being added, from the library (+) or dropped at an index. */
const program = ref<{ folder: string; dropIndex: number | null } | null>(null)
const preview = ref<{ name: string; url: string; options: Record<string, unknown> | null } | null>(null)

let statusTimer: ReturnType<typeof setInterval> | undefined
let dayTimer: ReturnType<typeof setInterval> | undefined

useHead({
    title: computed(() => t('nav.player')),
})

const playlistMode = computed(() => configStore.playout.processing?.mode !== 'folder')
const { currentTitle } = useOnAir()

/**
 * Building a playlist: the video, clock and transport make way for the
 * library and the list, with a line saying what is on air. The preview
 * stops meanwhile, which also spares the network.
 */
const building = ref(readBuilding())

function readBuilding(): boolean {
    try {
        return localStorage.getItem('noar.player.build') === '1'
    } catch {
        return false
    }
}

watch(building, (value) => {
    try {
        localStorage.setItem('noar.player.build', value ? '1' : '0')
    } catch {
        // Only remembered for this visit.
    }
})

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

function addProgram(files: LibraryFile[], place: AddPlace, label: string) {
    playlistPanel.value?.addFiles(files, place, label)
    program.value = null
}

function openGenerator() {
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
    <div
        class="max-w-[1440px] mx-auto px-4 sm:px-6 pt-5 flex flex-col gap-4"
        :class="building && playlistMode ? 'pb-4 lg:h-[calc(100dvh-57px)]' : 'pb-8'"
    >
        <div class="flex flex-wrap items-center gap-3">
            <div
                v-if="playlistMode"
                role="radiogroup"
                :aria-label="t('onair.layout')"
                class="inline-flex bg-base-200 border border-line-strong rounded-field p-1 gap-1"
            >
                <!-- Building the playlist is the main work here: it stands out even when not chosen. -->
                <button
                    v-for="option in [false, true]"
                    :key="String(option)"
                    type="button"
                    role="radio"
                    :aria-checked="building === option"
                    class="border-0 rounded-md px-4 min-h-10 text-[15px] font-medium cursor-pointer inline-flex items-center gap-2"
                    :class="
                        building === option
                            ? 'bg-accent text-accent-content shadow-sm'
                            : option
                              ? 'bg-transparent text-accent-text hover:bg-raised'
                              : 'bg-transparent text-muted hover:text-base-content hover:bg-raised'
                    "
                    @click="building = option"
                >
                    <i class="bi text-[16px]" :class="option ? 'bi-list-check' : 'bi-broadcast'" aria-hidden="true" />
                    {{ option ? t('onair.layoutBuild') : t('onair.layoutLive') }}
                </button>
            </div>
            <p v-if="building && playlistMode" class="m-0 text-[13px] text-muted truncate" role="status">
                <span v-if="playlistStore.playoutIsRunning" class="text-on-air-text font-semibold"
                    >● {{ t('nav.onAir') }}:</span
                >
                {{ playlistStore.playoutIsRunning ? currentTitle : t('onair.notRunning') }}
            </p>
        </div>

        <template v-if="!building || !playlistMode">
            <div class="flex flex-wrap gap-4">
                <LivePreview class="flex-[999_1_640px] min-w-0" />
                <NowPanel class="flex-[1_1_380px] min-w-0" />
            </div>

            <TransportBar />
        </template>

        <!--
            Live: the air only. Building: the library and the playlist fill
            the screen; only the list scrolls. The editor stays mounted while
            hidden, since the clock and "up next" read the list it loads.
        -->
        <div
            v-show="building || !playlistMode"
            class="flex flex-wrap gap-4 items-start"
            :class="building && playlistMode ? 'lg:flex-1 lg:min-h-0 lg:flex-nowrap lg:items-stretch' : ''"
        >
            <LibraryPanel
                class="flex-[1_1_280px] w-full lg:max-w-[340px] max-h-[70vh]"
                :class="
                    building && playlistMode
                        ? 'lg:max-h-none lg:h-full'
                        : 'lg:sticky lg:top-[72px] lg:max-h-[calc(100vh-96px)]'
                "
                :allow-add="playlistMode"
                @preview="openPreview"
                @add="addFile"
                @add-program="(folder) => (program = { folder, dropIndex: null })"
            />
            <PlaylistPanel
                v-if="playlistMode"
                ref="playlistPanel"
                class="flex-[999_1_640px]"
                :active="building"
                fill
                @preview="openPreview"
                @generate="openGenerator"
                @add-program="(folder, index) => (program = { folder, dropIndex: index })"
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
        <AddProgramDialog
            v-if="program"
            :folder="program.folder"
            :drop-index="program.dropIndex"
            @close="program = null"
            @add="addProgram"
        />
    </div>
</template>
