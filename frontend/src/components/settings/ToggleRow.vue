<script setup lang="ts">
defineProps<{ label: string; hint?: string; disabled?: boolean }>()

const model = defineModel<boolean>()
</script>
<template>
    <label
        class="flex items-start gap-3 min-h-10 cursor-pointer"
        :class="disabled ? 'opacity-60 cursor-not-allowed' : ''"
    >
        <input v-model="model" type="checkbox" role="switch" class="toggle toggle-accent mt-0.5" :disabled="disabled" />
        <span class="flex flex-col">
            <span>{{ label }}</span>
            <span v-if="hint" class="text-[13px] text-muted">{{ hint }}</span>
        </span>
    </label>
</template>
