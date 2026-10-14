<script setup lang="ts">
import { ref, onBeforeUnmount, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useEventSource } from '@vueuse/core'

import { stringFormatter } from '@/composables/helper'
import { useAuth } from '@/stores/auth'
import { useIndex } from '@/stores/index'
import { useConfig } from '@/stores/config'

const { fileSize } = stringFormatter()
const { t } = useI18n()
const authStore = useAuth()
const configStore = useConfig()
const indexStore = useIndex()

const streamUrl = ref(`/data/event/${configStore.channels[configStore.i]?.id}?endpoint=system&uuid=${authStore.uuid}`)

// 'http://127.0.0.1:8787/data/event/1?endpoint=system&uuid=f2f8c29b-712a-48c5-8919-b535d3a05a3a'
const { status, data, error, close } = useEventSource(streamUrl, [], {
    autoReconnect: {
        retries: -1,
        delay: 1000,
        onFailed() {
            indexStore.sseConnected = false
        },
    },
})

const errorCounter = ref(0)
const defaultStat = {
    cpu: { cores: 1, usage: 0.0 },
    load: { one: 0.0, five: 0.0, fifteen: 0.0 },
    memory: { total: 0.0, used: 0.0, process: 0.0 },
    network: { name: '...', current_in: 0.0, current_out: 0.0, total_in: 0.0, total_out: 0.0 },
    storage: { path: '', total: 0.0, free: 0.0 },
    swap: { total: 0.0, used: 0.0, free: 0.0 },
    system: { name: '...', kernel: '...', version: '...', ffp_version: '...' },
} as SystemStatistics

const sysStat = ref(defaultStat)

onBeforeUnmount(() => {
    close()
    indexStore.sseConnected = false
})

watch([status, error], async () => {
    if (status.value === 'OPEN') {
        indexStore.sseConnected = true
        errorCounter.value = 0
    } else {
        indexStore.sseConnected = false
        errorCounter.value += 1
        sysStat.value = defaultStat

        if (errorCounter.value > 15) {
            await authStore.obtainUuid()
            streamUrl.value = `/data/event/${configStore.channels[configStore.i]?.id}?endpoint=system&uuid=${
                authStore.uuid
            }`
            errorCounter.value = 0
        }
    }
})

watch([data], () => {
    if (data.value) {
        try {
            sysStat.value = JSON.parse(data.value)
        } catch {
            indexStore.sseConnected = true
        }
    }
})
</script>
<template>
    <div class="grid gap-4 grid-cols-1 sm:grid-cols-2 xl:grid-cols-3">
        <section class="card-box p-5 flex flex-col gap-1 sm:col-span-2 xl:col-span-3">
            <div class="text-lg font-semibold">{{ sysStat.system.name }} {{ sysStat.system.version }}</div>
            <div class="text-muted text-sm">
                <span v-if="sysStat.system.kernel">{{ sysStat.system.kernel }}</span>
                <span v-if="sysStat.system.ffp_version"> · NoAr v{{ sysStat.system.ffp_version }}</span>
            </div>
        </section>
        <section class="card-box p-5 flex flex-col gap-3">
            <h2 class="m-0 text-base font-semibold">{{ t('system.cpu') }}</h2>
            <div class="font-mono text-[28px] tabular">{{ sysStat.cpu.usage.toFixed(1) }}%</div>
            <div class="h-2 rounded bg-base-300 overflow-hidden">
                <div class="h-full bg-accent" :style="{ width: `${Math.min(100, sysStat.cpu.usage)}%` }" />
            </div>
            <div class="text-sm text-muted">
                {{ t('system.cores') }}: {{ sysStat.cpu.cores }} · {{ t('system.load') }}:
                <span class="font-mono">{{ sysStat.load.one }} {{ sysStat.load.five }} {{ sysStat.load.fifteen }}</span>
            </div>
        </section>
        <section class="card-box p-5 flex flex-col gap-3">
            <h2 class="m-0 text-base font-semibold">{{ t('system.memory') }}</h2>
            <div class="font-mono text-[28px] tabular">{{ fileSize(sysStat.memory.used) }}</div>
            <div class="h-2 rounded bg-base-300 overflow-hidden">
                <div
                    class="h-full bg-accent"
                    :style="{
                        width: `${sysStat.memory.total ? (sysStat.memory.used / sysStat.memory.total) * 100 : 0}%`,
                    }"
                />
            </div>
            <div class="text-sm text-muted">
                {{ t('system.total') }}: {{ fileSize(sysStat.memory.total) }} · {{ t('system.process') }}:
                {{ fileSize(sysStat.memory.process) }} · {{ t('system.swap') }}: {{ fileSize(sysStat.swap.used) }}
            </div>
        </section>
        <section class="card-box p-5 flex flex-col gap-3">
            <h2 class="m-0 text-base font-semibold">
                {{ t('system.network') }} <span class="text-muted font-normal">{{ sysStat.network?.name }}</span>
            </h2>
            <div class="grid grid-cols-2 gap-2">
                <div>
                    <div class="text-xs text-muted">{{ t('system.in') }}</div>
                    <div class="font-mono text-lg">{{ fileSize(sysStat.network?.current_in) }}/s</div>
                    <div class="text-xs text-subtle">{{ fileSize(sysStat.network?.total_in) }}</div>
                </div>
                <div>
                    <div class="text-xs text-muted">{{ t('system.out') }}</div>
                    <div class="font-mono text-lg">{{ fileSize(sysStat.network?.current_out) }}/s</div>
                    <div class="text-xs text-subtle">{{ fileSize(sysStat.network?.total_out) }}</div>
                </div>
            </div>
        </section>
        <section v-if="sysStat.storage?.path" class="card-box p-5 flex flex-col gap-3 sm:col-span-2 xl:col-span-3">
            <h2 class="m-0 text-base font-semibold">{{ t('system.storage') }}</h2>
            <div class="h-2 rounded bg-base-300 overflow-hidden">
                <div
                    class="h-full bg-accent"
                    :style="{
                        width: `${sysStat.storage.total ? ((sysStat.storage.total - sysStat.storage.free) / sysStat.storage.total) * 100 : 0}%`,
                    }"
                />
            </div>
            <div class="text-sm text-muted">
                <span class="font-mono">{{ sysStat.storage.path }}</span> · {{ t('system.size') }}:
                {{ fileSize(sysStat.storage.total) }} · {{ t('system.free') }}: {{ fileSize(sysStat.storage.free) }}
            </div>
        </section>
    </div>
</template>
