-- Version the bound layout itself so installations already converted by 00029
-- upgrade once without importing frozen processing columns or replacing IDs.
ALTER TABLE graphics_channel_bindings ADD COLUMN layers_version INTEGER NOT NULL DEFAULT 0;
ALTER TABLE graphics_channel_bindings ADD COLUMN control_layer_id TEXT;
