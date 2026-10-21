-- SCTE-35 ad break signalling on MPEG-TS stream outputs (SRT, UDP).
CREATE TABLE config_scte35 (
    config_id INTEGER PRIMARY KEY,
    enabled INTEGER NOT NULL DEFAULT 0,
    -- PID the splice messages are carried on.
    pid INTEGER NOT NULL DEFAULT 500,
    -- Seconds between the first message of a break and its start.
    preroll REAL NOT NULL DEFAULT 6,
    -- Runs of commercials shorter or longer than this are not signalled.
    min_break REAL NOT NULL DEFAULT 30,
    max_break REAL NOT NULL DEFAULT 300,
    FOREIGN KEY (config_id) REFERENCES config(id) ON UPDATE CASCADE ON DELETE CASCADE
);

INSERT INTO config_scte35 (config_id) SELECT id FROM config;
