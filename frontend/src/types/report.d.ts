export {}

declare global {
    /** Kind of an item in the report; `gap` is a stretch without any record. */
    type ReportKind = 'program' | 'commercial' | 'live' | 'filler' | 'ingest' | 'gap'

    /** `unrecorded` is the plan of a time before NoAr recorded what aired. */
    type ReportState = 'aired' | 'on_air' | 'planned' | 'unrecorded'

    interface ReportDay {
        /** `YYYY-MM-DD` */
        date: string
        /** Unix milliseconds. */
        start: number
        end: number
        /** The day has a playlist; without one its plan only has scheduled events. */
        playlist: boolean
    }

    interface ReportItem {
        /** Playlist day, `YYYY-MM-DD`. */
        day: string
        /** Unix milliseconds. */
        start: number
        /** Unix milliseconds; for the item on air, when it is due to end. */
        end: number
        title: string
        sub_title: string | null
        /** Storage path, a listener name for ingest, or empty. */
        source: string
        /** Storage folder of the program the clip belongs to. */
        program: string | null
        kind: ReportKind
        /** `playlist`, `folder`, `schedule` or `ingest`. */
        origin: string
        event_id: number | null
        event_title: string | null
        /** When the scheduled event was due, Unix milliseconds. */
        scheduled_at: number | null
        state: ReportState
        /** `completed`, `cut`, `failed` or `stopped`, once aired. */
        status: string | null
        /** `event`, `ingest`, `skipped`, `signal_lost`, `error` or `interrupted`. */
        reason: string | null
        detail: string | null
        planned_seconds: number
        /** What a filler stood in for. */
        replaces: string | null
    }

    interface Report {
        timezone: string
        /** `HH:MM:SS` */
        day_start: string
        /** Unix milliseconds: items before it aired. */
        now: number
        /** First record of the channel; before it only the plan is known. */
        recorded_since: number | null
        days: ReportDay[]
        items: ReportItem[]
    }

    /** An item the report can be narrowed to. */
    interface ReportPick {
        type: 'file' | 'folder' | 'live' | 'event'
        /** Storage path, or the event id. */
        value: string
        label: string
        /** What it is, for its chip: `program`, `commercial`, `live`, `event`, `folder`. */
        group: 'program' | 'commercial' | 'folder' | 'live' | 'event'
    }
}
