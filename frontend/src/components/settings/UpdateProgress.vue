<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import { useAuth } from '@/stores/auth'

/**
 * While the updater installs the new version: NoAr answers while the image
 * downloads, stops while the container is replaced, and answers again from
 * a new start. The page reloads then.
 */
const props = defineProps<{
    /** `started_at` of the run that asked for the update. */
    startedAt: number
}>()

const emit = defineEmits<{ close: [] }>()

const { t } = useI18n()
const authStore = useAuth()

/** After this long without a new start, say how to look on the server. */
const LATE_MS = 3 * 60 * 1000
const POLL_MS = 1500

type Step = 'download' | 'restart' | 'back'

const step = ref<Step>('download')
const late = ref(false)
const deadline = Date.now() + LATE_MS
let timer: ReturnType<typeof setTimeout> | undefined

const steps = computed(() =>
    (['download', 'restart', 'back'] as const).map((name, index, all) => ({
        name,
        done: all.indexOf(step.value) > index || step.value === 'back',
        active: step.value === name && name !== 'back',
    })),
)

async function poll() {
    try {
        const response = await fetch('/api/system/server', { headers: authStore.authHeader })

        if (response.ok) {
            const info = await response.json()

            if (info.started_at !== props.startedAt) {
                step.value = 'back'
                timer = setTimeout(() => window.location.reload(), 1500)
                return
            }
        } else {
            step.value = 'restart'
        }
    } catch {
        // Not answering: the container is being replaced.
        step.value = 'restart'
    }

    if (Date.now() > deadline) {
        late.value = true
    }

    timer = setTimeout(poll, POLL_MS)
}

onMounted(() => (timer = setTimeout(poll, POLL_MS)))
onBeforeUnmount(() => clearTimeout(timer))
</script>
<template>
    <div class="z-50 fixed inset-0 flex items-center justify-center bg-black/70 px-4" role="presentation">
        <section
            role="dialog"
            aria-modal="true"
            aria-labelledby="update-progress"
            class="w-full max-w-[560px] bg-base-100 border border-line-strong rounded-[14px] shadow-xl px-6 py-5.5 flex flex-col gap-4"
        >
            <h2 id="update-progress" class="m-0 text-lg font-semibold">{{ t('settings.updates.progressTitle') }}</h2>
            <ol class="m-0 p-0 list-none flex flex-col gap-3" aria-live="polite">
                <li
                    v-for="entry in steps"
                    :key="entry.name"
                    class="flex gap-3 items-start"
                    :class="!entry.done && !entry.active ? 'text-muted' : ''"
                >
                    <span
                        v-if="entry.done"
                        class="w-5.5 h-5.5 rounded-full bg-ok/20 text-ok inline-flex items-center justify-center flex-none"
                        aria-hidden="true"
                    >
                        <i class="bi bi-check2 text-xs" />
                    </span>
                    <span
                        v-else-if="entry.active"
                        class="loading loading-spinner w-5.5 text-accent flex-none"
                        aria-hidden="true"
                    />
                    <span
                        v-else
                        class="w-5.5 h-5.5 rounded-full border border-line-strong flex-none"
                        aria-hidden="true"
                    />
                    <span class="flex flex-col">
                        <span :class="entry.active ? 'font-semibold' : ''">
                            {{ t(`settings.updates.step.${entry.name}`) }}
                        </span>
                        <span v-if="entry.name !== 'back'" class="text-muted text-[13px]">
                            {{ t(`settings.updates.step.${entry.name}Hint`) }}
                        </span>
                    </span>
                </li>
            </ol>
            <div v-if="late" class="flex flex-col gap-2 border-t border-line pt-3" role="status">
                <span class="text-accent-text font-semibold">{{ t('settings.updates.lateTitle') }}</span>
                <span class="text-[13px] text-base-content/85">{{ t('settings.updates.lateText') }}</span>
                <code
                    class="font-mono text-[13px] bg-base-200 border border-line-strong rounded-field px-3 py-2 break-all"
                >
                    docker compose logs --tail 50 noar-updater
                </code>
                <div class="flex justify-end">
                    <button type="button" class="btn-quiet" @click="emit('close')">{{ t('storage.close') }}</button>
                </div>
            </div>
            <p v-else class="m-0 text-muted text-[13px] border-t border-line pt-3">
                {{ t('settings.updates.progressHint') }}
            </p>
        </section>
    </div>
</template>
