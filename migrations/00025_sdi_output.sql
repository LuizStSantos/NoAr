-- SDI output: the program on a baseband video card (Blackmagic DeckLink).
-- Every configuration gets an inactive SDI output. stream_url keeps the card
-- and mode as decklink://<device>?mode=<mode>; the codec settings only encode
-- the Player preview.
INSERT INTO config_output (config_id, active, name, stream_url, width, height, fps, video_codec, video_options, audio_codec, audio_bitrate)
SELECT c.id, 0, 'sdi', 'decklink://0?mode=1080i5994', 1920, 1080, 29.97, 'libx264',
       '{"preset":"faster","rate_control":"crf","quality":"23","maxrate":"2400"}', 'aac', 128
FROM config c
WHERE NOT EXISTS (
    SELECT 1 FROM config_output existing WHERE existing.config_id = c.id AND existing.name = 'sdi'
);
