-- Installing new versions through the updater container.
ALTER TABLE config_update ADD COLUMN auto_update INTEGER NOT NULL DEFAULT 0;
-- Server time, HH:MM, at which a new version installs itself.
ALTER TABLE config_update ADD COLUMN update_time TEXT NOT NULL DEFAULT '03:00';
-- A one-off install the operator scheduled, Unix milliseconds.
ALTER TABLE config_update ADD COLUMN scheduled_at INTEGER;
