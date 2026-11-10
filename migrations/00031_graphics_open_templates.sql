-- A completed import is independent of whether an operator keeps its templates.
CREATE TABLE graphics_channel_state (
    channel_id INTEGER PRIMARY KEY REFERENCES channels(id) ON DELETE CASCADE
);
INSERT INTO graphics_channel_state(channel_id)
SELECT channel_id FROM graphics_channel_bindings GROUP BY channel_id HAVING COUNT(*) = 3;

-- NULL remains meaningful for action scenes and video-window templates.
ALTER TABLE graphics_templates ADD COLUMN permanent INTEGER CHECK (permanent IN (0, 1));
UPDATE graphics_templates SET permanent = (
    SELECT enabled FROM graphics_channel_bindings b
    WHERE b.channel_id = graphics_templates.channel_id AND b.template_id = graphics_templates.id
      AND b.role IN ('logo', 'ticker')
);
UPDATE graphics_templates SET permanent = 0 WHERE permanent IS NULL
AND NOT EXISTS(SELECT 1 FROM graphics_channel_bindings b WHERE b.template_id = graphics_templates.id)
AND NOT EXISTS(SELECT 1 FROM graphics_scene_bindings b WHERE b.template_id = graphics_templates.id);

-- Protect concurrent writers as well as the API preflight capacity check.
CREATE TRIGGER graphics_permanent_insert_limit BEFORE INSERT ON graphics_templates
WHEN NEW.permanent = 1 AND (SELECT COUNT(*) FROM graphics_templates
    WHERE channel_id = NEW.channel_id AND permanent = 1) >= 16
BEGIN SELECT RAISE(ABORT, 'At most 16 permanent templates per channel'); END;
CREATE TRIGGER graphics_permanent_update_limit BEFORE UPDATE OF permanent ON graphics_templates
WHEN NEW.permanent = 1 AND OLD.permanent IS NOT 1 AND (SELECT COUNT(*) FROM graphics_templates
    WHERE channel_id = NEW.channel_id AND permanent = 1) >= 16
BEGIN SELECT RAISE(ABORT, 'At most 16 permanent templates per channel'); END;
