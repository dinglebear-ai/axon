#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

mkdir -p "$TMP/data"
: >"$TMP/.env"
: >"$TMP/config.toml"

export AXON_ENV_FILE="$TMP/.env"
export AXON_CONFIG_PATH="$TMP/config.toml"
export AXON_DATA_DIR="$TMP/data"
export AXON_SQLITE_PATH="$TMP/jobs.db"
export QDRANT_URL="${QDRANT_URL:-http://127.0.0.1:9}"
export TEI_URL="${TEI_URL:-http://127.0.0.1:9}"
export AXON_CHROME_REMOTE_URL="${AXON_CHROME_REMOTE_URL:-http://127.0.0.1:6000}"

cd "$ROOT"

require_jq() {
    if ! command -v jq >/dev/null 2>&1; then
        echo "jq is required for scripts/smoke-watch.sh" >&2
        exit 127
    fi
}

require_jq

cargo build -q --locked --bin axon
AXON_BIN="$ROOT/target/debug/axon"

WATCH_JSON="$("$AXON_BIN" watch https://example.com --every-seconds 3600 --json)"
WATCH_ID="$(printf '%s' "$WATCH_JSON" | jq -r '.watch_id')"
test "$WATCH_ID" != "null"
printf '%s' "$WATCH_JSON" | jq -e '.canonical_uri == "https://example.com" and .enabled == true'

"$AXON_BIN" watch exec https://example.com --json \
    | jq -e '.kind == "source" and (.id | type == "string")'

"$AXON_BIN" watch status https://example.com --json \
    | jq -e '.watch.watch_id == "'"$WATCH_ID"'" and .latest_job_summary.kind == "source"'

"$AXON_BIN" watch history https://example.com --json \
    | jq -e '.watch_id == "'"$WATCH_ID"'" and (.jobs | length) >= 1 and .jobs[0].kind == "source"'
