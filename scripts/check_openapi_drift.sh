#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

if [ ! -d apps/web/node_modules ]; then
  npm ci --prefix apps/web
fi

npm --prefix apps/web run openapi:generate

if ! command -v pnpm >/dev/null 2>&1; then
  echo "ERROR: pnpm is required to check Palette OpenAPI type drift" >&2
  exit 1
fi

if [ ! -d apps/palette-tauri/node_modules ]; then
  pnpm --dir apps/palette-tauri install --frozen-lockfile
fi

pnpm --dir apps/palette-tauri generate:api

drifted="$(
  git diff --name-only -- \
    apps/web/openapi/axon.json \
    apps/web/lib/generated/axon-api.ts \
    apps/palette-tauri/src/lib/axon-api.d.ts
)"

if [ -n "$drifted" ]; then
  echo "ERROR: OpenAPI generated artifacts are out of date:" >&2
  echo "$drifted" >&2
  echo >&2
  echo "Run scripts/check_openapi_drift.sh and commit the regenerated files." >&2
  exit 1
fi

git diff --quiet -- \
  apps/web/openapi/axon.json \
  apps/web/lib/generated/axon-api.ts \
  apps/palette-tauri/src/lib/axon-api.d.ts
