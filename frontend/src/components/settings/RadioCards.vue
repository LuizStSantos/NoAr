<script setup lang="ts">
import { useId } from 'vue'

defineProps<{
    options: { value: string; label: string; description?: string; disabled?: boolean }[]
    label: string
}>()

const model = defineModel<string>()
const name = useId()
</script>
<template>
    <div role="radiogroup" :aria-label="label" class="grid gap-3 grid-cols-[repeat(auto-fit,minmax(200px,1fr))]">
        <label
            v-for="option in options"
            :key="option.value"
            class="flex items-start gap-3 rounded-xl border px-4 py-3.5 min-h-11"
            :class="[
                model === option.value ? 'border-accent bg-accent-soft' : 'border-line-strong bg-field',
                option.disabled ? 'opacity-50 cursor-not-allowed' : 'cursor-pointer hover:border-subtle',
            ]"
        >
            <input
                v-model="model"
                type="radio"
                :name="name"
                :value="option.value"
                :disabled="option.disabled"
                class="radio radio-sm radio-accent mt-0.5"
            />
            <span class="flex flex-col gap-0.5">
                <span class="font-semibold">{{ option.label }}</span>
                <span v-if="option.description" class="text-[13px] text-muted">{{ option.description }}</span>
            </span>
        </label>
    </div>
</template>
