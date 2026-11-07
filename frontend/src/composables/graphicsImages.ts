import { createFilePreviewUrl } from '@/composables/fileAccess'

/**
 * A URL that shows the storage image at `path` (relative to the channel's
 * storage, or absolute inside it), or '' when there is none.
 */
export async function storageImageUrl(
    channel: { id: number; storage: string } | undefined,
    path: string | undefined,
): Promise<string> {
    const storage = channel?.storage ?? ''
    const image = path?.trim()

    if (!image || !storage) {
        return ''
    }

    const absolute = image.startsWith('/') ? image : `${storage.replace(/\/+$/, '')}/${image}`
    const parent = storage.slice(0, storage.lastIndexOf('/'))

    if (!absolute.startsWith(storage)) {
        throw new Error('outside the storage')
    }

    return createFilePreviewUrl(channel?.id, absolute.slice(parent.length))
}
