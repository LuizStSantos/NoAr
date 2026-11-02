-- NDI output: the uncompressed program published as an NDI source. Every
-- configuration gets an inactive NDI output, sized like its active output.
-- stream_url keeps the NDI source name; the codec settings only encode the
-- Player preview.
INSERT INTO config_output (config_id, active, name, stream_url, width, height, fps, video_codec, video_options, audio_codec, audio_bitrate)
SELECT c.id, 0, 'ndi', 'NoAr', COALESCE(o.width, 1280), COALESCE(o.height, 720), COALESCE(o.fps, 25.0), 'libx264',
       '{"preset":"faster","rate_control":"crf","quality":"23","maxrate":"2400"}', 'aac', 128
FROM config c
LEFT JOIN config_output o ON o.config_id = c.id AND o.active = 1
WHERE NOT EXISTS (
    SELECT 1 FROM config_output existing WHERE existing.config_id = c.id AND existing.name = 'ndi'
);
