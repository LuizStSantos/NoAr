-- Scheduled events that play over the running programme: breaks, clips and
-- live sources at a set time, once or on chosen weekdays.
CREATE TABLE schedule_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    channel_id INTEGER NOT NULL REFERENCES channels (id) ON DELETE CASCADE,
    kind TEXT NOT NULL DEFAULT 'media' CHECK (kind IN ('media', 'live')),
    title TEXT NOT NULL DEFAULT '',
    -- JSON list of storage paths: the clips to play, or the one `.live` file.
    sources TEXT NOT NULL DEFAULT '[]',
    -- Total length of the clips in seconds; unused for live events.
    duration REAL NOT NULL DEFAULT 0,
    -- First (or only) day, YYYY-MM-DD, and times in the channel time zone.
    start_date TEXT NOT NULL,
    start_time TEXT NOT NULL,
    -- Live events only. An end before the start means the next day.
    end_time TEXT,
    -- Weekday bits, Sunday = 1 ... Saturday = 64; 0 plays once.
    repeat_days INTEGER NOT NULL DEFAULT 0 CHECK (repeat_days BETWEEN 0 AND 127),
    entry_mode TEXT NOT NULL DEFAULT 'after_clip' CHECK (entry_mode IN ('after_clip', 'interrupt')),
    after_mode TEXT NOT NULL DEFAULT 'continue' CHECK (after_mode IN ('continue', 'clock')),
    show_in_guide INTEGER NOT NULL DEFAULT 0,
    -- Live events: how long to wait for a lost signal before going back.
    retry_seconds INTEGER NOT NULL DEFAULT 120 CHECK (retry_seconds >= 0),
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX schedule_events_channel ON schedule_events (channel_id);

-- One row per occurrence that went on air (or was skipped), so an occurrence
-- never plays twice, also across restarts.
CREATE TABLE schedule_runs (
    event_id INTEGER NOT NULL REFERENCES schedule_events (id) ON DELETE CASCADE,
    occurrence TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('running', 'done', 'skipped', 'failed')),
    started_at TEXT,
    finished_at TEXT,
    PRIMARY KEY (event_id, occurrence)
);
