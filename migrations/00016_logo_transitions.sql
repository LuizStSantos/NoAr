-- How the logo comes up and goes off: 'fade' or 'cut'.
ALTER TABLE config_processing ADD COLUMN logo_transition_in TEXT NOT NULL DEFAULT 'fade';
ALTER TABLE config_processing ADD COLUMN logo_transition_out TEXT NOT NULL DEFAULT 'fade';
-- Length of the logo's fades and cross-fades in seconds; NULL keeps the built-in lengths.
ALTER TABLE config_processing ADD COLUMN logo_transition_seconds REAL;
