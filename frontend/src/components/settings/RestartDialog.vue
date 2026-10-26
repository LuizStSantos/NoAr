<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'

import { useDialogFocus } from '@/composables/dialogFocus'

/** Asks before something that stops the output for a moment. */
const props = defineProps<{ show: boolean; title: string; text: string; hint?: string; busy?: boolean }>()
const emit = defineEmits<{ close: []; confirm: [] }>()

const { t } = useI18n()

const dialog = ref<HTMLElement>()

useDialogFocus(dialog, {
    open: () => props.show,
    onEscape: () => {
        if (!props.busy) {
            emit('close')
        }
    },
})
</script>
<template>
    <div
        v-if="show"
        class="z-50 fixed inset-0 flex items-center justify-center bg-black/55 px-4"
        @click.self="!busy && emit('close')"
    >
        <section
            ref="dialog"
            role="alertdialog"
            aria-modal="true"
            aria-labelledby="restart-title"
            aria-describedby="restart-text"
            tabindex="-1"
            class="outline-none w-full max-w-[480px] bg-base-100 border border-line-strong rounded-[16px] shadow-2xl"
        >
            <div class="flex gap-3.5 items-start px-5.5 pt-5 pb-1.5">
                <span
                    class="flex-none w-10 h-10 rounded-[10px] bg-accent-soft text-accent-text flex items-center justify-center text-lg"
                    aria-hidden="true"
                >
                    <i class="bi bi-arrow-repeat" />
                </span>
                <div class="flex flex-col gap-1.5">
                    <h2 id="restart-title" class="m-0 text-lg font-semibold">{{ title }}</h2>
                    <p id="restart-text" class="m-0 text-base-content/85">{{ text }}</p>
                    <p v-if="hint" class="m-0 text-[13px] text-muted">{{ hint }}</p>
                </div>
            </div>
            <div class="flex justify-end gap-2.5 px-5.5 pt-4.5 pb-5">
                <button type="button" class="btn-quiet" :disabled="busy" @click="emit('close')">
                    {{ t('cancel') }}
                </button>
                <button type="button" class="btn-main" :disabled="busy" @click="emit('confirm')">
                    <span v-if="busy" class="loading loading-spinner loading-xs" />{{ t('settings.restart.now') }}
                </button>
            </div>
        </section>
    </div>
</template>
