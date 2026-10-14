#!/bin/sh
# Bundle the Documents editor (entry.js) into ../vendor/editor.js.
# Run after changing entry.js or the versions in package.json:
#   cd src/ui/editor && npm ci && sh build.sh
set -e
cd "$(dirname "$0")"
mkdir -p ../vendor
npx esbuild entry.js --bundle --minify --format=iife --target=safari15 \
  --legal-comments=eof --outfile=../vendor/editor.js
ls -l ../vendor/editor.js
