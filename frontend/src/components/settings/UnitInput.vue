<script setup lang="ts">
defineOptions({ inheritAttrs: false })
defineProps<{ unit: string }>()

const model = defineModel<string | number | null>()
</script>
<template>
    <span class="relative flex items-center">
        <input v-model="model" v-bind="$attrs" class="field-input font-mono pe-14" />
        <span class="absolute end-3 text-muted text-xs pointer-events-none">{{ unit }}</span>
    </span>
</template>
