-- Quick graphics scenes: named sets of logo and L-shape changes applied in
-- one go, from NoAr or an encoder (JSON list, see Processing::graphics_scenes).
ALTER TABLE config_processing ADD COLUMN graphics_scenes TEXT NOT NULL DEFAULT '[]';
