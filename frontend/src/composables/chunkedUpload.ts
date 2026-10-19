import { authFetch, authFetchRaw } from '@/composables/authFetch'
import { useHelper } from '@/composables/helper'

/** Where and how one file is sent. */
export interface ChunkedUploadRequest {
    url: string
    headers?: HeadersInit
    method?: 'POST' | 'PUT'
    extraFormData?: Record<string, string>
    responseType?: 'json' | 'text'
    chunkSize?: number
    parallelUploads?: number
    batchId?: string
}

const DEFAULT_CHUNK_SIZE = 1024 * 512
const DEFAULT_PARALLEL_UPLOADS = 4

/** Resumed attempts of the same file keep their batch on the server. */
const uploadBatches = new Map<string, string>()

async function uploadStatus(
    file: File,
    request: ChunkedUploadRequest,
    batchId: string,
    signal?: AbortSignal,
): Promise<{ received_ranges: [number, number][] }> {
    const url = new URL(request.url, window.location.origin)

    url.searchParams.set('file_name', file.name)
    url.searchParams.set('size', file.size.toString())
    url.searchParams.set('batch_id', batchId)

    return authFetch<{ received_ranges: [number, number][] }>(url, { headers: request.headers, signal })
}

/**
 * Send a file in chunks, a few at a time, skipping the byte ranges the server
 * already has from an earlier attempt. `onProgress` gets the bytes the server
 * confirmed so far. Aborting `signal` stops it with an `AbortError`.
 */
export async function sendFileInChunks<T = string>(
    file: File,
    request: ChunkedUploadRequest,
    onProgress?: (confirmedBytes: number) => void,
    signal?: AbortSignal,
): Promise<T | undefined> {
    const { errMsg } = useHelper()
    const chunkSize = request.chunkSize || DEFAULT_CHUNK_SIZE
    const parallelUploads = request.parallelUploads || DEFAULT_PARALLEL_UPLOADS
    const fileSize = Number(file.size)
    const uploadKey = `${request.url}\0${file.name}\0${fileSize}\0${file.lastModified}`
    const batchId = request.batchId || uploadBatches.get(uploadKey) || crypto.randomUUID()

    uploadBatches.set(uploadKey, batchId)

    const status = await uploadStatus(file, request, batchId, signal)
    const queue: { start: number; end: number }[] = []
    let completedBytes = 0
    let lastResponse: T | undefined

    for (let offset = 0; offset < fileSize; offset += chunkSize) {
        const end = Math.min(offset + chunkSize, fileSize)
        const received = status.received_ranges.some(([from, to]) => from <= offset && to >= end)

        if (received) {
            completedBytes += end - offset
        } else {
            queue.push({ start: offset, end })
        }
    }

    onProgress?.(completedBytes)

    async function worker() {
        while (queue.length) {
            const chunk = queue.shift()

            if (!chunk) {
                return
            }

            signal?.throwIfAborted()

            const form = new FormData()

            form.append('fileName', file.name)
            form.append('start', chunk.start.toString())
            form.append('end', chunk.end.toString())
            form.append('size', fileSize.toString())
            form.append('chunk', file.slice(chunk.start, chunk.end))
            form.append('batch_id', batchId)

            for (const [key, value] of Object.entries(request.extraFormData ?? {})) {
                form.append(key, value)
            }

            const response = await authFetchRaw(request.url, {
                method: request.method || 'PUT',
                headers: request.headers,
                body: form,
                signal,
            })

            if (!response.ok) {
                throw new Error(await errMsg(response))
            }

            completedBytes += chunk.end - chunk.start
            onProgress?.(Math.min(completedBytes, fileSize))
            lastResponse = (request.responseType === 'json' ? await response.json() : await response.text()) as T
        }
    }

    await Promise.all(
        Array(Math.min(parallelUploads, queue.length))
            .fill(0)
            .map(() => worker()),
    )
    uploadBatches.delete(uploadKey)

    return lastResponse
}
