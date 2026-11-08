<script setup lang="ts">
import { useI18n } from 'vue-i18n'

import FolderTree from '@/components/storage/FolderTree.vue'
import GenericModal from '@/components/utils/GenericModal.vue'

import { baseName } from '@/composables/storage'

defineProps<{
    channelId: number
    current: string
    renameOpen: boolean
    showCreate: boolean
    deleteNames: string[]
    deleteHasFolder: boolean
    moveNames: string[]
}>()

const renameValue = defineModel<string>('renameValue', { required: true })
const newFolderName = defineModel<string>('newFolderName', { required: true })
const deleteRecursive = defineModel<boolean>('deleteRecursive', { required: true })
const moveTarget = defineModel<string>('moveTarget', { required: true })

const emit = defineEmits<{
    confirmRename: [confirmed: boolean]
    confirmCreate: [confirmed: boolean]
    confirmDelete: [confirmed: boolean]
    confirmMove: [confirmed: boolean]
}>()

const { t } = useI18n()
</script>
<template>
    <GenericModal
        :show="renameOpen"
        :title="t('storage.renameTitle')"
        :modal-action="(confirmed: boolean) => emit('confirmRename', confirmed)"
    >
        <label class="field-label w-[min(420px,80vw)]">
            {{ t('storage.newName') }}
            <input v-model="renameValue" class="field-input" autofocus @keydown.enter="emit('confirmRename', true)" />
        </label>
    </GenericModal>

    <GenericModal
        :show="showCreate"
        :title="t('storage.createTitle')"
        :modal-action="(confirmed: boolean) => emit('confirmCreate', confirmed)"
    >
        <label class="field-label w-[min(420px,80vw)]">
            {{ t('storage.folderName') }}
            <input v-model="newFolderName" class="field-input" autofocus @keydown.enter="emit('confirmCreate', true)" />
        </label>
    </GenericModal>

    <GenericModal
        :show="deleteNames.length > 0"
        :title="t('storage.deleteTitle')"
        :ok-label="t('storage.delete')"
        danger
        :modal-action="(confirmed: boolean) => emit('confirmDelete', confirmed)"
    >
        <div class="flex flex-col gap-3 w-[min(460px,80vw)]">
            <p class="m-0">{{ t('storage.deleteQuestion', { count: deleteNames.length }, deleteNames.length) }}</p>
            <ul class="m-0 ps-5 max-h-40 overflow-y-auto font-mono text-[13px] text-muted">
                <li v-for="name in deleteNames" :key="name">{{ name }}</li>
            </ul>
            <label v-if="deleteHasFolder" class="flex items-center gap-2.5 cursor-pointer">
                <input v-model="deleteRecursive" type="checkbox" class="checkbox checkbox-sm checkbox-accent" />
                {{ t('storage.deleteRecursive') }}
            </label>
        </div>
    </GenericModal>

    <GenericModal
        :show="moveNames.length > 0"
        :title="t('storage.moveTitle', { count: moveNames.length }, moveNames.length)"
        :ok-label="t('storage.moveHere')"
        :ok-disabled="moveTarget === current"
        :modal-action="(confirmed: boolean) => emit('confirmMove', confirmed)"
    >
        <div class="flex flex-col gap-2 w-[min(460px,80vw)]">
            <ul class="m-0 p-1 max-h-[50vh] overflow-y-auto border border-line rounded-field">
                <FolderTree
                    :channel-id="channelId"
                    path=""
                    :label="t('storage.root')"
                    :current="moveTarget"
                    @open="moveTarget = $event"
                />
            </ul>
            <p class="m-0 text-[13px] text-muted">
                {{ t('storage.moveTarget', { folder: moveTarget ? baseName(moveTarget) : t('storage.root') }) }}
            </p>
        </div>
    </GenericModal>
</template>
