-- Last successful records survive provider outages and backend restarts.
CREATE TABLE graphics_data_sources (
 id INTEGER PRIMARY KEY AUTOINCREMENT, channel_id INTEGER NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
 name TEXT NOT NULL, kind TEXT NOT NULL CHECK(kind IN ('rss','csv','json')),
 url TEXT NOT NULL, interval INTEGER NOT NULL CHECK(interval BETWEEN 5 AND 3600),
 records_path TEXT NOT NULL DEFAULT '', document TEXT NOT NULL DEFAULT '',
 data TEXT NOT NULL DEFAULT '[]', last_success INTEGER, last_attempt INTEGER, error TEXT,
 revision INTEGER NOT NULL DEFAULT 1, fetch_seq INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX graphics_sources_channel ON graphics_data_sources(channel_id);
CREATE TRIGGER graphics_sources_limit BEFORE INSERT ON graphics_data_sources
WHEN (SELECT COUNT(*) FROM graphics_data_sources WHERE channel_id = NEW.channel_id) >= 16
BEGIN SELECT RAISE(ABORT, 'At most 16 data sources per channel'); END;
