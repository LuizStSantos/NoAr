-- Graphics at a set time: logo and L-shape changes (as a quick scene does)
-- applied when the event starts and undone when it ends, once or on chosen
-- weekdays. They change the picture on air, never the programme.
CREATE TABLE graphics_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    channel_id INTEGER NOT NULL REFERENCES channels (id) ON DELETE CASCADE,
    title TEXT NOT NULL DEFAULT '',
    -- JSON: what to do to the logo and the L-shape (see GraphicsScene).
    actions TEXT NOT NULL DEFAULT '{}',
    -- First (or only) day, YYYY-MM-DD, and times in the channel time zone.
    start_date TEXT NOT NULL,
    start_time TEXT NOT NULL,
    -- When to undo it; none leaves it on until changed. An end before the
    -- start means the next day.
    end_time TEXT,
    -- Weekday bits, Sunday = 1 ... Saturday = 64; 0 happens once.
    repeat_days INTEGER NOT NULL DEFAULT 0 CHECK (repeat_days BETWEEN 0 AND 127),
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX graphics_events_channel ON graphics_events (channel_id);
