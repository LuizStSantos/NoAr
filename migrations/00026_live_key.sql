-- Key that lets an encoder or a script extend or end the live on air through
-- /live/{channel}/{action}, like the logo and SCTE-35 keys; empty turns it off.
ALTER TABLE config_processing ADD COLUMN live_key TEXT NOT NULL DEFAULT '';
