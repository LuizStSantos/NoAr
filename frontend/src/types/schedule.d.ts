export {}

declare global {
    type ScheduleKind = 'media' | 'live'
    type EntryMode = 'after_clip' | 'interrupt'
    type AfterMode = 'continue' | 'clock'
    type RunStatus = 'running' | 'done' | 'skipped' | 'failed'

    interface ScheduleEvent {
        id: number
        channel_id: number
        kind: ScheduleKind
        title: string
        /** Storage paths: the clips, or the one `.live` file. */
        sources: string[]
        /** Total clip length in seconds; 0 for lives. */
        duration: number
        start_date: string
        start_time: string
        end_time: string | null
        /** Weekday bits: Sunday 1 ... Saturday 64; 0 = once. */
        repeat_days: number
        entry_mode: EntryMode
        after_mode: AfterMode
        show_in_guide: boolean
        retry_seconds: number
        enabled: boolean
    }

    type ScheduleEventInput = Omit<ScheduleEvent, 'id' | 'channel_id' | 'duration'>

    interface ScheduleOccurrence {
        event: ScheduleEvent
        /** Local `YYYY-MM-DDTHH:MM:SS`. */
        start: string
        end: string
        status: RunStatus | null
    }

    interface InterruptedClip {
        source: string
        title: string | null
        position: number
    }

    interface ActiveScheduleEvent {
        event_id: number
        title: string
        kind: ScheduleKind
        entry_mode: EntryMode
        after_mode: AfterMode
        planned_seconds: number
        elapsed_seconds: number
        item: string | null
        interrupted: InterruptedClip | null
    }

    interface ScheduleStatus {
        active: ActiveScheduleEvent | null
        queued: ScheduleOccurrence[]
        next: ScheduleOccurrence[]
    }

    interface LiveSource {
        path: string
        name: string
        url: string
        protocol: string
        title: string | null
    }

    interface LiveGuide {
        title: string
        plot: string
        genre: string
        rating: string
    }

    interface SignalTest {
        ok: boolean
        error: string | null
        width: number | null
        height: number | null
        fps: number | null
        video_codec: string | null
        audio_codec: string | null
        sample_rate: number | null
    }

    interface ProgramItem {
        source: string
        start: string
        title: string | null
        in: number
        out: number
        duration: number
        ad: boolean
    }

    /** A guide entry of the day, as the timeline shows the programme. */
    interface GuideBlock {
        start: string
        stop: string
        title: string
        sub_title: string | null
        scheduled: boolean
    }

    interface ScheduleSourceItem {
        path: string
        name: string
        duration: number
    }

    /** What the "schedule material" dialog opens with. */
    interface MediaDraft {
        event?: ScheduleEvent
        sources: ScheduleSourceItem[]
        date: string
        time: string
    }

    /** What the live dialog opens with. */
    interface LiveDraft {
        event?: ScheduleEvent
        /** An existing live; a new one is created when missing. */
        live?: LiveSource
        date: string
        start: string
        end: string
    }

    /** What is dragged onto the timeline. */
    type ScheduleDragItem =
        | { type: 'media'; path: string; name: string; duration: number }
        | { type: 'live'; path: string; name: string; protocol: string }
        | { type: 'event'; id: number }
}
