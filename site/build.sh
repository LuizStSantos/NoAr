#!/bin/sh
# Assemble the NoAr website into one folder (default _site): the page, its
# fonts and the README screenshots it shows. GitHub Pages publishes that
# folder (.github/workflows/pages.yml); to look at it locally:
#   sh site/build.sh && python3 -m http.server -d _site 8000
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
out=${1:-$root/_site}

rm -rf "$out"
mkdir -p "$out/docs/images"
cp "$root/site/index.html" "$root/site/favicon.svg" "$out/"
cp -R "$root/site/fonts" "$out/fonts"
for name in player scheduler builder storage onair composer marks report remote login social-preview; do
    cp "$root/docs/images/$name.png" "$out/docs/images/"
done
touch "$out/.nojekyll"
echo "Website assembled in $out"
