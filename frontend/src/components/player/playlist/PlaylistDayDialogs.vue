<script setup lang="ts">
import { useI18n } from 'vue-i18n'

import GenericModal from '@/components/utils/GenericModal.vue'

/** Confirmation UI emits decisions; whole-day operations stay in usePlaylistDayActions. */
defineProps<{ dateLabel: string; shortDateLabel: string; unsaved: number }>()
const showCopy = defineModel<boolean>('showCopy', { required: true })
const copyDate = defineModel<string>('copyDate', { required: true })
const showImport = defineModel<boolean>('showImport', { required: true })
const importFile = defineModel<File | null>('importFile', { required: true })
const showDelete = defineModel<boolean>('showDelete', { required: true })
const showDiscard = defineModel<boolean>('showDiscard', { required: true })
const pendingDate = defineModel<string | null>('pendingDate', { required: true })
const emit = defineEmits<{
    copy: [confirmed: boolean]
    import: [confirmed: boolean]
    remove: [confirmed: boolean]
    discard: [confirmed: boolean]
    changeDate: [confirmed: boolean]
}>()
const { t } = useI18n()
</script>
<template>
    <GenericModal
        :show="showCopy"
        :title="t('onair.copyToDay')"
        :ok-label="t('onair.copy')"
        :ok-disabled="!copyDate"
        :modal-action="(confirmed: boolean) => emit('copy', confirmed)"
    >
        <div class="flex flex-col gap-3 w-[min(380px,85vw)]">
            <label class="field-label">
                {{ t('onair.targetDay') }}
                <input v-model="copyDate" type="date" class="field-input" />
            </label>
            <p class="m-0 text-sm text-muted">{{ t('onair.copyHint') }}</p>
        </div>
    </GenericModal>

    <GenericModal
        :show="showImport"
        :title="t('onair.importList')"
        :ok-label="t('onair.import')"
        :ok-disabled="!importFile"
        :modal-action="(confirmed: boolean) => emit('import', confirmed)"
    >
        <div class="flex flex-col gap-3 w-[min(420px,85vw)]">
            <input
                type="file"
                accept=".m3u,.m3u8,.txt,text/plain"
                class="file-input file-input-sm w-full"
                @change="importFile = ($event.target as HTMLInputElement).files?.[0] ?? null"
            />
            <p class="m-0 text-sm text-muted">{{ t('onair.importHint', { date: dateLabel }) }}</p>
        </div>
    </GenericModal>

    <GenericModal
        :show="showDelete"
        :title="t('onair.deleteList', { date: shortDateLabel })"
        :ok-label="t('onair.delete')"
        danger
        :modal-action="(confirmed: boolean) => emit('remove', confirmed)"
    >
        <p class="m-0 max-w-sm">{{ t('onair.deleteQuestion', { date: dateLabel }) }}</p>
    </GenericModal>

    <GenericModal
        :show="showDiscard"
        :title="t('onair.discardTitle')"
        :ok-label="t('onair.discard')"
        danger
        :modal-action="(confirmed: boolean) => emit('discard', confirmed)"
    >
        <p class="m-0 max-w-sm">{{ t('onair.discardQuestion', unsaved) }}</p>
    </GenericModal>

    <GenericModal
        :show="pendingDate !== null"
        :title="t('onair.unsavedTitle')"
        :ok-label="t('onair.discardAndContinue')"
        danger
        :modal-action="(confirmed: boolean) => emit('changeDate', confirmed)"
    >
        <p class="m-0 max-w-sm">{{ t('onair.unsavedQuestion', unsaved) }}</p>
    </GenericModal>
</template>
