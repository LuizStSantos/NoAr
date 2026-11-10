-- Existing scene-action events remain unchanged. New events select a channel model.
ALTER TABLE graphics_events ADD COLUMN template TEXT;
