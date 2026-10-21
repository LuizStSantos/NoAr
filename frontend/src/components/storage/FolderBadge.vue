<script setup lang="ts">
import { useI18n } from 'vue-i18n'

/** What a folder is for: commercials, a live, or a series or film with a sheet. */
withDefaults(
    defineProps<{
        hasSheet?: boolean
        /** The sheet is a film's, not a series'. */
        movie?: boolean
        live?: boolean
        commercial?: boolean
    }>(),
    { hasSheet: false, movie: false, live: false, commercial: false },
)

const { t } = useI18n()
</script>

<template>
    <span
        v-if="commercial"
        class="text-[11px] text-after-clip border border-after-clip/50 rounded-md px-1.5 leading-4"
        :title="t('storage.commercial.badgeHint')"
    >
        {{ t('storage.commercial.badge') }}
    </span>
    <span
        v-else-if="live"
        class="text-[11px] text-live border border-live/50 rounded-md px-1.5 leading-4"
        :title="t('storage.live.badgeHint')"
    >
        {{ t('storage.live.badge') }}
    </span>
    <span
        v-else-if="hasSheet"
        class="text-[11px] text-accent-text border border-accent/40 rounded-md px-1.5 leading-4"
        :title="t('storage.hasSheet')"
    >
        {{ movie ? t('storage.movieBadge') : t('storage.seriesBadge') }}
    </span>
</template>
