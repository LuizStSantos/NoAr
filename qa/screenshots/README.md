# README screenshots

The pictures in `docs/images/` come from a disposable NoAr server filled with a
demo channel, "Example TV". Never point these scripts at a real channel.

1. Start a fresh server whose setup is not done yet, with its storage under a
   folder of its own (for example `/srv/noar-stage/media`), on port 8787.
2. Fill it: titled videos made with FFmpeg, guide sheets and artwork, a
   commercials folder, a live, an assembled program, marks, today's playlist,
   scheduled events and the channel logo; then the playout starts.

   ```sh
   python3 qa/screenshots/build_demo.py --url http://127.0.0.1:8787 \
       --storage /srv/noar-stage/media/1
   ```

   The sign-in is `admin` with the password `Example-2026` (`--password`
   changes it).
3. Serve the frontend (`npx vite --port 5757` from the repository root, which
   forwards the API to port 8787), wait half a minute for the playout and take
   the pictures with Playwright: English, dark theme, 1440x900.

   ```sh
   node qa/screenshots/capture.cjs              # all of them
   node qa/screenshots/capture.cjs --only player,report
   ```

   A screen that fails leaves `docs/images/<name>.failed.png` to look at; do
   not commit those.

The scheduled events are placed after the current time, so take the pictures
right after filling the server.
