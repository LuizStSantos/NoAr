-- The on-screen text overlay (Legenda page) was removed from NoAr.
DROP INDEX IF EXISTS config_text_presets_one_persistent;

DROP TABLE IF EXISTS config_text_presets;

-- NoAr runs in Docker and keeps its data under /data. Move recordings that
-- still use the old packaged default into the data volume.
UPDATE config_recording
SET
    path = '/data/recordings/' || (
        SELECT
            channel_id
        FROM
            config
        WHERE
            config.id = config_recording.config_id
    )
WHERE
    path LIKE '/var/lib/ffplayout/recordings%';
