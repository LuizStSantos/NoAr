import { authFetch } from '@/composables/authFetch'
import { createFilePreviewUrl } from '@/composables/fileAccess'

const JSON_HEADERS = { 'Content-Type': 'application/json;charset=UTF-8' }

/** Join storage path parts with single slashes, without a leading slash. */
export function storageJoin(...parts: (string | undefined | null)[]): string {
    return parts
        .filter((part) => part)
        .join('/')
        .replace(/\/+/g, '/')
        .replace(/^\/|\/$/g, '')
}

/** The last part of a storage path. */
export function baseName(path: string): string {
    return path.split('/').filter(Boolean).pop() ?? ''
}

export interface FolderListing {
    folders: string[]
    sheetFolders: string[]
}

/** The subfolders of a storage folder, for the folder tree. */
export async function listFolders(channelId: number, path: string): Promise<FolderListing> {
    const data = await authFetch<{ folders?: string[]; sheet_folders?: string[] }>(`/api/file/${channelId}/browse`, {
        method: 'POST',
        headers: JSON_HEADERS,
        body: JSON.stringify({ source: `/${path}`, folders_only: true }),
    })

    return { folders: data.folders ?? [], sheetFolders: data.sheet_folders ?? [] }
}

export function readSheet(channelId: number, path: string): Promise<MediaSheet> {
    return authFetch<MediaSheet>(`/api/file/${channelId}/sheet`, {
        method: 'POST',
        headers: JSON_HEADERS,
        body: JSON.stringify({ path }),
    })
}

export function saveSheet(
    channelId: number,
    path: string,
    update: { kind: SheetKind; fields: SheetFields } | { raw: string },
): Promise<MediaSheet> {
    return authFetch<MediaSheet>(`/api/file/${channelId}/sheet`, {
        method: 'PUT',
        headers: JSON_HEADERS,
        body: JSON.stringify({ path, ...update }),
    })
}

export function uploadArtwork(channelId: number, path: string, kind: ArtworkKind, file: File): Promise<MediaSheet> {
    const body = new FormData()
    body.append('file', file)

    return authFetch<MediaSheet>(
        `/api/file/${channelId}/artwork?path=${encodeURIComponent(path)}&kind=${encodeURIComponent(kind)}`,
        { method: 'PUT', body },
    )
}

export function removeArtwork(channelId: number, path: string, kind: ArtworkKind): Promise<MediaSheet> {
    return authFetch<MediaSheet>(`/api/file/${channelId}/artwork/remove`, {
        method: 'POST',
        headers: JSON_HEADERS,
        body: JSON.stringify({ path, kind }),
    })
}

/** A short-lived address to show a storage image in an `<img>`. */
export function imageUrl(channelId: number, path: string): Promise<string> {
    return createFilePreviewUrl(channelId, `/${path}`)
}

/** An empty sheet form. */
export function emptyFields(): SheetFields {
    return {
        title: '',
        plot: '',
        genres: [],
        rating: '',
        year: '',
        premiered: '',
        countries: [],
        actors: [],
        directors: [],
        season: null,
        episode: null,
    }
}
