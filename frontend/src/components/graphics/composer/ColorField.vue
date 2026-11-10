<script setup lang="ts">
import { computed, useId } from 'vue'
import { useI18n } from 'vue-i18n'

/**
 * A colour as `#rrggbb` or `#rrggbbaa`: the browser's colour picker next to
 * the code, which can still be typed or pasted. The picker has no opacity, so
 * choosing a colour keeps the code's alpha. With `clearable`, empty means
 * none (transparent).
 */
const props = defineProps<{ label: string; value: string | null | undefined; clearable?: boolean }>()
const emit = defineEmits<{ change: [value: string | null] }>()
const { t } = useI18n()
const id = useId()

const HEX = /^#([0-9a-f]{3}|[0-9a-f]{6}|[0-9a-f]{8})$/i

/** The picker's `#rrggbb`, from `#rgb`, `#rrggbb` or `#rrggbbaa`. */
const pickerValue = computed(() => {
    const value = props.value?.trim() ?? ''

    if (!HEX.test(value)) {
        return '#000000'
    }

    const digits = value.slice(1)

    return digits.length === 3
        ? `#${[...digits].map((digit) => digit + digit).join('')}`.toLowerCase()
        : `#${digits.slice(0, 6)}`.toLowerCase()
})

const alpha = computed(() => {
    const value = props.value?.trim() ?? ''

    return HEX.test(value) && value.length === 9 ? value.slice(7) : ''
})

function pick(value: string) {
    emit('change', value + alpha.value)
}

function type(value: string) {
    const text = value.trim()
    emit('change', text || (props.clearable ? null : pickerValue.value))
}
</script>
<template>
    <div class="field-label">
        <label :for="id">{{ label }}</label>
        <div class="flex items-center gap-2">
            <input
                type="color"
                class="h-9 w-11 shrink-0 cursor-pointer rounded-field border border-line-strong bg-base-100 p-0.5"
                :value="pickerValue"
                :aria-label="t('graphics.composer.pickColor', { name: label })"
                @change="pick(($event.target as HTMLInputElement).value)"
            />
            <input
                :id="id"
                class="field-input min-w-0 flex-auto font-mono"
                :value="value ?? ''"
                placeholder="#rrggbb"
                spellcheck="false"
                @change="type(($event.target as HTMLInputElement).value)"
            />
            <button
                v-if="clearable && value"
                type="button"
                class="btn-icon min-h-9"
                :aria-label="t('graphics.composer.clearColor', { name: label })"
                :title="t('graphics.composer.clearColor', { name: label })"
                @click="emit('change', null)"
            >
                <i class="bi bi-x-lg" aria-hidden="true" />
            </button>
        </div>
    </div>
</template>
