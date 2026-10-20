-- How NoAr looks for new versions of itself. One row.
CREATE TABLE config_update (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    -- Look for a newer image every six hours.
    auto_check INTEGER NOT NULL DEFAULT 1
);

INSERT INTO config_update (id) VALUES (1);
