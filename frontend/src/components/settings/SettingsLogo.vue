<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import LogoRemote from '@/components/settings/LogoRemote.vue'
import SaveBar from '@/components/settings/SaveBar.vue'
import SectionCard from '@/components/settings/SectionCard.vue'
import ToggleRow from '@/components/settings/ToggleRow.vue'
import UnitInput from '@/components/settings/UnitInput.vue'
import StoragePicker from '@/components/schedule/StoragePicker.vue'
import GenericModal from '@/components/utils/GenericModal.vue'
import { createFilePreviewUrl } from '@/composables/fileAccess'
import { usePlayoutPage } from '@/composables/settingsPage'

const { t } = useI18n()
const { ready, changes, alsoSaves, saveLabel, save, discard, settings, configStore } = usePlayoutPage('logo')

type Column = 'left' | 'center' | 'right'
type Row = 'top' | 'middle' | 'bottom'

const COLUMNS: Column[] = ['left', 'center', 'right']
const ROWS: Row[] = ['top', 'middle', 'bottom']
const SIZES = ['', '6%:-1', '8%:-1', '10%:-1', '12%:-1', '15%:-1', '20%:-1']

const picking = ref<'logo' | 'guide' | null>(null)
const showAdvanced = ref(false)
const logoUrl = ref('')
const logoFailed = ref(false)

const processing = computed(() => configStore.playout.processing)
const output = computed(() => configStore.playout.output)

interface Placement {
    column: Column
    row: Row
    margin: number
}

/** Read a corner or center placement back from the position expression. */
function parsePosition(position: string): Placement | null {
    const [x = '', y = ''] = position.replace(/\s+/g, '').split(':')
    const axis = (value: string, big: string, small: string): { side: number; margin: number } | null => {
        if (/^\d+$/.test(value)) {
            return { side: 0, margin: Number(value) }
        }

        if (value === `${big}/2-${small}/2`) {
            return { side: 1, margin: -1 }
        }

        const end = new RegExp(`^${big}-${small}-(\\d+)$`).exec(value)

        return end ? { side: 2, margin: Number(end[1]) } : null
    }
    const horizontal = axis(x, 'W', 'w')
    const vertical = axis(y, 'H', 'h')

    if (!horizontal || !vertical) {
        return null
    }

    const margins = [horizontal.margin, vertical.margin].filter((value) => value >= 0)

    return {
        column: COLUMNS[horizontal.side] as Column,
        row: ROWS[vertical.side] as Row,
        margin: margins[0] ?? 12,
    }
}

function writePosition(placement: Placement) {
    const m = Math.max(0, Math.round(placement.margin))
    const x = placement.column === 'left' ? `${m}` : placement.column === 'center' ? 'W/2-w/2' : `W-w-${m}`
    const y = placement.row === 'top' ? `${m}` : placement.row === 'middle' ? 'H/2-h/2' : `H-h-${m}`

    processing.value.logo_position = `${x}:${y}`
}

const placement = computed(() => parsePosition(processing.value.logo_position))

const margin = computed({
    get: () => placement.value?.margin ?? 12,
    set: (value: number) => {
        if (placement.value) {
            writePosition({ ...placement.value, margin: Number(value) || 0 })
        }
    },
})

function place(column: Column, row: Row) {
    writePosition({ column, row, margin: placement.value?.margin ?? 12 })
}

const opacity = computed({
    get: () => Math.round((processing.value.logo_opacity ?? 1) * 100),
    set: (value: number) => (processing.value.logo_opacity = Math.round(value) / 100),
})

const sizeChoice = computed({
    get: () => (SIZES.includes(processing.value.logo_scale.trim()) ? processing.value.logo_scale.trim() : 'custom'),
    set: (value: string) => {
        if (value !== 'custom') {
            processing.value.logo_scale = value
        } else {
            showAdvanced.value = true
        }
    },
})

/** The output picture, which the preview frame takes the shape of. */
const frameSize = computed(() => ({
    width: Math.max(1, output.value?.width || 1920),
    height: Math.max(1, output.value?.height || 1080),
}))

/** Action and title safe areas (SMPTE ST 2046-1), as a share of the picture. */
const safeAreas = [
    { key: 'action', share: 0.93, color: 'text-[#e3b341]' },
    { key: 'title', share: 0.9, color: 'text-[#6cb6ff]' },
]

const GUIDES_KEY = 'noar.logoGuides'
const showGuides = ref(readGuides())

function readGuides(): boolean {
    try {
        return localStorage.getItem(GUIDES_KEY) !== 'off'
    } catch {
        return true
    }
}

watch(showGuides, (on) => {
    try {
        localStorage.setItem(GUIDES_KEY, on ? 'on' : 'off')
    } catch {
        // Storage can be unavailable; the choice then lasts for the page.
    }
})

/** Where the logo sits in the preview frame, in percent. */
const previewStyle = computed(() => {
    const { width, height } = frameSize.value
    const spot = placement.value ?? { column: 'right', row: 'top', margin: 12 }
    const size = /^(\d+(?:\.\d+)?)%/.exec(processing.value.logo_scale.trim())
    const style: Record<string, string> = {
        width: `${size ? Number(size[1]) : 12}%`,
        opacity: String(processing.value.logo_opacity ?? 1),
    }
    const mx = `${(spot.margin / width) * 100}%`
    const my = `${(spot.margin / height) * 100}%`

    if (spot.column === 'left') style.left = mx
    if (spot.column === 'right') style.right = mx
    if (spot.column === 'center') {
        style.left = '50%'
        style.transform = 'translateX(-50%)'
    }

    if (spot.row === 'top') style.top = my
    if (spot.row === 'bottom') style.bottom = my
    if (spot.row === 'middle') {
        style.top = '50%'
        style.transform = `${style.transform ?? ''} translateY(-50%)`.trim()
    }

    return style
})

/** A URL that shows the storage image at `path`, or '' when there is none. */
async function storageImageUrl(path: string | undefined): Promise<string> {
    const storage = configStore.channels[configStore.i]?.storage ?? ''
    const logo = path?.trim()

    if (!logo || !storage) {
        return ''
    }

    const absolute = logo.startsWith('/') ? logo : `${storage.replace(/\/+$/, '')}/${logo}`
    const parent = storage.slice(0, storage.lastIndexOf('/'))

    if (!absolute.startsWith(storage)) {
        throw new Error('outside the storage')
    }

    return createFilePreviewUrl(configStore.channels[configStore.i]?.id, absolute.slice(parent.length))
}

async function loadLogo() {
    logoFailed.value = false
    logoUrl.value = ''

    try {
        logoUrl.value = await storageImageUrl(processing.value?.logo)
    } catch {
        logoFailed.value = true
    }
}

watch(() => processing.value?.logo, loadLogo, { immediate: true })

const guideLogoUrl = ref('')
const guideLogoFailed = ref(false)

async function loadGuideLogo() {
    guideLogoFailed.value = false
    guideLogoUrl.value = ''

    try {
        guideLogoUrl.value = await storageImageUrl(processing.value?.guide_logo || processing.value?.logo)
    } catch {
        guideLogoFailed.value = true
    }
}

watch(() => [processing.value?.guide_logo, processing.value?.logo], loadGuideLogo, { immediate: true })

function pick(file: { path: string }) {
    if (picking.value === 'guide') {
        processing.value.guide_logo = file.path
    } else {
        processing.value.logo = file.path
    }

    picking.value = null
}
</script>
<template>
    <template v-if="ready">
        <div>
            <h1 class="m-0 text-xl font-semibold">{{ t('settings.sections.logo') }}</h1>
            <p class="mt-1 mb-0 text-muted">{{ t('settings.logo.intro') }}</p>
        </div>

        <SectionCard :title="t('settings.logo.logo')">
            <template #aside>
                <input
                    v-model="processing.add_logo"
                    type="checkbox"
                    role="switch"
                    class="toggle toggle-accent"
                    :aria-label="t('settings.logo.show')"
                />
            </template>
            <div class="flex flex-wrap gap-6" :class="processing.add_logo ? '' : 'opacity-60'">
                <div class="w-full max-w-[480px] flex-none self-start flex flex-col gap-2">
                    <div
                        class="relative w-full rounded-xl overflow-hidden bg-[#0b0c0d] border border-line"
                        :style="{ aspectRatio: `${frameSize.width} / ${frameSize.height}` }"
                    >
                        <span class="absolute inset-0 flex items-center justify-center text-[#6e6e72] text-xs">{{
                            t('settings.logo.frame')
                        }}</span>
                        <svg
                            v-if="showGuides"
                            class="absolute inset-0 w-full h-full pointer-events-none"
                            :viewBox="`0 0 ${frameSize.width} ${frameSize.height}`"
                            preserveAspectRatio="none"
                            aria-hidden="true"
                            fill="none"
                            stroke-width="1"
                        >
                            <g
                                class="text-[#8a8a90]"
                                stroke="currentColor"
                                stroke-opacity="0.35"
                                stroke-dasharray="4 4"
                            >
                                <line
                                    v-for="x in [1, 2]"
                                    :key="`v${x}`"
                                    :x1="(frameSize.width * x) / 3"
                                    :x2="(frameSize.width * x) / 3"
                                    y1="0"
                                    :y2="frameSize.height"
                                    vector-effect="non-scaling-stroke"
                                />
                                <line
                                    v-for="y in [1, 2]"
                                    :key="`h${y}`"
                                    x1="0"
                                    :x2="frameSize.width"
                                    :y1="(frameSize.height * y) / 3"
                                    :y2="(frameSize.height * y) / 3"
                                    vector-effect="non-scaling-stroke"
                                />
                            </g>
                            <g class="text-[#8a8a90]" stroke="currentColor" stroke-opacity="0.6">
                                <line
                                    :x1="frameSize.width / 2"
                                    :x2="frameSize.width / 2"
                                    :y1="frameSize.height / 2 - frameSize.height * 0.04"
                                    :y2="frameSize.height / 2 + frameSize.height * 0.04"
                                    vector-effect="non-scaling-stroke"
                                />
                                <line
                                    :x1="frameSize.width / 2 - frameSize.height * 0.04"
                                    :x2="frameSize.width / 2 + frameSize.height * 0.04"
                                    :y1="frameSize.height / 2"
                                    :y2="frameSize.height / 2"
                                    vector-effect="non-scaling-stroke"
                                />
                            </g>
                            <rect
                                v-for="area in safeAreas"
                                :key="area.key"
                                :class="area.color"
                                stroke="currentColor"
                                stroke-opacity="0.7"
                                :x="(frameSize.width * (1 - area.share)) / 2"
                                :y="(frameSize.height * (1 - area.share)) / 2"
                                :width="frameSize.width * area.share"
                                :height="frameSize.height * area.share"
                                vector-effect="non-scaling-stroke"
                            />
                            <rect
                                v-if="placement && margin > 0"
                                class="text-accent"
                                stroke="currentColor"
                                stroke-opacity="0.8"
                                stroke-dasharray="2 3"
                                :x="margin"
                                :y="margin"
                                :width="Math.max(0, frameSize.width - margin * 2)"
                                :height="Math.max(0, frameSize.height - margin * 2)"
                                vector-effect="non-scaling-stroke"
                            />
                        </svg>
                        <img
                            v-if="logoUrl && !logoFailed"
                            :src="logoUrl"
                            alt=""
                            class="absolute h-auto"
                            :style="previewStyle"
                            @error="logoFailed = true"
                        />
                        <span
                            v-else
                            class="absolute aspect-[2/1] border border-dashed border-accent text-accent-text text-[10px] flex items-center justify-center rounded"
                            :style="previewStyle"
                        >
                            logo
                        </span>
                        <span class="absolute right-2 bottom-1.5 font-mono text-[10px] text-[#6e6e72]">
                            {{ frameSize.width }}×{{ frameSize.height }}
                        </span>
                    </div>
                    <label class="flex items-center gap-2 text-[13px] text-muted cursor-pointer">
                        <input v-model="showGuides" type="checkbox" class="checkbox checkbox-xs" />
                        {{ t('settings.logo.guides') }}
                    </label>
                    <ul
                        v-if="showGuides"
                        class="m-0 p-0 list-none flex flex-wrap gap-x-3.5 gap-y-1 text-[11px] text-muted"
                    >
                        <li class="flex items-center gap-1.5">
                            <span class="w-3 border-t border-dashed border-[#8a8a90]" />{{
                                t('settings.logo.guideThirds')
                            }}
                        </li>
                        <li
                            v-for="area in safeAreas"
                            :key="area.key"
                            class="flex items-center gap-1.5"
                            :class="area.color"
                        >
                            <span class="w-3 border-t border-current" /><span class="text-muted">{{
                                t(`settings.logo.guide.${area.key}`)
                            }}</span>
                        </li>
                        <li v-if="placement && margin > 0" class="flex items-center gap-1.5 text-accent">
                            <span class="w-3 border-t border-dotted border-current" /><span class="text-muted">{{
                                t('settings.logo.guideMargin', { n: margin })
                            }}</span>
                        </li>
                    </ul>
                </div>
                <div class="flex-[1_1_300px] flex flex-col gap-4 min-w-0">
                    <label class="field-label">
                        {{ t('settings.logo.file') }}
                        <span class="flex gap-2">
                            <input
                                v-model.trim="processing.logo"
                                class="field-input font-mono text-[13px]"
                                spellcheck="false"
                            />
                            <button class="btn-quiet flex-none" @click="picking = 'logo'">
                                {{ t('settings.choose') }}
                            </button>
                        </span>
                    </label>
                    <div class="flex flex-wrap gap-5">
                        <div class="field-label">
                            {{ t('settings.logo.position') }}
                            <div
                                role="radiogroup"
                                :aria-label="t('settings.logo.position')"
                                class="grid grid-cols-3 gap-1.5 w-[132px]"
                            >
                                <template v-for="row in ROWS" :key="row">
                                    <button
                                        v-for="column in COLUMNS"
                                        :key="`${row}-${column}`"
                                        role="radio"
                                        :aria-checked="placement?.row === row && placement?.column === column"
                                        :aria-label="t(`settings.logo.place.${row}-${column}`)"
                                        class="h-[38px] rounded-md border cursor-pointer"
                                        :class="
                                            placement?.row === row && placement?.column === column
                                                ? 'bg-accent border-accent'
                                                : 'bg-field border-line-strong hover:border-subtle'
                                        "
                                        @click="place(column, row)"
                                    />
                                </template>
                            </div>
                            <span v-if="!placement" class="field-hint">{{ t('settings.logo.customPosition') }}</span>
                        </div>
                        <div class="flex flex-col gap-3.5 flex-[1_1_160px]">
                            <label class="field-label">
                                {{ t('settings.logo.margin') }}
                                <UnitInput
                                    v-model.number="margin"
                                    unit="px"
                                    type="number"
                                    min="0"
                                    :disabled="!placement"
                                />
                            </label>
                            <label class="field-label">
                                {{ t('settings.logo.size') }}
                                <select v-model="sizeChoice" class="field-input">
                                    <option value="">{{ t('settings.logo.original') }}</option>
                                    <option v-for="size in SIZES.slice(1)" :key="size" :value="size">
                                        {{ t('settings.logo.sizePercent', { n: size.split('%')[0] }) }}
                                    </option>
                                    <option value="custom">{{ t('settings.output.customValue') }}</option>
                                </select>
                            </label>
                        </div>
                    </div>
                    <label class="field-label">
                        {{ t('settings.logo.opacity', { n: opacity }) }}
                        <input
                            v-model.number="opacity"
                            type="range"
                            min="0"
                            max="100"
                            class="range range-xs range-accent max-w-[360px]"
                        />
                    </label>
                    <button
                        class="flex items-center gap-2 bg-transparent border-0 p-0 min-h-8 cursor-pointer text-muted text-left"
                        :aria-expanded="showAdvanced"
                        @click="showAdvanced = !showAdvanced"
                    >
                        <i
                            class="bi text-xs"
                            :class="showAdvanced ? 'bi-chevron-down' : 'bi-chevron-right'"
                            aria-hidden="true"
                        />
                        {{ t('settings.logo.advanced') }}
                    </button>
                    <div v-if="showAdvanced" class="grid grid-cols-1 sm:grid-cols-2 gap-3.5">
                        <label class="field-label">
                            {{ t('settings.logo.positionExpr') }}
                            <input
                                v-model.trim="processing.logo_position"
                                class="field-input font-mono text-[13px]"
                                placeholder="W-w-12:12"
                                spellcheck="false"
                            />
                            <span class="field-hint">{{ t('settings.logo.positionExprHint') }}</span>
                        </label>
                        <label class="field-label">
                            {{ t('settings.logo.scaleExpr') }}
                            <input
                                v-model.trim="processing.logo_scale"
                                class="field-input font-mono text-[13px]"
                                placeholder="12%:-1"
                                spellcheck="false"
                            />
                            <span class="field-hint">{{ t('settings.logo.scaleExprHint') }}</span>
                        </label>
                    </div>
                </div>
            </div>
        </SectionCard>

        <SectionCard :title="t('settings.logo.guideLogo')" :description="t('settings.logo.guideLogoHint')">
            <div class="flex flex-wrap items-center gap-4">
                <div
                    class="w-24 h-24 flex-none rounded-xl border border-line bg-[#0b0c0d] flex items-center justify-center overflow-hidden p-2"
                >
                    <img
                        v-if="guideLogoUrl && !guideLogoFailed"
                        :src="guideLogoUrl"
                        alt=""
                        class="max-w-full max-h-full object-contain"
                        @error="guideLogoFailed = true"
                    />
                    <i v-else class="bi bi-image text-subtle text-xl" aria-hidden="true" />
                </div>
                <label class="field-label flex-[1_1_320px] min-w-0">
                    {{ t('settings.logo.file') }}
                    <span class="flex gap-2">
                        <input
                            v-model.trim="processing.guide_logo"
                            class="field-input font-mono text-[13px]"
                            :placeholder="processing.logo || ''"
                            spellcheck="false"
                        />
                        <button class="btn-quiet flex-none" @click="picking = 'guide'">
                            {{ t('settings.choose') }}
                        </button>
                    </span>
                    <span class="field-hint">{{
                        processing.guide_logo ? t('settings.logo.guideLogoOwn') : t('settings.logo.guideLogoSame')
                    }}</span>
                </label>
            </div>
        </SectionCard>

        <LogoRemote />

        <SectionCard :title="t('settings.logo.subtitles')" :description="t('settings.logo.subtitlesHint')">
            <template #aside>
                <input
                    v-model="processing.vtt_enable"
                    type="checkbox"
                    role="switch"
                    class="toggle toggle-accent"
                    :aria-label="t('settings.logo.useSubtitles')"
                />
            </template>
            <div class="grid grid-cols-1 md:grid-cols-3 gap-3.5" :class="processing.vtt_enable ? '' : 'opacity-60'">
                <label class="field-label">
                    {{ t('settings.logo.trackName') }}
                    <input v-model.trim="processing.vtt_name" class="field-input" />
                </label>
                <label class="field-label">
                    {{ t('settings.logo.language') }}
                    <input v-model.trim="processing.vtt_language" class="field-input font-mono" placeholder="pt-BR" />
                </label>
                <label class="field-label">
                    {{ t('settings.logo.emptyFile') }}
                    <input
                        v-model.trim="processing.vtt_dummy"
                        class="field-input font-mono text-[13px]"
                        spellcheck="false"
                    />
                    <span class="field-hint">{{ t('settings.logo.emptyFileHint') }}</span>
                </label>
            </div>
            <ToggleRow v-model="processing.vtt_default" :label="t('settings.logo.subtitleDefault')" />
        </SectionCard>

        <SaveBar
            :changes="changes"
            :also-saves="alsoSaves"
            :saving="settings.saving"
            :save-label="saveLabel"
            @save="save"
            @discard="discard"
        />

        <GenericModal
            :show="picking !== null"
            :title="t('settings.choose')"
            hide-buttons
            :modal-action="() => (picking = null)"
        >
            <div class="w-[min(520px,90vw)] max-h-[60vh] flex flex-col">
                <StoragePicker @pick="pick" />
            </div>
        </GenericModal>
    </template>
</template>
