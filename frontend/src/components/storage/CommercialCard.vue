<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'

import { baseName } from '@/composables/storage'

/** A commercials folder, or a folder inside one, in Storage. */
const props = defineProps<{
    /** Storage path of the open folder. */
    folder: string
    /** Storage path of the marked folder covering it (itself or a parent). */
    marked: string
    /** Videos of the open folder, already worded. */
    summary: string
    busy: boolean
}>()

const emit = defineEmits<{
    unmark: []
    open: [path: string]
}>()

const { t } = useI18n()

const own = computed(() => props.folder === props.marked)
</script>
<template>
    <section v-if="own" aria-labelledby="commercial-card-title" class="card-box p-4.5 flex flex-wrap gap-5 items-start">
        <div
            class="size-14 flex-none rounded-xl bg-after-clip/15 text-after-clip flex items-center justify-center"
            aria-hidden="true"
        >
            <i class="bi bi-megaphone text-2xl" />
        </div>
        <div class="flex-[1_1_420px] min-w-0 flex flex-col gap-2.5">
            <div class="flex flex-wrap items-center gap-3">
                <div class="flex-1 min-w-0">
                    <div class="text-xs text-after-clip uppercase tracking-[0.08em]">
                        {{ t('storage.commercial.kicker') }}
                    </div>
                    <h2 id="commercial-card-title" class="m-0 mt-0.5 text-[22px] font-semibold truncate">
                        {{ baseName(folder) }}
                    </h2>
                </div>
                <label class="flex items-center gap-2.5 min-h-10 cursor-pointer">
                    {{ t('storage.commercial.switch') }}
                    <input
                        type="checkbox"
                        role="switch"
                        class="toggle toggle-accent"
                        checked
                        :disabled="busy"
                        @change="emit('unmark')"
                    />
                </label>
            </div>
            <p class="m-0 text-base-content/85">{{ t('storage.commercial.intro') }}</p>
            <ul class="m-0 p-0 list-none flex flex-col gap-1.5">
                <li
                    v-for="key in ['guide', 'logo', 'everywhere']"
                    :key="key"
                    class="flex gap-2 items-start text-[13px] sm:text-sm"
                >
                    <i class="bi bi-check2 text-ok mt-px" aria-hidden="true" />{{ t(`storage.commercial.${key}`) }}
                </li>
            </ul>
            <span class="text-[13px] text-muted">
                {{ summary }} ·
                {{ t('storage.commercial.marker', { file: '.comercial' }) }}
            </span>
        </div>
    </section>

    <section v-else aria-labelledby="commercial-card-title" class="card-box p-4.5 flex flex-wrap items-center gap-4">
        <i class="bi bi-megaphone text-xl text-after-clip" aria-hidden="true" />
        <div class="flex-[1_1_280px] min-w-0 flex flex-col gap-1">
            <h2 id="commercial-card-title" class="m-0 text-base font-semibold">
                {{ t('storage.commercial.inherited', { folder: baseName(marked) }) }}
            </h2>
            <p class="m-0 text-muted">{{ t('storage.commercial.inheritedHint') }}</p>
        </div>
        <button type="button" class="btn-quiet" @click="emit('open', marked)">
            {{ t('storage.commercial.open', { folder: baseName(marked) }) }}
        </button>
    </section>
</template>
