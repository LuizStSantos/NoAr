import { computed, reactive, ref, watch, type Ref } from 'vue'

import { imageUrl, storageJoin } from '@/composables/storage'

export type StorageFileView = 'videos' | 'images' | 'all'

/** A folder image and its current short-lived preview URL. */
export interface StorageViewerImage {
    key: string
    name: string
    url: string | null
    size: number
    dims: string | null
}

/** Changes when the file does, so a replaced image is not taken from the cache. */
export function storageImageKey(file: FileObject): string {
    return `${file.name}:${file.modified ?? ''}:${file.size ?? ''}`
}

interface StorageImageOptions {
    channelId: Readonly<Ref<number>>
    current: Readonly<Ref<string>>
    view: Readonly<Ref<StorageFileView>>
    images: Readonly<Ref<FileObject[]>>
    shownImages: Readonly<Ref<FileObject[]>>
}

/** Own expiring image addresses and viewer selection, never folder navigation. */
export function useStorageImages({ channelId, current, view, images, shownImages }: StorageImageOptions) {
    // Images are shown from short-lived addresses, fetched a few at a time.
    const imageUrls = reactive(new Map<string, string>())
    const imageDims = reactive(new Map<string, string>())
    const retried = new Set<string>()
    let imagesFolder: string | null = null

    async function loadImageUrls() {
        const folder = current.value

        if (imagesFolder !== folder) {
            imageUrls.clear()
            imageDims.clear()
            retried.clear()
            imagesFolder = folder
        }

        if (view.value === 'videos' || !channelId.value) {
            return
        }

        const queue = images.value.filter((file) => !imageUrls.has(storageImageKey(file)))

        async function worker() {
            for (let file = queue.shift(); file; file = queue.shift()) {
                try {
                    const url = await imageUrl(channelId.value, storageJoin(folder, file.name))

                    if (folder === current.value) {
                        imageUrls.set(storageImageKey(file), url)
                    }
                } catch {
                    // The row keeps its icon.
                }
            }
        }

        await Promise.all([worker(), worker(), worker(), worker()])
    }

    /** An address expired (they last a quarter of an hour): ask for a new one once. */
    function imageFailed(file: FileObject) {
        const key = storageImageKey(file)

        if (!retried.has(key)) {
            retried.add(key)
            imageUrls.delete(key)
            loadImageUrls()
        }
    }

    function imageLoaded(event: Event, file: FileObject) {
        const element = event.target as HTMLImageElement

        if (element.naturalWidth) {
            imageDims.set(storageImageKey(file), `${element.naturalWidth}×${element.naturalHeight}`)
        }
    }

    const viewerIndex = ref<number | null>(null)
    const viewerImages = computed<StorageViewerImage[]>(() =>
        shownImages.value.map((file) => ({
            key: storageImageKey(file),
            name: file.name,
            url: imageUrls.get(storageImageKey(file)) ?? null,
            size: file.size ?? 0,
            dims: imageDims.get(storageImageKey(file)) ?? null,
        })),
    )

    // The image on screen was deleted or renamed away.
    watch(
        () => viewerImages.value.length,
        (length) => {
            if (viewerIndex.value !== null && viewerIndex.value >= length) {
                viewerIndex.value = length ? length - 1 : null
            }
        },
    )

    function openViewer(name: string) {
        const position = shownImages.value.findIndex((file) => file.name === name)

        if (position >= 0) {
            viewerIndex.value = position
            loadImageUrls()
        }
    }

    watch([images, view], () => loadImageUrls())

    return { imageUrls, imageDims, imageFailed, imageLoaded, viewerIndex, viewerImages, openViewer }
}
