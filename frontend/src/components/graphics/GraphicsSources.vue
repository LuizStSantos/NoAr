<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { authFetch } from '@/composables/authFetch'
import { type GraphicsSource, type SourceInput, sourceColumns, useGraphicsSources } from '@/composables/graphicsSources'
import GenericModal from '@/components/utils/GenericModal.vue'
import { useConfig } from '@/stores/config'

const { t } = useI18n()
const config = useConfig()
const { sources, unavailable, refresh } = useGraphicsSources()
const channel = computed(() => config.channels[config.i]?.id)
const selected = ref<number>()
const csvExample = 'symbol,price\nUSD,5.10'
const blank = (): SourceInput => ({
    name: '',
    kind: 'csv',
    url: '',
    interval: 30,
    records_path: '',
    document: '',
    revision: 0,
})
const form = ref<SourceInput>(blank())
const busy = ref(false)
const error = ref('')
const note = ref('')
const savedWithoutData = ref(false)
const removing = ref<GraphicsSource>()

function edit(source?: GraphicsSource) {
    selected.value = source?.id
    form.value = source
        ? {
              name: source.name,
              kind: source.kind,
              url: source.url,
              interval: source.interval,
              records_path: source.records_path,
              revision: source.revision,
              document: '',
          }
        : blank()
    error.value = ''
    note.value = ''
}
async function upload(event: Event) {
    const input = event.target as HTMLInputElement
    const file = input.files?.[0]
    input.value = ''
    if (!file) {
        return
    }
    if (file.size > 1024 * 1024) {
        error.value = t('graphics.sources.fileLimit')
        return
    }
    form.value.kind = 'csv'
    form.value.url = ''
    form.value.document = await file.text()
}
async function save() {
    busy.value = true
    error.value = ''
    note.value = ''
    try {
        const source = await authFetch<GraphicsSource>(
            `/api/graphics/${channel.value}/sources${selected.value ? '/' + selected.value : ''}`,
            {
                method: selected.value ? 'PUT' : 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(form.value),
            },
        )
        await refresh()
        edit(source)
        savedWithoutData.value = !!source.error || !source.rows.length
        note.value = savedWithoutData.value ? t('graphics.sources.savedUnavailable') : t('graphics.sources.saved')
    } catch (failure) {
        error.value = String(failure)
    } finally {
        busy.value = false
    }
}
async function fetchNow(source: GraphicsSource) {
    busy.value = true
    error.value = ''
    try {
        await authFetch(`/api/graphics/${channel.value}/sources/${source.id}/refresh`, { method: 'POST' })
        await refresh()
    } catch (failure) {
        error.value = String(failure)
    } finally {
        busy.value = false
    }
}
async function remove(confirmed: boolean) {
    const source = removing.value
    removing.value = undefined
    if (!confirmed || !source) {
        return
    }
    try {
        await authFetch(`/api/graphics/${channel.value}/sources/${source.id}`, { method: 'DELETE' })
        if (selected.value === source.id) {
            edit()
        }
        await refresh()
    } catch (failure) {
        error.value = String(failure)
    }
}
function when(value: number | null) {
    return value === null ? t('graphics.sources.never') : new Date(value).toLocaleString()
}
</script>
<template>
    <section class="flex flex-col gap-4" :aria-label="t('graphics.sections.sources')">
        <div>
            <h1 class="page-title">{{ t('graphics.sections.sources') }}</h1>
            <p class="text-sm text-muted">{{ t('graphics.sources.intro') }}</p>
        </div>
        <p v-if="unavailable" role="status" class="text-error">{{ t('graphics.sources.unavailable') }}</p>
        <p v-if="error" role="alert" class="text-error break-words">{{ error }}</p>
        <p v-if="note" role="status" :class="savedWithoutData ? 'text-warning' : 'text-success'">{{ note }}</p>
        <div class="grid xl:grid-cols-2 gap-4 min-w-0">
            <form class="card-box p-4 flex flex-col gap-3 min-w-0" @submit.prevent="save">
                <div class="flex items-center justify-between">
                    <h2 class="text-base font-semibold m-0">
                        {{ selected ? t('graphics.sources.edit') : t('graphics.sources.new') }}
                    </h2>
                    <button type="button" class="btn-quiet" @click="edit()">{{ t('graphics.sources.new') }}</button>
                </div>
                <label class="field-label"
                    >{{ t('graphics.sources.name')
                    }}<input v-model="form.name" class="field-input" maxlength="80" required
                /></label>
                <label class="field-label"
                    >{{ t('graphics.sources.kind')
                    }}<select v-model="form.kind" class="field-input">
                        <option value="csv">CSV / Google Sheets</option>
                        <option value="rss">RSS / Atom</option>
                        <option value="json">HTTP JSON</option>
                    </select></label
                >
                <label class="field-label"
                    >{{ t('graphics.sources.url')
                    }}<input v-model="form.url" class="field-input" type="url" placeholder="https://…"
                /></label>
                <p class="text-xs text-muted m-0">{{ t('graphics.sources.publicHint') }}</p>
                <template v-if="form.kind === 'csv'">
                    <label class="field-label"
                        >{{ t('graphics.sources.upload')
                        }}<input class="field-input" type="file" accept=".csv,text/csv" @change="upload"
                    /></label>
                    <label class="field-label"
                        >{{ t('graphics.sources.csv')
                        }}<textarea
                            v-model="form.document"
                            class="field-input min-h-28 font-mono"
                            :placeholder="selected ? t('graphics.sources.keepCsv') : csvExample"
                        />
                    </label>
                </template>
                <label v-if="form.kind === 'json'" class="field-label"
                    >{{ t('graphics.sources.path')
                    }}<input v-model="form.records_path" class="field-input" placeholder="/data/items"
                /></label>
                <label class="field-label"
                    >{{ t('graphics.sources.interval')
                    }}<input
                        v-model.number="form.interval"
                        type="number"
                        min="5"
                        max="3600"
                        class="field-input"
                        required
                /></label>
                <p class="text-xs text-muted m-0">{{ t('graphics.sources.pollHint') }}</p>
                <button class="btn-main" type="submit" :disabled="busy">{{ t('graphics.sources.save') }}</button>
            </form>
            <div class="flex flex-col gap-4 min-w-0">
                <p v-if="!sources.length" class="card-box p-4 text-muted">{{ t('graphics.sources.empty') }}</p>
                <article v-for="source in sources" :key="source.id" class="card-box p-4 min-w-0 flex flex-col gap-3">
                    <h2 class="text-base font-semibold m-0 break-words">{{ source.name }}</h2>
                    <p class="text-xs text-muted m-0">
                        {{ source.kind.toUpperCase() }} · {{ source.interval }} s ·
                        {{ t('graphics.sources.lastSuccess') }}: {{ when(source.last_success) }}
                    </p>
                    <p v-if="source.error" role="status" class="text-warning text-sm m-0 break-words">
                        {{ t(source.last_success ? 'graphics.sources.lastGood' : 'graphics.sources.noData') }}
                        {{ source.error }}
                    </p>
                    <div class="flex flex-wrap gap-2">
                        <button class="btn-quiet" :disabled="busy" @click="edit(source)">
                            {{ t('graphics.sources.edit') }}</button
                        ><button class="btn-quiet" :disabled="busy" @click="fetchNow(source)">
                            {{ t('graphics.sources.refresh') }}</button
                        ><button class="btn-quiet" :disabled="busy" @click="removing = source">
                            {{ t('graphics.sources.delete') }}
                        </button>
                    </div>
                    <div v-if="source.rows.length" class="overflow-auto max-h-64 min-w-0">
                        <table class="text-xs w-full">
                            <thead>
                                <tr>
                                    <th
                                        v-for="column in sourceColumns(source)"
                                        :key="column"
                                        class="text-left p-2 border-b border-line"
                                    >
                                        {{ column }}
                                    </th>
                                </tr>
                            </thead>
                            <tbody>
                                <tr v-for="(row, index) in source.rows.slice(0, 10)" :key="index">
                                    <td
                                        v-for="column in sourceColumns(source)"
                                        :key="column"
                                        class="p-2 border-b border-line max-w-60 truncate"
                                    >
                                        {{ row[column] }}
                                    </td>
                                </tr>
                            </tbody>
                        </table>
                    </div>
                    <p class="text-xs text-muted m-0">
                        {{ t('graphics.sources.recordCount', { count: source.rows.length }) }}
                    </p>
                </article>
            </div>
        </div>
        <GenericModal
            :show="!!removing"
            :title="t('graphics.sources.deleteTitle')"
            :ok-label="t('graphics.sources.delete')"
            danger
            :modal-action="remove"
            ><p class="max-w-sm">{{ t('graphics.sources.deleteHint') }}</p></GenericModal
        >
    </section>
</template>
