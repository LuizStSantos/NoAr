<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'

import LiveFields from '@/components/live/LiveFields.vue'
import { useIndex } from '@/stores/index'

/**
 * Create a live from Storage, or change an existing one: its address and the
 * guide data of its folder. Scheduling stays in the Scheduler.
 */
const props = defineProps<{
    channelId: number
    /** The live to change; a new one is created in `Lives/` when missing. */
    live: LiveSource | null
    /** The live folder's sheet, to start from when changing it. */
    sheet: MediaSheet | null
}>()

const emit = defineEmits<{
    close: []
    saved: [live: LiveSource, schedule: boolean]
}>()

const { t } = useI18n()
const indexStore = useIndex()

const form = ref<InstanceType<typeof LiveFields>>()
const saving = ref(false)
const error = ref('')

async function save(schedule: boolean) {
    error.value = form.value?.check() ?? ''

    if (error.value || !form.value) {
        return
    }

    saving.value = true

    try {
        const live = await form.value.submit()

        indexStore.msgAlert('success', t('storage.live.saved'), 2)
        emit('saved', live, schedule)
    } catch (err) {
        error.value = err instanceof Error ? err.message : String(err)
    } finally {
        saving.value = false
    }
}
</script>

<template>
    <div class="z-50 fixed inset-0 bg-black/50 overflow-auto flex justify-center items-start py-8 px-3">
        <div
            role="dialog"
            aria-modal="true"
            aria-labelledby="live-editor-title"
            class="w-full max-w-170 bg-base-100 border border-base-content/20 rounded-lg shadow-xl flex flex-col"
            @keydown.esc="emit('close')"
        >
            <div class="flex items-center justify-between px-5 py-3 border-b border-base-300">
                <h2 id="live-editor-title" class="text-lg font-semibold flex items-center gap-2 m-0">
                    <i class="bi bi-broadcast text-live" aria-hidden="true" />
                    {{ props.live ? t('storage.live.editTitle') : t('storage.live.newTitle') }}
                </h2>
                <button type="button" class="btn-icon" :aria-label="t('schedule.close')" @click="emit('close')">
                    <i class="bi bi-x-lg" aria-hidden="true" />
                </button>
            </div>

            <div class="px-5 py-4 flex flex-col gap-5">
                <LiveFields ref="form" :channel-id="channelId" :live="live" :sheet="sheet" />
                <p v-if="!live" class="m-0 text-[13px] text-muted">{{ t('storage.live.scheduleLater') }}</p>
                <p v-if="error" class="m-0 text-error text-sm" role="alert">{{ error }}</p>
            </div>

            <div class="flex flex-wrap justify-end gap-2 px-5 py-3 border-t border-base-300">
                <button type="button" class="btn-quiet" @click="emit('close')">{{ t('cancel') }}</button>
                <button type="button" class="btn-quiet" :disabled="saving" @click="save(true)">
                    {{ t('storage.live.saveAndSchedule') }}
                </button>
                <button type="button" class="btn-main" :disabled="saving" @click="save(false)">
                    <span v-if="saving" class="loading loading-spinner loading-sm" />
                    {{ live ? t('storage.live.save') : t('storage.live.saveNew') }}
                </button>
            </div>
        </div>
    </div>
</template>
