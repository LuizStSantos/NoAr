<script setup lang="ts">
import { useId } from 'vue'

defineProps<{ title?: string; description?: string; danger?: boolean }>()

const id = useId()
</script>
<template>
    <section
        :aria-labelledby="title ? id : undefined"
        class="card-box px-5 py-4.5 flex flex-col gap-4"
        :class="danger ? 'border-[#5a2a2d]!' : ''"
    >
        <div v-if="title || $slots.aside" class="flex flex-wrap items-start justify-between gap-3">
            <div class="min-w-0 flex-[1_1_320px]">
                <h2 :id="id" class="m-0 text-base font-semibold">{{ title }}</h2>
                <p v-if="description" class="mt-0.5 mb-0 text-muted text-[13px] max-w-2xl">{{ description }}</p>
            </div>
            <slot name="aside" />
        </div>
        <slot />
    </section>
</template>
