# NoAr website

`index.html` is the project's one-page website, published on GitHub Pages at
<https://luizstsantos.github.io/NoAr/>. It is plain HTML, CSS and a little
JavaScript, with no build step and no requests to other sites except one
optional call to the GitHub API for the newest release number.

- **Languages**: English in the page itself; Portuguese from the `pt` entries of
  `TEXT` in the script. A visitor's choice is kept in the browser, and
  `?lang=pt` or `?lang=en` opens one directly. When you change a text, change
  both.
- **Screenshots**: the page shows the README gallery in `docs/images`, so
  recapturing it ([screenshot guide](../qa/screenshots/README.md)) updates the
  site too. `SCREENS` in the script lists them, with how much of each 1440×1000
  capture is shown.
- **Fonts**: IBM Plex Sans and Mono from `@fontsource`, under the SIL Open Font
  License (`fonts/OFL.txt`).

## Look at it locally

```sh
sh site/build.sh && python3 -m http.server -d _site 8000
```

Then open <http://127.0.0.1:8000>. `build.sh` puts the page, its fonts and the
screenshots it shows in `_site/`, which git ignores.

## Publishing

`.github/workflows/pages.yml` runs `build.sh` and deploys `_site` whenever
`site/` or `docs/images/` change on `main`, or by hand from **Actions ›
Website › Run workflow**. Once, in **Settings › Pages**, set **Source** to
**GitHub Actions**; until then the deploy step fails.
