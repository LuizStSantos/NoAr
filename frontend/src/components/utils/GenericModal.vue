<script setup lang="ts">
import { onBeforeUnmount, watch } from 'vue'
import { useI18n } from 'vue-i18n'

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

function onKey(event: KeyboardEvent) {
    if (event.key === 'Escape' && props.show) {
        props.modalAction(false)
    }
}

watch(
    () => props.show,
    (show) => {
        document.body.style.overflow = show ? 'hidden' : ''

        if (show) {
            window.addEventListener('keydown', onKey)
        } else {
            window.removeEventListener('keydown', onKey)
        }
    },
    { immediate: true },
)

onBeforeUnmount(() => {
    document.body.style.overflow = ''
    window.removeEventListener('keydown', onKey)
})
</script>
<template>
    <div v-if="show" class="z-50 fixed inset-0 flex justify-center bg-black/50 overflow-auto py-5 px-4">
        <div
            role="dialog"
            aria-modal="true"
            :aria-label="title"
            class="flex flex-col bg-base-100 border border-line-strong min-w-[min(330px,100%)] w-auto max-w-full sm:max-w-[90%] rounded-box p-5 shadow-2xl my-auto"
        >
            <div class="flex items-center gap-2">
                <h2 class="m-0 font-semibold text-[16px] truncate flex-1 w-0">{{ title }}</h2>
                <button v-if="hideButtons" class="btn-icon" :aria-label="t('cancel')" @click="modalAction(false)">
                    <i class="bi bi-x-lg" aria-hidden="true" />
                </button>
            </div>

            <div class="grow mt-3">
                <slot>
                    <div v-html="text" />
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
