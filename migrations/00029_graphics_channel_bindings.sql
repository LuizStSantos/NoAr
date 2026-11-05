-- Native layout and scene actions belong to the template, not processing.
ALTER TABLE graphics_templates ADD COLUMN settings TEXT NOT NULL DEFAULT '{}'
    CHECK (json_valid(settings));

CREATE TABLE graphics_channel_bindings (
    channel_id INTEGER NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('logo', 'ticker', 'lshape')),
    template_id INTEGER NOT NULL REFERENCES graphics_templates(id) ON DELETE RESTRICT,
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    PRIMARY KEY (channel_id, role)
);

CREATE TABLE graphics_scene_bindings (
    channel_id INTEGER NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    template_id INTEGER NOT NULL REFERENCES graphics_templates(id) ON DELETE RESTRICT,
    position INTEGER NOT NULL,
    PRIMARY KEY (channel_id, template_id),
    UNIQUE (channel_id, position)
);
