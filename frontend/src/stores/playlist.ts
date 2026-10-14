import dayjs from 'dayjs'
import utc from 'dayjs/plugin/utc.js'
import timezone from 'dayjs/plugin/timezone.js'

import { defineStore } from 'pinia'

import { authFetch } from '@/composables/authFetch'
import { playlistOperations } from '@/composables/helper'
import { useAuth } from '@/stores/auth'
import { useIndex } from '@/stores/index'
import { useConfig } from '@/stores/config'

dayjs.extend(utc)
dayjs.extend(timezone)

const { processPlaylist } = playlistOperations()

/** What is saved of a playlist entry, to tell edited lists from saved ones. */
function snapshot(list: PlaylistItem[]): string {
    return JSON.stringify(
        list.map((item) => [
            item.source,
            item.in,
            item.out,
            item.duration,
            item.title || '',
            Boolean(item.ad),
            item.audio || '',
            item.custom_filter || '',
        ]),
    )
}

export const usePlaylist = defineStore('playlist', {
    state: () => ({
        playlist: [] as PlaylistItem[],
        isLoading: true,
        listDate: dayjs().format('YYYY-MM-DD'),
        /** `channel:date` of the list in `playlist`. */
        loadedKey: '',
        savedSnapshot: '[]',
        /** Edits since the list was loaded or saved. */
        editCount: 0,
        /** Entries added since the list was loaded or saved. */
        addedUids: [] as string[],
        progressValue: 0,
        current: {} as PlaylistItem,
        currentIndex: 0,
        ingestRuns: false,
        ingestListenerId: null as number | null,
        ingestListenerName: null as string | null,
        ingestListenerBackend: null as string | null,
        audioLevel: null as AudioLevel | null,
        loudness: null as LiveLoudnessMetrics | null,
        uptime: null as number | null,
        elapsedSec: 0,
        shift: 0,
        playoutIsRunning: false,
        firstLoad: true,
        scrollToItem: false,
    }),

    getters: {
        dirty: (state) => snapshot(state.playlist) !== state.savedSnapshot,
    },
    actions: {
        async getPlaylist(date: string) {
            const authStore = useAuth()
            const configStore = useConfig()
            const indexStore = useIndex()
            const channel = configStore.channels[configStore.i]?.id

            this.isLoading = true

            await authFetch<Playlist>(`/api/playlist/${channel}?date=${date}`, {
                method: 'GET',
                headers: authStore.authHeader,
            })
                .then((data: Playlist) => {
                    this.playlist = data.program ? processPlaylist(date, data.program, false) : []
                })
                .catch((e) => {
                    if (e.status >= 400) {
                        indexStore.msgAlert('error', e.data, 5)
                    }

                    this.playlist = []
                })

            this.loadedKey = `${channel}:${date}`
            this.markSaved()
            this.isLoading = false
        },

        /** Recalculate start times after the list changed. */
        touch() {
            this.editCount += 1
            this.playlist = processPlaylist(this.listDate, this.playlist, false)
        },

        markSaved() {
            this.savedSnapshot = snapshot(this.playlist)
            this.editCount = 0
            this.addedUids = []
        },

        setStatus(item: PlayoutStatus) {
            this.playoutIsRunning = true
            this.current = item.media
            this.currentIndex = item.index
            this.elapsedSec = item.elapsed
            this.ingestRuns = item.ingest
            this.ingestListenerId = item.ingest ? (item.ingest_listener_id ?? null) : null
            this.ingestListenerName = item.ingest ? (item.ingest_listener_name ?? null) : null
            this.ingestListenerBackend = item.ingest ? (item.ingest_listener_backend ?? null) : null
            this.audioLevel = item.audio ?? null
            this.loudness = item.loudness ?? null
            this.uptime = item.uptime ?? null
            this.shift = item.shift

            this.progressValue = (this.elapsedSec * 100) / this.current.out
        },

        resetStatus() {
            this.ingestRuns = false
            this.ingestListenerId = null
            this.ingestListenerName = null
            this.ingestListenerBackend = null
            this.elapsedSec = 0
            this.shift = 0
            this.audioLevel = null
            this.loudness = null
            this.uptime = null
            this.current = {} as PlaylistItem
            this.currentIndex = 0
        },
    },
})
