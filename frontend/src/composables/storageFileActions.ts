import { computed, ref, type Ref } from 'vue'
import { useI18n } from 'vue-i18n'

import { useConfig } from '@/stores/config'
import { useIndex } from '@/stores/index'

import { authFetch } from '@/composables/authFetch'
import { downloadStorageFile } from '@/composables/fileAccess'
import { storageJoin } from '@/composables/storage'

export interface StorageRow {
    name: string
    folder: boolean
    file?: FileObject
}

interface StorageFileActionOptions {
    channelId: Readonly<Ref<number>>
    current: Readonly<Ref<string>>
    selected: Ref<string[]>
    folders: () => readonly Folder[]
    refresh: () => Promise<void>
}

/** Own file mutations and their dialogs; the page remains the folder/selection owner. */
export function useStorageFileActions({ channelId, current, selected, folders, refresh }: StorageFileActionOptions) {
    const configStore = useConfig()
    const indexStore = useIndex()
    const { t } = useI18n()
    const downloading = ref(false)
    const renameTarget = ref<StorageRow | null>(null)
    const renameValue = ref('')
    const showCreate = ref(false)
    const newFolderName = ref('')
    const deleteNames = ref<string[]>([])
    const deleteRecursive = ref(false)
    const moveNames = ref<string[]>([])
    const moveTarget = ref('')

    function failed(error: any) {
        indexStore.msgAlert('error', t('storage.actionFailed', { error: error?.data ?? error?.message ?? error }), 6)
    }

    async function move(names: string[], target: string) {
        const from = current.value

        if (target === from) {
            indexStore.msgAlert('warning', t('storage.moveSame'), 3)
            return
        }

        try {
            for (const name of names) {
                if (storageJoin(from, name) === target || target.startsWith(`${storageJoin(from, name)}/`)) {
                    continue
                }

                await authFetch(`/api/file/${channelId.value}/rename`, {
                    method: 'POST',
                    headers: configStore.contentType,
                    body: JSON.stringify({
                        source: `/${storageJoin(from, name)}`,
                        target: `/${storageJoin(target, name)}`,
                    }),
                })
            }
        } catch (e) {
            failed(e)
        }

        await refresh()
    }

    // Rename, create, delete and move dialogs.
    function startRename(row: StorageRow) {
        renameTarget.value = row
        renameValue.value = row.name
    }

    async function finishRename(confirmed: boolean) {
        const row = renameTarget.value
        const name = renameValue.value.trim()

        renameTarget.value = null

        if (!confirmed || !row || !name || name === row.name) {
            return
        }

        try {
            await authFetch(`/api/file/${channelId.value}/rename`, {
                method: 'POST',
                headers: configStore.contentType,
                body: JSON.stringify({
                    source: `/${storageJoin(current.value, row.name)}`,
                    target: `/${storageJoin(current.value, name)}`,
                }),
            })
        } catch (e) {
            failed(e)
        }

        await refresh()
    }

    async function finishCreate(confirmed: boolean) {
        const name = newFolderName.value.trim()

        showCreate.value = false
        newFolderName.value = ''

        if (!confirmed || !name) {
            return
        }

        if (folders().some((folder) => folder.name === name)) {
            indexStore.msgAlert('warning', t('storage.folderExists'), 3)
            return
        }

        try {
            await authFetch(`/api/file/${channelId.value}/create-folder`, {
                method: 'POST',
                headers: configStore.contentType,
                body: JSON.stringify({ source: `/${storageJoin(current.value, name)}` }),
            })
            indexStore.msgAlert('success', t('storage.folderCreated'), 2)
        } catch (e) {
            failed(e)
        }

        await refresh()
    }

    const deleteHasFolder = computed(() =>
        deleteNames.value.some((name) => folders().some((folder) => folder.name === name)),
    )

    async function finishDelete(confirmed: boolean) {
        const names = deleteNames.value
        const recursive = deleteRecursive.value

        deleteNames.value = []
        deleteRecursive.value = false

        if (!confirmed) {
            return
        }

        try {
            for (const name of names) {
                await authFetch(`/api/file/${channelId.value}/remove`, {
                    method: 'POST',
                    headers: configStore.contentType,
                    body: JSON.stringify({ source: `/${storageJoin(current.value, name)}`, recursive }),
                })
            }
        } catch (e) {
            failed(e)
        }

        selected.value = selected.value.filter((name) => !names.includes(name))
        await refresh()
    }

    function startMove(names: string[]) {
        moveNames.value = names
        moveTarget.value = current.value
    }

    async function finishMove(confirmed: boolean) {
        const names = moveNames.value

        moveNames.value = []

        if (confirmed && names.length) {
            await move(names, moveTarget.value)
        }
    }

    async function downloadFile(row: StorageRow) {
        if (row.folder || downloading.value) {
            return
        }

        const id = channelId.value
        const path = `/${storageJoin(current.value, row.name)}`
        downloading.value = true

        try {
            await downloadStorageFile(id, path, row.name)
        } catch (error) {
            failed(error)
        } finally {
            downloading.value = false
        }
    }

    return {
        renameTarget,
        renameValue,
        showCreate,
        newFolderName,
        deleteNames,
        deleteRecursive,
        deleteHasFolder,
        moveNames,
        moveTarget,
        downloading,
        failed,
        move,
        startRename,
        finishRename,
        finishCreate,
        finishDelete,
        startMove,
        finishMove,
        downloadFile,
    }
}
