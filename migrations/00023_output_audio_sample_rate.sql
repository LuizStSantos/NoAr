-- Sample rate of an encoded output's audio. The engine keeps processing at
-- 48 kHz and resamples just before the encoder. NULL keeps 48 kHz, which is
-- what every existing output encodes today.
ALTER TABLE config_output ADD COLUMN audio_sample_rate INTEGER
    CHECK (audio_sample_rate IS NULL OR audio_sample_rate IN (32000, 44100, 48000));
