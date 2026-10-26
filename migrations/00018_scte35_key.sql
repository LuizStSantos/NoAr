-- A key for encoders and automation to mark SCTE-35 breaks without logging
-- in, like the logo key. Empty turns the keyed route off.
ALTER TABLE config_scte35 ADD COLUMN api_key TEXT NOT NULL DEFAULT '';
