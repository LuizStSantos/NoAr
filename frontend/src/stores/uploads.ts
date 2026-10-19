import { defineStore } from 'pinia'
import { computed, ref } from 'vue'

import { randomId } from '@/composables/browser'
import { sendFileInChunks } from '@/composables/chunkedUpload'

/**
 * Every upload of the session, whatever page it was started from. Files wait
 * in one queue and go up one at a time (each in parallel chunks), so new
 * uploads to any folder can be added while others run, and they keep going
 * while the user works elsewhere in NoAr.
 */

export type UploadStatus = 'waiting' | 'uploading' | 'done' | 'error' | 'canceled'

export interface UploadItem {
    id: string
    channelId: number
    /** Storage folder it goes to; empty for the storage root. */
    folder: string
    file: File
    name: string
    size: number
    loaded: number
    status: UploadStatus
    error: string
}

/** A finished upload, for pages that show the folder it went to. */
export interface LandedUpload {
    channelId: number
    folder: string
    name: string
    at: number
}

/** Speed is averaged over this many seconds. */
const SPEED_WINDOW = 5

export const useUploads = defineStore('uploads', () => {
    const items = ref<UploadItem[]>([])
    const landed = ref<LandedUpload | null>(null)
    const samples: { time: number; bytes: number }[] = []
    let running = false
    let abort: AbortController | null = null

    const active = computed(() =>
        items.value.filter((item) => item.status === 'waiting' || item.status === 'uploading'),
    )
    const busy = computed(() => active.value.length > 0)
    const totalBytes = computed(() => active.value.reduce((sum, item) => sum + item.size, 0))
    const sentBytes = computed(() => active.value.reduce((sum, item) => sum + item.loaded, 0))
    const speed = ref(0)

    function key(channelId: number, folder: string, name: string) {
        return `${channelId}\0${folder}\0${name}`
    }

    /** Whether this file is already waiting or going up to that folder. */
    function pending(channelId: number, folder: string, name: string): boolean {
        const wanted = key(channelId, folder, name)

        return active.value.some((item) => key(item.channelId, item.folder, item.name) === wanted)
    }

    function add(channelId: number, folder: string, files: File[]): number {
        let added = 0

        for (const file of files) {
            if (pending(channelId, folder, file.name)) {
                continue
            }

            items.value.push({
                id: randomId(),
                channelId,
                folder,
                file,
                name: file.name,
                size: file.size,
                loaded: 0,
                status: 'waiting',
                error: '',
            })
            added++
        }

        void run()

        return added
    }

    function measure(item: UploadItem, loaded: number) {
        const now = Date.now() / 1000

        samples.push({ time: now, bytes: loaded - item.loaded })
        item.loaded = loaded

        while (samples.length && now - (samples[0]?.time ?? now) > SPEED_WINDOW) {
            samples.shift()
        }

        const span = Math.max(1, now - (samples[0]?.time ?? now))

        speed.value = samples.reduce((sum, sample) => sum + Math.max(0, sample.bytes), 0) / span
    }

    async function run() {
        if (running) {
            return
        }

        running = true

        try {
            let item = items.value.find((entry) => entry.status === 'waiting')

            while (item) {
                // Work on the reactive entry so the page sees the progress.
                const current = item

                current.status = 'uploading'
                current.loaded = 0
                abort = new AbortController()

                try {
                    await sendFileInChunks(
                        current.file,
                        {
                            url: `/api/file/${current.channelId}/upload?path=${encodeURIComponent(current.folder)}`,
                            method: 'PUT',
                        },
                        (loaded) => measure(current, loaded),
                        abort.signal,
                    )
                    current.status = 'done'
                    current.loaded = current.size
                    landed.value = {
                        channelId: current.channelId,
                        folder: current.folder,
                        name: current.name,
                        at: Date.now(),
                    }
                } catch (error) {
                    if ((current.status as UploadStatus) !== 'canceled') {
                        current.status = 'error'
                        current.error = error instanceof Error ? error.message : String(error)
                    }
                } finally {
                    abort = null
                }

                item = items.value.find((entry) => entry.status === 'waiting')
            }
        } finally {
            running = false
            samples.length = 0
            speed.value = 0
        }
    }

    function cancel(id: string) {
        const item = items.value.find((entry) => entry.id === id)

        if (!item || (item.status !== 'waiting' && item.status !== 'uploading')) {
            return
        }

        const wasUploading = item.status === 'uploading'

        item.status = 'canceled'

        if (wasUploading) {
            abort?.abort()
        }
    }

    function retry(id: string) {
        const item = items.value.find((entry) => entry.id === id)

        if (item && (item.status === 'error' || item.status === 'canceled')) {
            item.status = 'waiting'
            item.error = ''
            item.loaded = 0
            void run()
        }
    }

    /** Forget the finished, failed and canceled ones. */
    function clearFinished() {
        items.value = items.value.filter((item) => item.status === 'waiting' || item.status === 'uploading')
    }

    return {
        items,
        landed,
        active,
        busy,
        totalBytes,
        sentBytes,
        speed,
        pending,
        add,
        cancel,
        retry,
        clearFinished,
    }
})
