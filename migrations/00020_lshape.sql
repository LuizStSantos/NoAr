-- The L-shape: a full-screen graphic the picture shrinks into, relative to the
-- storage (empty turns it off), how big the picture is in it (percent of the
-- screen) and the corner it goes to.
ALTER TABLE config_processing ADD COLUMN lshape TEXT NOT NULL DEFAULT '';
ALTER TABLE config_processing ADD COLUMN lshape_scale REAL NOT NULL DEFAULT 75;
ALTER TABLE config_processing ADD COLUMN lshape_corner TEXT NOT NULL DEFAULT 'top-right';
