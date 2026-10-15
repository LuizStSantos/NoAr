import { computed, ref } from 'vue'
import { defineStore } from 'pinia'

import { authFetch } from '@/composables/authFetch'
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

/**
 * Program names of the playlist clips, from the same `.nfo` lookup the XMLTV
 * guide uses, so the player groups the list the way the guide will show it.
 */
export const useProgrammes = defineStore('programmes', () => {
    const playlistStore = usePlaylist()

    const entries = ref<ClipProgramme[]>([])
    const signature = ref('')

    function currentSignature(): string {
        return JSON.stringify(
            playlistStore.playlist.map((item) => [item.source, item.title ?? '', Boolean(item.ad), item.out - item.in]),
        )
    }

    async function refresh() {
        const authStore = useAuth()
        const configStore = useConfig()
        const id = configStore.channels[configStore.i]?.id
        const wanted = currentSignature()

        if (!id || wanted === signature.value) {
            return
        }

        const clips = playlistStore.playlist.map((item) => ({
            source: item.source,
            title: item.title || null,
            ad: Boolean(item.ad),
            length: Math.max(0, item.out - item.in),
        }))

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

        if (entry) {
            return entry
        }

        const folder = parentFolder(item.source)

        return {
            key: item.title || folder || item.source,
            title: item.title || folder || fileName(item.source),
            sub_title: null,
            is_break: Boolean(item.ad),
            insert: false,
        }
    }

    const groups = computed<ProgrammeGroup[]>(() => {
        const list: ProgrammeGroup[] = []

        playlistStore.playlist.forEach((item, index) => {
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
    })

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

    return { entries, groups, refresh, groupOf, roleOf }
})
