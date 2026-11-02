-- The media engine is GStreamer; the log level of its messages is no longer
-- named after FFmpeg. The API still accepts and returns `ffmpeg_level`.
ALTER TABLE config_logging RENAME COLUMN ffmpeg_level TO media_level;
