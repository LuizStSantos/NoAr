-- What users did: every change, sign-in and keyed external command, with who,
-- where and the result. Kept 180 days. The user is copied by name, so the
-- history stays readable after the user is deleted.
CREATE TABLE activity_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    -- Unix time in seconds.
    at INTEGER NOT NULL,
    -- NULL for a sign-in attempt or an external command with a key.
    user_id INTEGER,
    username TEXT NOT NULL DEFAULT '',
    role TEXT NOT NULL DEFAULT '',
    channel_id INTEGER,
    -- A stable name such as playlist.save; the interface translates it.
    action TEXT NOT NULL,
    -- What it was done to: a date, a name, a file.
    target TEXT NOT NULL DEFAULT '',
    method TEXT NOT NULL,
    route TEXT NOT NULL,
    -- The HTTP status of the answer.
    status INTEGER NOT NULL,
    ip TEXT NOT NULL DEFAULT ''
);

CREATE INDEX activity_log_at ON activity_log (at);
CREATE INDEX activity_log_user_at ON activity_log (username, at);
CREATE INDEX activity_log_channel_at ON activity_log (channel_id, at);
