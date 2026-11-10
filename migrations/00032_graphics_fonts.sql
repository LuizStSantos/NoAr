-- Uploaded faces are immutable and included in the normal database backup.
CREATE TABLE graphics_fonts (
    id TEXT PRIMARY KEY,
    channel_id INTEGER NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    filename TEXT NOT NULL,
    family TEXT NOT NULL UNIQUE,
    size INTEGER NOT NULL CHECK(size > 0 AND size <= 10485760),
    data BLOB NOT NULL CHECK(length(data) = size)
);
CREATE INDEX graphics_fonts_channel ON graphics_fonts(channel_id);
CREATE TRIGGER graphics_fonts_limit BEFORE INSERT ON graphics_fonts
WHEN (SELECT COUNT(*) FROM graphics_fonts WHERE channel_id = NEW.channel_id) >= 32
BEGIN SELECT RAISE(ABORT, 'At most 32 uploaded fonts per channel'); END;
