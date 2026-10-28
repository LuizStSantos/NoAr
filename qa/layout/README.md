# NoAr layout check

`check-layout.cjs` opens every page of a **disposable staging** NoAr in Chromium at computer widths (`--profile desktop`: 1280, 1366, 1440, 1920 px) or phone and tablet widths (`--profile mobile`: 320, 360, 390 px with touch emulation, 820 and 1024 px), and reports per page:

- the page scrolling sideways;
- elements past the edge of the screen (outside any scrolling area);
- short words split across two lines;
- the main menu on more than one line (desktop), or the menu button missing (below 1280 px);
- page errors.

It saves a full-page screenshot of each page and `report.json` in `--out`, and exits with 1 when a page has a problem. It only loads pages and logs in: it saves nothing.

```sh
NOAR_E2E_STAGING=1 NOAR_LAYOUT_USER=<test user> NOAR_LAYOUT_PASSWORD=<test password> \
  node qa/layout/check-layout.cjs --profile mobile --base http://127.0.0.1:8787 --out /tmp/noar-layout
```

Other options: `--widths 360,390`, `--routes /,/player,/schedule`. In development the Vite server can be the base (`--base http://127.0.0.1:5757`).

It refuses to run without `NOAR_E2E_STAGING=1`, and against anything but a loopback host unless `NOAR_E2E_ALLOW_REMOTE=1` (only for a reviewed disposable host). Use a test-only account, never an on-air instance. It needs Playwright with Chromium (`require('playwright')`).

The agents `noar-desktop-layout` and `noar-mobile-layout` use it; see `.claude/agents/`. Screenshots go to `--out` (default `qa/layout/results/`, ignored by git); do not commit them.
