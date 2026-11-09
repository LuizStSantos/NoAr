import { useI18n } from 'vue-i18n'

import type { GraphicsTemplate, TemplateLayer } from '@/composables/graphicsTemplates'

/** Localize conversion defaults for display without changing saved names or identifiers. */
export function useGraphicsDisplayNames() {
    const { t } = useI18n()

    function templateName(template: Pick<GraphicsTemplate, 'name' | 'group'>): string {
        const match = /^(logo|ticker|lshape)( \((?:[2-9]|[1-9]\d+)\))?$/.exec(template.name)

        if (template.group !== 'Channel graphics' || !match) {
            return template.name
        }

        return t(`graphics.assigned.${match[1]}`) + (match[2] ?? '')
    }

    function layerName(layer: Pick<TemplateLayer, 'id' | 'name' | 'kind'>): string {
        if (layer.id === 'image' && layer.name === 'Image' && layer.kind === 'image') {
            return t('graphics.composer.kinds.image')
        }

        if (layer.id === 'picture' && layer.name === 'Picture' && layer.kind === 'video') {
            return t('graphics.composer.kinds.video')
        }

        return layer.name
    }

    return { templateName, layerName }
}
