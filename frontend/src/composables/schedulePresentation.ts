import { useI18n } from 'vue-i18n'

export const SCHEDULE_DAY_SECONDS = 86400
export const SCHEDULE_DRAG_TYPE = 'application/x-noar-schedule'
export const SCHEDULE_GRAPHICS_COLOR = '#8f7ae6'
export const SCHEDULE_ZOOM_OPTIONS = [
    { minutes: 60, label: '1 h', tick: 600 },
    { minutes: 360, label: '6 h', tick: 3600 },
    { minutes: 1440, label: '24 h', tick: 7200 },
]

export interface ScheduleGradeBlock {
    start: number
    end: number
    label: string
    detail: string | null
    scheduled: boolean
}

export interface ScheduleEventBlock {
    occurrence: ScheduleOccurrence
    nominal: number
    start: number
    end: number
    color: string
}

export interface ScheduleGraphicsBlock {
    event: GraphicsEvent
    start: number
    end: number
    open: boolean
}

/** Shared text for graphics timeline/list views, including frozen legacy actions. */
export function useScheduledGraphicsText() {
    const { t } = useI18n()

    /** What a graphics event does, in a few words. */
    function graphicsSummary(event: GraphicsEvent): string {
        if (event.template) {
            return event.template.name || t('schedule.graphics.missingTemplate', { id: event.template.template_id })
        }

        const actions = event.actions
        const parts: string[] = []

        if (actions.logo !== 'keep') {
            parts.push(t(`graphics.scenes.summary.logo.${actions.logo}`))
        }

        if (actions.logo_position) {
            parts.push(t('graphics.scenes.summary.logoMoves'))
        }

        if (actions.lshape !== 'keep') {
            parts.push(t(`graphics.scenes.summary.lshape.${actions.lshape}`))
        }

        if (actions.lshape_corner && actions.lshape_corner !== 'default') {
            parts.push(t(`lshape.corners.${actions.lshape_corner}`).toLowerCase())
        }

        if (actions.lshape_scale) {
            parts.push(`${actions.lshape_scale}%`)
        }

        return parts.join(' · ') || t('graphics.scenes.summary.nothing')
    }

    return { graphicsSummary }
}
