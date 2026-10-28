import { authFetch } from '@/composables/authFetch'
import { createFilePreviewUrl } from '@/composables/fileAccess'
import { useAuth } from '@/stores/auth'

const JSON_HEADERS = { 'Content-Type': 'application/json;charset=UTF-8' }

/** Where Storage keeps the lives, one folder each, at the storage root. */
export const LIVES_FOLDER = 'Lives'

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
    /** Subfolders with a program sheet; `movieFolders` are the films among them. */
    sheetFolders: string[]
    movieFolders: string[]
    /** Subfolders holding a live's address file. */
    liveFolders: string[]
    /** Subfolders marked as commercials folders. */
    commercialFolders: string[]
}

/** The subfolders of a storage folder, for the folder tree. */
export async function listFolders(channelId: number, path: string): Promise<FolderListing> {
    const data = await authFetch<{
        folders?: string[]
        sheet_folders?: string[]
        movie_folders?: string[]
        live_folders?: string[]
        commercial_folders?: string[]
    }>(`/api/file/${channelId}/browse`, {
        method: 'POST',
        headers: JSON_HEADERS,
        body: JSON.stringify({ source: `/${path}`, folders_only: true }),
    })

    return {
        folders: data.folders ?? [],
        sheetFolders: data.sheet_folders ?? [],
        movieFolders: data.movie_folders ?? [],
        liveFolders: data.live_folders ?? [],
        commercialFolders: data.commercial_folders ?? [],
    }
}

/** How a clip shows up in the guide, from its sheets. */
export interface GuideEntry {
    title: string
    sub_title: string | null
}

/** The guide title and episode of each clip at `paths`, relative to `storage`. */
export async function guideEntries(channelId: number, storage: string, paths: string[]): Promise<GuideEntry[]> {
    const root = storage.replace(/\/+$/, '')
    const found = await authFetch<GuideEntry[]>(`/api/playlist/${channelId}/programs`, {
        method: 'POST',
        headers: JSON_HEADERS,
        body: JSON.stringify(paths.map((path) => ({ source: `${root}/${path}` }))),
    })

    return paths.map((_, index) => found[index] ?? { title: '', sub_title: null })
}

/**
 * The picture that stands for a clip or folder at `path`: its own poster or
 * still, else the poster of the program around it. Storage path, or null.
 */
export async function guidePoster(channelId: number, path: string): Promise<string | null> {
    const sheet = await readSheet(channelId, path)

    if (sheet.poster || sheet.still) {
        return sheet.poster || sheet.still
    }

    const program = sheet.program || sheet.inherited_from

    return program ? (await readSheet(channelId, program)).poster : null
}

/** A stretch of a video without logo, in seconds of the video. */
export interface LogoSpan {
    from: number
    to: number
}

/** Marks inside a video: where it keeps the logo off, and its ad breaks. */
export interface VideoMarks {
    hide: LogoSpan[]
    /** Ad breaks signalled with SCTE-35: out at `from`, back at `to`. */
    breaks: LogoSpan[]
    /** The part of the video that airs by default; a playlist entry may still set its own. */
    trim: LogoSpan | null
    /** Stretches shown in the L-shape. */
    lshape: LogoSpan[]
    /** This video's own L-shape graphic (storage path), for its stretches. */
    lshape_graphic: string | null
    /** The corner of the picture in this video's L-shape; null is the channel's. */
    lshape_corner: string | null
    /** The size of the picture in this video's L-shape, in percent; null is the channel's. */
    lshape_scale: number | null
    /** Where the logo goes while this video plays (`X%:Y%`); null is the channel's place. */
    logo_position: string | null
    /** The logo shown while this video plays (storage path); null is the channel's. */
    logo_image: string | null
}

/** Marks read or saved, with every part present. */
function completeMarks(marks: Partial<VideoMarks>): VideoMarks {
    return {
        hide: marks.hide ?? [],
        breaks: marks.breaks ?? [],
        trim: marks.trim ?? null,
        lshape: marks.lshape ?? [],
        lshape_graphic: marks.lshape_graphic ?? null,
        lshape_corner: marks.lshape_corner ?? null,
        lshape_scale: marks.lshape_scale ?? null,
        logo_position: marks.logo_position ?? null,
        logo_image: marks.logo_image ?? null,
    }
}

/** The marks of the video at `path`. */
export async function readLogoMarks(channelId: number, path: string): Promise<VideoMarks> {
    const marks = await authFetch<Partial<VideoMarks>>(`/api/file/${channelId}/logo-marks`, {
        method: 'POST',
        headers: JSON_HEADERS,
        body: JSON.stringify({ path }),
    })

    return completeMarks(marks)
}

export async function saveLogoMarks(channelId: number, path: string, marks: VideoMarks): Promise<VideoMarks> {
    const saved = await authFetch<Partial<VideoMarks>>(`/api/file/${channelId}/logo-marks`, {
        method: 'PUT',
        headers: JSON_HEADERS,
        body: JSON.stringify({ path, ...marks }),
    })

    return completeMarks(saved)
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

/** Credit roles in the order of the XMLTV DTD. */
export const CREDIT_ROLES: CreditRole[] = [
    'director',
    'actor',
    'writer',
    'adapter',
    'producer',
    'composer',
    'editor',
    'presenter',
    'commentator',
    'guest',
]

/**
 * How a person's photo is found: their name the way Kodi names the file
 * (`Ana Souza` -> `Ana_Souza.jpg`), ignoring case. Matches `photo_key` on the
 * server.
 */
export function photoKey(name: string): string {
    return name
        .trim()
        .split(/\s+/)
        .join('_')
        .replace(/[\p{Cc}/\\:*?"<>|]/gu, '')
        .replace(/^\.+/, '')
        .toLowerCase()
}

/**
 * Upload the photo of a person in the credits of the sheet at `path`;
 * `progress` gets the share sent, 0 to 1.
 */
export async function uploadPhoto(
    channelId: number,
    path: string,
    name: string,
    file: File,
    progress: (share: number) => void,
): Promise<MediaSheet> {
    const auth = useAuth()

    await auth.inspectToken()

    const body = new FormData()
    body.append('file', file)

    return new Promise((resolve, reject) => {
        const request = new XMLHttpRequest()

        request.open(
            'PUT',
            `/api/file/${channelId}/photo?path=${encodeURIComponent(path)}&name=${encodeURIComponent(name)}`,
        )
        request.setRequestHeader('Authorization', `Bearer ${auth.jwtToken}`)
        request.responseType = 'text'
        request.upload.onprogress = (event) => {
            if (event.lengthComputable && event.total > 0) {
                progress(event.loaded / event.total)
            }
        }
        request.onload = () => {
            if (request.status >= 200 && request.status < 300) {
                resolve(JSON.parse(request.responseText) as MediaSheet)
            } else {
                reject(new Error(request.responseText || request.statusText))
            }
        }
        request.onerror = () => reject(new Error(request.statusText || 'Network error'))
        request.send(body)
    })
}

/** Mark a storage folder as a commercials folder, or remove the mark. */
export async function markCommercial(channelId: number, path: string, commercial: boolean): Promise<void> {
    await authFetch(`/api/file/${channelId}/commercial`, {
        method: 'PUT',
        headers: JSON_HEADERS,
        body: JSON.stringify({ path, commercial }),
    })
}

/** Which of these storage paths lie in a commercials folder. */
export async function commercialPaths(channelId: number, paths: string[]): Promise<boolean[]> {
    if (!paths.length) {
        return []
    }

    const found = await authFetch<ClipProgramme[]>(`/api/playlist/${channelId}/programs`, {
        method: 'POST',
        headers: JSON_HEADERS,
        body: JSON.stringify(paths.map((source) => ({ source }))),
    })

    return paths.map((_, index) => Boolean(found[index]?.commercial))
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
        credits: [],
        season: null,
        episode: null,
    }
}
