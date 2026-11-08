import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { cloneDeep } from 'es-toolkit/object'

import { authFetch } from '@/composables/authFetch'
import { playlistOperations } from '@/composables/helper'
import { addDays } from '@/composables/schedule'
import { useAuth } from '@/stores/auth'
import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'
import { usePlaylist } from '@/stores/playlist'

/** Own whole-day persistence and its confirmation state, retaining all loading/save guards. */
export function usePlaylistDayActions(reload: (date: string) => Promise<void>, closeMenu: () => void) {
    const { t } = useI18n()
    const { genUID } = playlistOperations()
    const configStore = useConfig()
    const playlistStore = usePlaylist()
    const authStore = useAuth()
    const indexStore = useIndex()
    const channel = computed(() => configStore.channels[configStore.i])
    const saving = ref(false)
    const showCopy = ref(false)
    const copyDate = ref('')
    const showImport = ref(false)
    const importFile = ref<File | null>(null)
    const showDelete = ref(false)

    async function save(date = playlistStore.listDate) {
        // A list is still loading: what is on screen belongs to the day or the
        // channel before, and must not be written over the one being loaded.
        if (playlistStore.isLoading) {
            return
        }
        // Every clip removed: saving an empty day means deleting its playlist,
        // which asks first.
        if (!playlistStore.playlist.length) {
            if (date === playlistStore.listDate) {
                showDelete.value = true
            }
            return
        }
        if (playlistStore.loadFailed && date === playlistStore.listDate) {
            indexStore.msgAlert('error', t('onair.loadFailedNoSave'), 5)
            return
        }

        saving.value = true

        try {
            const { processPlaylist } = playlistOperations()
            const program = processPlaylist(date, cloneDeep(playlistStore.playlist), true).map((item) => {
                const copy = { ...item } as Partial<PlaylistItem>

                delete copy.uid
                delete copy.begin
                delete copy.overtime

                return copy
            })

            await authFetch(`/api/playlist/${channel.value?.id}`, {
                method: 'POST',
                headers: { ...configStore.contentType, ...authStore.authHeader },
                body: JSON.stringify({ channel: channel.value?.name, date, program }),
            })

            if (date === playlistStore.listDate) {
                playlistStore.markSaved()
            }

            indexStore.msgAlert('success', t('onair.saved', { date }), 2)
        } catch (e: any) {
            if (e?.status === 409) {
                if (date === playlistStore.listDate) {
                    playlistStore.markSaved()
                }

                indexStore.msgAlert('success', t('onair.alreadySaved'), 2)
            } else {
                indexStore.msgAlert('error', e?.data ?? e, 4)
            }
        } finally {
            saving.value = false
        }
    }

    const showDiscard = ref(false)

    function discard(confirm = true) {
        showDiscard.value = false

        if (confirm) {
            reload(playlistStore.listDate)
        }
    }

    function openCopy() {
        closeMenu()
        copyDate.value = addDays(playlistStore.listDate, 1)
        showCopy.value = true
    }

    async function copyTo(confirm: boolean) {
        showCopy.value = false

        if (confirm && copyDate.value) {
            await save(copyDate.value)
        }
    }

    async function importList(confirm: boolean) {
        showImport.value = false

        const file = importFile.value
        importFile.value = null

        if (!confirm || !file) {
            return
        }

        const formData = new FormData()
        formData.append(file.name, file)

        try {
            await authFetch(
                `/api/file/${channel.value?.id}/import?file=${encodeURIComponent(file.name)}&date=${playlistStore.listDate}`,
                { method: 'PUT', headers: authStore.authHeader, body: formData },
            )
            indexStore.msgAlert('success', t('onair.imported'), 2)
            await reload(playlistStore.listDate)
        } catch (e: any) {
            indexStore.msgAlert('error', e?.data ?? e, 4)
        }
    }

    function fillDay() {
        closeMenu()

        const original = playlistStore.playlist
        const target = configStore.playout.playlist.lengthInSec || 86400
        const filled: PlaylistItem[] = []
        let length = 0

        if (!original.length) {
            return
        }

        while (length < target) {
            for (const item of original) {
                if (length >= target) {
                    break
                }

                // Repeats do not take the marks of a single entry.
                filled.push({ ...cloneDeep(item), uid: genUID(), marks: undefined })
                length += item.out - item.in
            }
        }

        playlistStore.playlist = filled
        playlistStore.touch()
    }

    async function deleteList(confirm: boolean) {
        showDelete.value = false

        if (!confirm) {
            return
        }

        try {
            await authFetch(`/api/playlist/${channel.value?.id}/${playlistStore.listDate}`, {
                method: 'DELETE',
                headers: { ...configStore.contentType, ...authStore.authHeader },
            })
            playlistStore.playlist = []
            playlistStore.markSaved()
            indexStore.msgAlert('warning', t('onair.deleted'), 2)
        } catch (e: any) {
            indexStore.msgAlert('error', e?.data ?? e, 4)
        }
    }

    return {
        saving,
        showCopy,
        copyDate,
        showImport,
        importFile,
        showDelete,
        showDiscard,
        save,
        discard,
        openCopy,
        copyTo,
        importList,
        fillDay,
        deleteList,
    }
}
