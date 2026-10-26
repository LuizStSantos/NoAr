-- Convert uploaded videos to the channel's output format (size, frame rate,
-- sample rate) once they arrive, instead of on air. Off by default.
ALTER TABLE config_storage ADD COLUMN convert_uploads INTEGER NOT NULL DEFAULT 0;
