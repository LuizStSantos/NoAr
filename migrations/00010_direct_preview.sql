-- The Player now shows NoAr's own low-latency copy of the output. The
-- address of the first channel pointed at 127.0.0.1, which only worked in a
-- browser on the server itself: clear it when it was never changed, so the
-- direct preview is used. Addresses set by hand are kept as an option.
UPDATE channels
SET
    preview_url = ''
WHERE
    preview_url = 'http://127.0.0.1:8787/public/1/live/stream.m3u8';
