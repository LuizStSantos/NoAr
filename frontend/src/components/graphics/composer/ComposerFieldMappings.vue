<script setup lang="ts">
import { useI18n } from 'vue-i18n'

import { type DataField, type GraphicsSource, sourceBindings, sourceColumns } from '@/composables/graphicsSources'
import type { GraphicsTemplate } from '@/composables/graphicsTemplates'

/** Field inputs emit bindings; only the editor applies them to layers and records undo. */
const props = defineProps<{
    fields: string[]
    template: GraphicsTemplate
    sources: GraphicsSource[]
}>()
const emit = defineEmits<{ map: [field: string, binding?: DataField] }>()
const { t } = useI18n()

function selectFieldSource(field: string, event: Event) {
    const id = Number((event.target as HTMLSelectElement).value)
    const source = props.sources.find((source) => source.id === id)
    emit('map', field, source ? { source_id: id, column: sourceColumns(source)[0] ?? '', row: 0 } : undefined)
}

function changeFieldMapping(field: string, changes: Partial<DataField>) {
    const current = sourceBindings(props.template)[field]

    if (current) {
        emit('map', field, { ...current, ...changes })
    }
}
</script>
<template>
    <fieldset v-if="fields.length" class="basis-full w-full border-t border-line pt-4 flex flex-col gap-3 min-w-0">
        <legend class="font-semibold text-sm">{{ t('graphics.sources.mapping') }}</legend>
        <p class="text-xs text-muted m-0">{{ t('graphics.sources.mappingHint') }}</p>
        <div v-for="field in fields" :key="field" class="grid sm:grid-cols-3 gap-2 min-w-0">
            <label class="field-label"
                >{{ t('graphics.sources.fieldSource', { field })
                }}<select
                    class="field-input"
                    :value="sourceBindings(template)[field]?.source_id ?? ''"
                    @change="selectFieldSource(field, $event)"
                >
                    <option value="">{{ t('graphics.sources.manual') }}</option>
                    <option v-for="source in sources" :key="source.id" :value="source.id">
                        {{ source.name }}
                    </option>
                </select></label
            >
            <template v-if="sourceBindings(template)[field]">
                <label class="field-label"
                    >{{ t('graphics.sources.column', { field })
                    }}<select
                        class="field-input"
                        :value="sourceBindings(template)[field]?.column"
                        @change="
                            changeFieldMapping(field, {
                                column: ($event.target as HTMLSelectElement).value,
                            })
                        "
                    >
                        <option
                            v-for="column in sourceColumns(
                                sources.find((source) => source.id === sourceBindings(template)[field]?.source_id)!,
                            )"
                            :key="column"
                            :value="column"
                        >
                            {{ column }}
                        </option>
                    </select></label
                >
                <label class="field-label"
                    >{{ t('graphics.sources.record', { field })
                    }}<input
                        class="field-input"
                        type="number"
                        min="1"
                        max="200"
                        :value="(sourceBindings(template)[field]?.row ?? 0) + 1"
                        @change="
                            changeFieldMapping(field, {
                                row: Number(($event.target as HTMLInputElement).value) - 1,
                            })
                        "
                /></label>
            </template>
        </div>
        <slot />
    </fieldset>
</template>
