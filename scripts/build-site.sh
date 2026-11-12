#!/bin/sh
# Put the public site together in docs/: the front page (site/), and the web
# app (dist/, built by `pnpm build`) under app/. Any static host can serve it.
set -e
cd "$(dirname "$0")/.."
V=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
[ -f dist/index.html ] || { echo "build the app first: pnpm build" >&2; exit 1; }
rm -rf docs
mkdir -p docs/img
cp -r dist docs/app
sed "s/@VERSION@/$V/g" site/index.html > docs/index.html
cp site/img/* docs/img/
cp guide/img/desk-loops.jpg guide/img/desk-roll.jpg guide/img/desk-song.jpg guide/img/desk-mix.jpg docs/img/
cp ui/src/assets/fonts/PaperMono-VariableFont_wght.woff2 ui/src/assets/grain.png ui/public/favicon.png docs/img/
cp LICENSE docs/LICENSE.txt
# GitHub Pages: serve the files as they are
touch docs/.nojekyll
# a custom domain, when there is one: put its name in site/CNAME
if [ -f site/CNAME ]; then cp site/CNAME docs/CNAME; fi
