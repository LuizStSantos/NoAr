import { computed, ref, type Ref } from 'vue'

import { type TemplateLayer, fitLayer } from '@/composables/graphicsTemplates'

type OutputFrame = { width: number; height: number }

/** Geometry controls emit document patches; scale presets never compound. */
export function useComposerGeometry(
    selected: Ref<TemplateLayer | undefined>,
    frame: Ref<OutputFrame>,
    updateSelectedLayer: (patch: Partial<TemplateLayer>) => void,
) {
    const proportional = ref(false)
    const unit = ref('%')
    const scaleBases = new Map<string, { width: number; height: number }>()

    function rememberScaleBase(layer: TemplateLayer) {
        scaleBases.set(layer.id, { width: layer.width, height: layer.height })
    }

    function resetScaleBases(layers: TemplateLayer[]) {
        scaleBases.clear()

        for (const layer of layers) {
            rememberScaleBase(layer)
        }
    }

    const videoCorner = computed(() => {
        const layer = selected.value

        if (!layer || layer.kind !== 'video') {
            return 'custom'
        }

        const horizontal =
            Math.abs(layer.x) < 0.01 ? 'left' : Math.abs(layer.x + layer.width - 100) < 0.01 ? 'right' : ''
        const vertical =
            Math.abs(layer.y) < 0.01 ? 'top' : Math.abs(layer.y + layer.height - 100) < 0.01 ? 'bottom' : ''

        return horizontal && vertical ? `${vertical}-${horizontal}` : 'custom'
    })

    function setVideoWindow(corner: string, size: number) {
        if (!selected.value || corner === 'custom') {
            return
        }

        const percent = Math.max(30, Math.min(95, size))
        const width = Math.max(2, Math.round((frame.value.width * percent) / 100) & ~1)
        const height = Math.max(2, Math.round((frame.value.height * percent) / 100) & ~1)
        const x = corner.endsWith('right') ? (frame.value.width - width) & ~1 : 0
        const y = corner.startsWith('bottom') ? (frame.value.height - height) & ~1 : 0
        updateSelectedLayer({
            x: (x * 100) / frame.value.width,
            y: (y * 100) / frame.value.height,
            width: (width * 100) / frame.value.width,
            height: (height * 100) / frame.value.height,
        })
    }

    function setDimension(key: 'x' | 'y' | 'width' | 'height', value: string) {
        if (!selected.value) {
            return
        }

        const number = Number(value.replace(',', '.'))

        if (!Number.isFinite(number)) {
            return
        }

        const axis = key === 'x' || key === 'width' ? frame.value.width : frame.value.height
        updateSelectedLayer(
            fitLayer(
                selected.value,
                { [key]: unit.value === 'px' ? (number / axis) * 100 : number },
                proportional.value && (key === 'width' || key === 'height'),
            ),
        )
    }

    function formatDimension(key: 'x' | 'y' | 'width' | 'height') {
        const value = selected.value?.[key] ?? 0
        const axis = key === 'x' || key === 'width' ? frame.value.width : frame.value.height
        return Math.round(value * (unit.value === 'px' ? axis / 100 : 1) * 100) / 100
    }

    function setScale(value: number) {
        const layer = selected.value

        if (!layer || !Number.isFinite(value) || value <= 0) {
            return
        }

        const base = scaleBases.get(layer.id) ?? layer
        updateSelectedLayer(
            fitLayer(layer, { width: (base.width * value) / 100, height: (base.height * value) / 100 }, true),
        )
    }

    const scaleValue = computed(() =>
        selected.value
            ? Math.round(
                  (selected.value.width / (scaleBases.get(selected.value.id)?.width ?? selected.value.width)) * 100,
              )
            : 100,
    )

    return {
        proportional,
        unit,
        videoCorner,
        scaleValue,
        setVideoWindow,
        setDimension,
        formatDimension,
        setScale,
        rememberScaleBase,
        resetScaleBases,
    }
}
