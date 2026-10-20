-- What actually went on air, one row per item, for the report. A row opens
-- when the item starts and closes when it ends; times are Unix milliseconds.
CREATE TABLE as_run (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    channel_id INTEGER NOT NULL REFERENCES channels (id) ON DELETE CASCADE,
    started_at INTEGER NOT NULL,
    -- Empty while the item is on air.
    ended_at INTEGER,
    -- Storage path of a clip or a live folder's `.live` file, a listener name
    -- for ingest, or empty for generated black.
    source TEXT NOT NULL DEFAULT '',
    title TEXT,
    kind TEXT NOT NULL CHECK (kind IN ('program', 'commercial', 'live', 'filler', 'ingest')),
    origin TEXT NOT NULL CHECK (origin IN ('playlist', 'folder', 'schedule', 'ingest')),
    -- The scheduled event the item belongs to. No foreign key: the row stays
    -- when the event is deleted.
    event_id INTEGER,
    event_title TEXT,
    -- When the scheduled event was due, Unix milliseconds.
    scheduled_at INTEGER,
    seek REAL NOT NULL DEFAULT 0,
    planned_seconds REAL NOT NULL DEFAULT 0,
    -- What a filler aired in place of: a missing clip, or a live without signal.
    replaces TEXT,
    status TEXT NOT NULL DEFAULT 'on_air' CHECK (status IN ('on_air', 'completed', 'cut', 'failed', 'stopped')),
    -- Why it ended early: `event`, `ingest`, `skipped`, `signal_lost` or
    -- `error`, with `detail` naming the event or listener, or the error.
    reason TEXT,
    detail TEXT
);

CREATE INDEX as_run_channel_start ON as_run (channel_id, started_at);
