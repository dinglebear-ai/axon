#!/usr/bin/env bash
#
# Package the Axon Chrome extension into a distributable ZIP.
#
# The `assets/` entry in this directory is a symlink into the monorepo's
# top-level `assets/`. Chrome's "Load unpacked" follows that symlink locally,
# but a distributable ZIP (and the Chrome Web Store) requires real files with
# no symlinks. This script stages the runtime files, copies only the assets
# actually referenced by the manifest/HTML/JS as real files, and zips them.
#
# Usage:
#   ./package.sh            # -> dist/axon-<version>.zip
#
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$here"

# Version comes straight from the manifest so the two never drift. Anchor to a
# top-level "version" key so "manifest_version" can never be matched by mistake.
version="$(grep -m1 -E '^[[:space:]]*"version"[[:space:]]*:' manifest.json \
  | sed -E 's/.*:[[:space:]]*"([^"]+)".*/\1/')"
if [[ -z "$version" ]]; then
  echo "error: could not read version from manifest.json" >&2
  exit 1
fi

out_dir="$here/dist"
out_zip="$out_dir/axon-${version}.zip"
stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT

# Runtime files that ship in the extension (everything except dev-only files).
for f in manifest.json *.html *.js *.css; do
  [[ -e "$f" ]] && cp "$f" "$stage/"
done

# Discover the assets actually referenced, then copy each as a real file
# (cp dereferences the symlink), preserving its relative path under the stage.
# Use a read loop instead of `mapfile` so bash 3.2 (macOS system bash) works.
refs=()
while IFS= read -r ref; do
  refs+=("$ref")
done < <(
  grep -rhoE "assets/[A-Za-z0-9_./-]+\.(png|svg|jpg|jpeg|webp|ico|gif)" \
    manifest.json ./*.html ./*.js ./*.css 2>/dev/null | sort -u
)
if [[ ${#refs[@]} -eq 0 ]]; then
  echo "error: no asset references found — refusing to ship without icons" >&2
  exit 1
fi
for rel in "${refs[@]}"; do
  if [[ ! -e "$rel" ]]; then
    echo "error: referenced asset is missing: $rel" >&2
    exit 1
  fi
  mkdir -p "$stage/$(dirname "$rel")"
  cp "$rel" "$stage/$rel"
done

mkdir -p "$out_dir"
rm -f "$out_zip"
# Zip from inside the stage so paths are relative to the extension root.
# No -y: store real files, never symlinks.
( cd "$stage" && zip -rq "$out_zip" . -x '*.DS_Store' )

echo "Packaged ${#refs[@]} asset(s) + runtime files"
echo "$out_zip"
