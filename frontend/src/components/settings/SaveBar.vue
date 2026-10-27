<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import { useSettings } from '@/stores/settings'

const props = withDefaults(
    defineProps<{
        changes: number
        saving?: boolean
        saveLabel?: string
        /** Shown instead of "All saved". */
        note?: string
        /** Other pages with unsaved changes that are saved too. */
        alsoSaves?: string[]
        canDiscard?: boolean
    }>(),
    { saving: false, saveLabel: '', note: '', alsoSaves: () => [], canDiscard: true },
)

const emit = defineEmits<{ save: []; discard: [] }>()

const { t } = useI18n()
const settings = useSettings()

const message = computed(() => {
    if (props.changes > 0) {
        const text = t('settings.unsaved', props.changes)

        return props.alsoSaves.length
            ? `${text} ${t('settings.alsoSaves', { pages: props.alsoSaves.join(', ') })}`
            : text
    }

    if (props.alsoSaves.length) {
        return t('settings.otherPagesUnsaved', { pages: props.alsoSaves.join(', ') })
    }

    return props.note || t('settings.allSaved')
})

const dirty = computed(() => props.changes > 0 || props.alsoSaves.length > 0)

const bar = ref<HTMLElement>()

/**
 * Save only when every visible field of the page is valid (its min, max,
 * step, required…); else point at the first wrong one with the browser's own
 * message, so nothing out of range reaches the server or a playout restart.
 */
function save() {
    const page = bar.value?.parentElement
    const fields = page
        ? [
              ...page.querySelectorAll<HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement>(
                  'input, select, textarea',
              ),
          ]
        : []
    const invalid = fields.find((field) => field.offsetParent !== null && !field.disabled && !field.checkValidity())

    if (invalid) {
        invalid.reportValidity()
        invalid.focus()
        return
    }

    emit('save')
}
</script>
<template>
    <div ref="bar" class="flex flex-col gap-2" :class="dirty || settings.restartPending ? 'sticky bottom-3 z-10' : ''">
        <div
            v-if="settings.restartPending"
            class="bg-accent-soft border border-accent rounded-box px-4 py-3 flex flex-wrap items-center gap-3"
            role="status"
        >
            <i class="bi bi-arrow-repeat text-accent-text" aria-hidden="true" />
            <span class="flex-auto">{{ t('settings.restartPending') }}</span>
            <button class="btn-quiet" @click="settings.restartPending = false">{{ t('settings.later') }}</button>
            <button class="btn-main" @click="settings.restartPlayout()">{{ t('settings.restartNow') }}</button>
        </div>
        <div
            class="bg-raised border border-line-strong rounded-box px-4 py-3 flex flex-wrap items-center gap-3 shadow-[0_8px_24px_rgba(0,0,0,0.25)]"
        >
            <span class="dot" :class="dirty ? 'bg-accent' : 'bg-ok'" aria-hidden="true" />
            <span class="flex-auto min-w-0" :class="dirty ? 'text-accent-text' : 'text-muted'" role="status">
                {{ message }}
            </span>
            <slot name="extra" />
            <button v-if="canDiscard && dirty" class="btn-quiet" :disabled="saving" @click="emit('discard')">
                {{ t('settings.discard') }}
            </button>
            <button class="btn-main" :disabled="!dirty || saving" @click="save">
                <span v-if="saving" class="loading loading-spinner loading-xs" />
                {{ saveLabel || t('settings.save') }}
            </button>
        </div>
    </div>
</template>
