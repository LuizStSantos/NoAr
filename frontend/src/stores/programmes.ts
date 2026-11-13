import { computed, ref } from 'vue'
import { defineStore } from 'pinia'

import { authFetch } from '@/composables/authFetch'
import { playlistOperations } from '@/composables/helper'
import { fileName } from '@/composables/player'
import { useAuth } from '@/stores/auth'
import { useConfig } from '@/stores/config'
import { usePlaylist } from '@/stores/playlist'

/** Consecutive playlist clips that form one guide entry. */
export interface ProgrammeGroup {
    /** uid of the first clip, stable while clips are added inside the group. */
    id: string
    key: string
    title: string
    subTitle: string | null
    /** Index of the first and last clip in the playlist, inclusive. */
    first: number
    last: number
    /** Playlist seconds; may run past 86400 after midnight. */
    start: number
    end: number
    /** Short clips without a sheet (idents, promos) inside the program. */
    inserts: number
    /** Ads and filler inside the program. */
    breaks: number
}

function parentFolder(source: string): string {
    const parts = source.split('/').filter(Boolean)

    return parts.length > 1 ? (parts[parts.length - 2] ?? '') : ''
}

/** The guide entry of a clip whose name lookup is not in: its title or folder. */
function fallbackEntry(item: PlaylistItem): ClipProgramme {
    const folder = parentFolder(item.source)

    return {
        key: item.title || folder || item.source,
        title: item.title || folder || fileName(item.source),
        sub_title: null,
        is_break: Boolean(item.ad),
        insert: false,
    }
}

/** What the name lookup sends for each clip. */
function clipQuery(item: PlaylistItem) {
    return {
        source: item.source,
        title: item.title || null,
        ad: Boolean(item.ad),
        length: Math.max(0, item.out - item.in),
    }
}

/** Group consecutive clips into the programs the guide shows. */
function groupClips(items: PlaylistItem[], entryAt: (index: number, item: PlaylistItem) => ClipProgramme) {
    const list: ProgrammeGroup[] = []

    items.forEach((item, index) => {
        const entry = entryAt(index, item)
        const end = (item.begin ?? 0) + (item.out - item.in)
        const last = list[list.length - 1]

        // Breaks and inserts belong to the program they interrupt.
        if (last && (entry.is_break || entry.insert || entry.key === last.key)) {
            last.last = index
            last.end = end
            last.breaks += entry.is_break ? 1 : 0
            last.inserts += entry.insert && !entry.is_break ? 1 : 0
            return
        }

        list.push({
            id: item.uid,
            key: entry.key,
            title: entry.title,
            subTitle: entry.sub_title,
            first: index,
            last: index,
            start: item.begin ?? 0,
            end,
            inserts: 0,
            breaks: 0,
        })
    })

    return list
}

/** Clips of the next day read to name its first program. */
const OPENING_CLIPS = 40
/** How long a next day's first program is kept before it is read again. */
const OPENING_MAX_AGE_MS = 60_000

/**
 * Program names of the playlist clips, from the same `.nfo` lookup the XMLTV
 * guide uses, so the player groups the list the way the guide will show it.
 */
export const useProgrammes = defineStore('programmes', () => {
    const playlistStore = usePlaylist()

    const entries = ref<ClipProgramme[]>([])
    const signature = ref('')
    /**
     * The first program of a day's playlist, by `channel:date`: what comes
     * next when the program on air is the last of its day. `null` when that
     * day has no playlist.
     */
    const opening = ref<{ key: string; group: ProgrammeGroup | null; at: number } | null>(null)
    let openingInFlight: string | null = null
    /** The lookup on its way, so callers that wait for it share one request. */
    let inFlight: { signature: string; done: Promise<void> } | null = null

    function currentSignature(): string {
        return JSON.stringify(
            playlistStore.playlist.map((item) => [item.source, item.title ?? '', Boolean(item.ad), item.out - item.in]),
        )
    }

    /** Look the names up for the playlist as it is now; resolves once they are in. */
    async function refresh() {
        const configStore = useConfig()
        const id = configStore.channels[configStore.i]?.id
        const wanted = currentSignature()

        if (!id || wanted === signature.value) {
            return
        }

        if (inFlight?.signature === wanted) {
            return inFlight.done
        }

        const done = lookup(id, wanted)

        inFlight = { signature: wanted, done }

        try {
            await done
        } finally {
            if (inFlight?.done === done) {
                inFlight = null
            }
        }
    }

    async function lookup(id: number, wanted: string) {
        const authStore = useAuth()
        const configStore = useConfig()
        const clips = playlistStore.playlist.map(clipQuery)

        if (!clips.length) {
            entries.value = []
            signature.value = wanted
            return
        }

        try {
            const result = await authFetch<ClipProgramme[]>(`/api/playlist/${id}/programs`, {
                method: 'POST',
                headers: { ...configStore.contentType, ...authStore.authHeader },
                body: JSON.stringify(clips),
            })

            if (currentSignature() === wanted) {
                entries.value = result
                signature.value = wanted
            }
        } catch {
            // Groups fall back to folder names until the lookup works.
        }
    }

    function entryAt(index: number, item: PlaylistItem): ClipProgramme {
        const entry = entries.value.length === playlistStore.playlist.length ? entries.value[index] : undefined

        return entry ?? fallbackEntry(item)
    }

    const groups = computed<ProgrammeGroup[]>(() => groupClips(playlistStore.playlist, entryAt))

    /**
     * Read the first program of `date`'s playlist into `opening`, unless it
     * was read less than a minute ago. Its times are in that day's playlist
     * seconds.
     */
    async function loadOpening(date: string) {
        const authStore = useAuth()
        const configStore = useConfig()
        const id = configStore.channels[configStore.i]?.id
        const key = `${id}:${date}`

        if (
            !id ||
            openingInFlight === key ||
            (opening.value?.key === key && Date.now() - opening.value.at < OPENING_MAX_AGE_MS)
        ) {
            return
        }

        openingInFlight = key

        try {
            const data = await authFetch<Playlist | undefined>(`/api/playlist/${id}?date=${date}`, {
                method: 'GET',
                headers: authStore.authHeader,
            })
            // Its own copies: the list on screen is not touched.
            const program = data?.program
                ? playlistOperations().processPlaylist(
                      date,
                      structuredClone(data.program).slice(0, OPENING_CLIPS),
                      false,
                  )
                : []
            let found: ClipProgramme[] = []

            if (program.length) {
                try {
                    found = await authFetch<ClipProgramme[]>(`/api/playlist/${id}/programs`, {
                        method: 'POST',
                        headers: { ...configStore.contentType, ...authStore.authHeader },
                        body: JSON.stringify(program.map(clipQuery)),
                    })
                } catch {
                    // Named by title or folder until the lookup works.
                }
            }

            const named = (index: number, item: PlaylistItem) =>
                (found.length === program.length ? found[index] : undefined) ?? fallbackEntry(item)

            opening.value = { key, group: groupClips(program, named)[0] ?? null, at: Date.now() }
        } catch {
            // Nothing is shown as next until the playlist can be read.
        } finally {
            if (openingInFlight === key) {
                openingInFlight = null
            }
        }
    }

    /** The first program of `date`'s playlist, once `loadOpening` has read it. */
    function openingOf(date: string): ProgrammeGroup | null {
        const configStore = useConfig()
        const key = `${configStore.channels[configStore.i]?.id}:${date}`

        return opening.value?.key === key ? opening.value.group : null
    }

    function groupOf(index: number): ProgrammeGroup | undefined {
        return groups.value.find((group) => group.first <= index && index <= group.last)
    }

    /** How a clip sits in its program: an ad or filler, an insert, or a part of it. */
    function roleOf(index: number): 'break' | 'insert' | null {
        const item = playlistStore.playlist[index]

        if (!item) {
            return null
        }

        const entry = entryAt(index, item)

        if (entry.is_break) {
            return 'break'
        }

        return entry.insert ? 'insert' : null
    }

    /** Whether a clip comes from a commercials folder. */
    function isCommercial(index: number): boolean {
        return entries.value.length === playlistStore.playlist.length && Boolean(entries.value[index]?.commercial)
    }

    return { entries, groups, refresh, groupOf, roleOf, isCommercial, loadOpening, openingOf }
})
