-- When an occurrence went on air (local time of the channel) and the
-- programme's time shift at that moment. A playout restart during the event
-- uses them to resume it where it was instead of falling back to the playlist.
ALTER TABLE schedule_runs ADD COLUMN on_air_at TEXT;
ALTER TABLE schedule_runs ADD COLUMN shift_before REAL;
