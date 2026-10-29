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

/** Counts playlist loads, so a slow answer for an earlier date or channel is ignored. */
let loadRequest = 0

/** Undo history of the list being edited: earlier states, newest last. */
const MAX_UNDO = 50
let undoStack: PlaylistItem[][] = []
let redoStack: PlaylistItem[][] = []
/** The list as it was after the last change, the state an undo goes back to. */
let lastState: PlaylistItem[] = []

function copyList(list: PlaylistItem[]): PlaylistItem[] {
    return list.map((item) => ({ ...item }))
}

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
            item.marks ?? null,
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
        /** The last load failed: `playlist` does not reflect the saved list, so it must not be saved over it. */
        loadFailed: false,
        savedSnapshot: '[]',
        /** Edits since the list was loaded or saved. */
        editCount: 0,
        /** Entries added since the list was loaded or saved. */
        addedUids: [] as string[],
        /** Clips selected in the playlist editor. */
        selectedUids: [] as string[],
        /** Steps that undo and redo can take. */
        undoCount: 0,
        redoCount: 0,
        progressValue: 0,
        current: {} as PlaylistItem,
        currentIndex: 0,
        audioLevel: null as AudioLevel | null,
        loudness: null as LiveLoudnessMetrics | null,
        uptime: null as number | null,
        outputLink: null as OutputLinkStatus | null,
        elapsedSec: 0,
        shift: 0,
        playoutIsRunning: false,
        /** The status stream has answered for this channel: until then neither "on air" nor "stopped" is shown. */
        statusKnown: false,
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
            const request = ++loadRequest

            this.isLoading = true

            let program: PlaylistItem[] = []
            let failed = false

            try {
                const data = await authFetch<Playlist | undefined>(`/api/playlist/${channel}?date=${date}`, {
                    method: 'GET',
                    headers: authStore.authHeader,
                })
                // 204 No Content: no playlist exists for this date yet.
                program = data?.program ? processPlaylist(date, data.program, false) : []
            } catch (e) {
                failed = true

                if (request === loadRequest) {
                    indexStore.msgAlert('error', e instanceof Error ? e.message : String(e), 5)
                }
            }

            // A later load has started meanwhile; its answer decides the list.
            if (request !== loadRequest) {
                return
            }

            this.playlist = program
            this.loadFailed = failed
            this.loadedKey = `${channel}:${date}`
            this.markSaved()
            this.resetHistory()
            this.isLoading = false
        },

        /** Recalculate start times after the list changed, and remember the step for undo. */
        touch() {
            undoStack.push(lastState)
            if (undoStack.length > MAX_UNDO) {
                undoStack.shift()
            }
            redoStack = []
            this.editCount += 1
            this.playlist = processPlaylist(this.listDate, this.playlist, false)
            lastState = copyList(this.playlist)
            this.undoCount = undoStack.length
            this.redoCount = 0
        },

        /** Forget the undo history: a new list was loaded. */
        resetHistory() {
            undoStack = []
            redoStack = []
            lastState = copyList(this.playlist)
            this.undoCount = 0
            this.redoCount = 0
        },

        undo() {
            const previous = undoStack.pop()

            if (!previous) {
                return
            }

            redoStack.push(copyList(this.playlist))
            this.playlist = processPlaylist(this.listDate, copyList(previous), false)
            lastState = copyList(this.playlist)
            this.editCount = Math.max(0, this.editCount - 1)
            this.undoCount = undoStack.length
            this.redoCount = redoStack.length
        },

        redo() {
            const next = redoStack.pop()

            if (!next) {
                return
            }

            undoStack.push(copyList(this.playlist))
            this.playlist = processPlaylist(this.listDate, copyList(next), false)
            lastState = copyList(this.playlist)
            this.editCount += 1
            this.undoCount = undoStack.length
            this.redoCount = redoStack.length
        },

        markSaved() {
            this.savedSnapshot = snapshot(this.playlist)
            this.editCount = 0
            this.addedUids = []
        },

        setStatus(item: PlayoutStatus) {
            this.playoutIsRunning = true
            this.statusKnown = true
            this.current = item.media
            this.currentIndex = item.index
            this.elapsedSec = item.elapsed
            this.audioLevel = item.audio ?? null
            this.loudness = item.loudness ?? null
            this.uptime = item.uptime ?? null
            this.outputLink = item.output_link ?? null
            this.shift = item.shift

            this.progressValue = (this.elapsedSec * 100) / this.current.out
        },

        resetStatus() {
            this.elapsedSec = 0
            this.shift = 0
            this.audioLevel = null
            this.loudness = null
            this.uptime = null
            this.outputLink = null
            this.current = {} as PlaylistItem
            this.currentIndex = 0
        },
    },
})
