<script setup lang="ts">
import { onBeforeUnmount, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import { useDialogFocus } from '@/composables/dialogFocus'

const { t } = useI18n()

const props = defineProps({
    title: {
        type: String,
        default: '',
    },
    text: {
        type: String,
        default: '',
    },
    modalAction: {
        type: Function,
        default() {
            return ''
        },
    },
    show: {
        type: Boolean,
        default: false,
    },
    hideButtons: {
        type: Boolean,
        default: false,
    },
    okLabel: {
        type: String,
        default: '',
    },
    cancelLabel: {
        type: String,
        default: '',
    },
    /** The confirm button deletes or discards something. */
    danger: {
        type: Boolean,
        default: false,
    },
    okDisabled: {
        type: Boolean,
        default: false,
    },
})

const dialog = ref<HTMLElement>()

useDialogFocus(dialog, { open: () => props.show, onEscape: () => props.modalAction(false) })

// Lock the page only while shown: a hidden modal must not unlock a dialog
// that is open at the same time.
watch(
    () => props.show,
    (show, wasShown) => {
        if (show) {
            document.body.style.overflow = 'hidden'
        } else if (wasShown) {
            document.body.style.overflow = ''
        }
    },
    { immediate: true },
)

onBeforeUnmount(() => {
    if (props.show) {
        document.body.style.overflow = ''
    }
})
</script>
<template>
    <div v-if="show" class="z-50 fixed inset-0 flex justify-center bg-black/50 overflow-auto py-5 px-4">
        <div
            ref="dialog"
            role="dialog"
            aria-modal="true"
            :aria-label="title"
            tabindex="-1"
            class="outline-none flex flex-col bg-base-100 border border-line-strong min-w-[min(330px,100%)] w-auto max-w-full sm:max-w-[90%] rounded-box p-5 shadow-2xl my-auto"
        >
            <div class="flex items-center gap-2">
                <h2 class="m-0 font-semibold text-[16px] truncate flex-1 w-0">{{ title }}</h2>
                <button v-if="hideButtons" class="btn-icon" :aria-label="t('cancel')" @click="modalAction(false)">
                    <i class="bi bi-x-lg" aria-hidden="true" />
                </button>
            </div>

            <div class="grow mt-3">
                <slot>
                    <!-- Plain text: the message can carry names typed by users (titles, files). -->
                    <p class="m-0 whitespace-pre-line [overflow-wrap:anywhere]">{{ text }}</p>
                </slot>
            </div>

            <div v-if="!hideButtons" class="flex justify-end gap-2 mt-4">
                <button class="btn-quiet" @click="modalAction(false)">
                    {{ cancelLabel || t('cancel') }}
                </button>
                <button
                    v-if="danger"
                    class="btn-quiet border-on-air! text-on-air-text! hover:bg-on-air-soft!"
                    :disabled="okDisabled"
                    @click="modalAction(true)"
                >
                    {{ okLabel || t('ok') }}
                </button>
                <button v-else class="btn-main" :disabled="okDisabled" @click="modalAction(true)">
                    {{ okLabel || t('ok') }}
                </button>
            </div>
        </div>
    </div>
</template>
