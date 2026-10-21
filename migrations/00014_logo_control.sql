-- The channel icon of the XMLTV guide, when it differs from the logo on air.
ALTER TABLE config_processing ADD COLUMN guide_logo TEXT NOT NULL DEFAULT '';
-- Key an encoder sends to hide, show or swap the logo on air; empty turns that off.
ALTER TABLE config_processing ADD COLUMN logo_key TEXT NOT NULL DEFAULT '';
