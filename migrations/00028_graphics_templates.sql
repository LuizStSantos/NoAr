-- Graphics templates: lower thirds, topic bars, sponsor bugs and other
-- graphics a channel designs once and puts on air many times. A template is
-- a list of layers (text, image, box) placed in percent of the frame, with
-- {fields} that are filled in each time it goes on air.
CREATE TABLE graphics_templates (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    channel_id INTEGER NOT NULL REFERENCES channels (id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    -- Free label to sort templates in the library ("Lower thirds", ...).
    group_name TEXT NOT NULL DEFAULT '',
    -- Seconds on air before it goes out by itself; 0 stays until taken out.
    duration REAL NOT NULL DEFAULT 8 CHECK (duration >= 0),
    -- What it does when it meets an L-shape: follow, wait or skip.
    during_lshape TEXT NOT NULL DEFAULT 'follow',
    -- JSON list of layers (see TemplateLayer).
    layers TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX graphics_templates_name ON graphics_templates (channel_id, name COLLATE NOCASE);
