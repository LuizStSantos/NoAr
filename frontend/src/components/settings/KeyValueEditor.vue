<script setup lang="ts">
import { ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

const props = defineProps<{ label: string; hint?: string; disabled?: boolean }>()
const model = defineModel<Record<string, string>>({ default: () => ({}) })

const { t } = useI18n()

interface Row {
    key: string
    value: string
}

const rows = ref<Row[]>([])

function toRecord(list: Row[]): Record<string, string> {
    const record: Record<string, string> = {}

    for (const row of list) {
        const key = row.key.trim()

        if (key && !(key in record)) {
            record[key] = row.value
        }
    }

    return record
}

watch(
    model,
    (value) => {
        // Keep half-typed rows while the parent echoes our own update back.
        if (JSON.stringify(toRecord(rows.value)) !== JSON.stringify(value ?? {})) {
            rows.value = Object.entries(value ?? {}).map(([key, val]) => ({ key, value: String(val) }))
        }
    },
    { immediate: true, deep: true },
)

function emit() {
    model.value = toRecord(rows.value)
}

function add() {
    rows.value.push({ key: '', value: '' })
}

function remove(index: number) {
    rows.value.splice(index, 1)
    emit()
}
</script>
<template>
    <div class="flex flex-col gap-2">
        <span class="text-xs text-muted">{{ props.label }}</span>
        <div v-if="rows.length" class="grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto] gap-2">
            <template v-for="(row, index) in rows" :key="index">
                <input
                    v-model="row.key"
                    class="field-input font-mono text-[13px] min-h-9.5"
                    :aria-label="t('settings.optionName')"
                    :placeholder="t('settings.optionName')"
                    :disabled="disabled"
                    spellcheck="false"
                    @input="emit"
                />
                <input
                    v-model="row.value"
                    class="field-input font-mono text-[13px] min-h-9.5"
                    :aria-label="t('settings.optionValue')"
                    :placeholder="t('settings.optionValue')"
                    :disabled="disabled"
                    spellcheck="false"
                    @input="emit"
                />
                <button
                    class="btn-icon"
                    :aria-label="t('settings.removeOption', { name: row.key })"
                    :disabled="disabled"
                    @click="remove(index)"
                >
                    <i class="bi bi-x-lg" aria-hidden="true" />
                </button>
            </template>
        </div>
        <span v-if="hint" class="field-hint">{{ hint }}</span>
        <button v-if="!disabled" class="link-accent self-start min-h-8" @click="add">
            + {{ t('settings.addOption') }}
        </button>
    </div>
</template>
