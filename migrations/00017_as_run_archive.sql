-- The as-run of deleted channels: the broadcast record outlives the channel.
-- No foreign key, and the channel's name is kept, because a new channel can
-- later get the same id.
CREATE TABLE as_run_archive (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    channel_id INTEGER NOT NULL,
    channel_name TEXT NOT NULL,
    -- When the channel was deleted, Unix milliseconds.
    deleted_at INTEGER NOT NULL,
    started_at INTEGER NOT NULL,
    ended_at INTEGER,
    source TEXT NOT NULL DEFAULT '',
    title TEXT,
    kind TEXT NOT NULL,
    origin TEXT NOT NULL,
    event_id INTEGER,
    event_title TEXT,
    scheduled_at INTEGER,
    seek REAL NOT NULL DEFAULT 0,
    planned_seconds REAL NOT NULL DEFAULT 0,
    replaces TEXT,
    status TEXT NOT NULL,
    reason TEXT,
    detail TEXT
);

CREATE INDEX as_run_archive_channel_start ON as_run_archive (channel_id, started_at);
