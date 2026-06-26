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
export QDRANT_URL="http://127.0.0.1:53333"
export TEI_URL="http://127.0.0.1:52000"
export AXON_CHROME_REMOTE_URL="http://127.0.0.1:6000"
export AXON_FRESHNESS_TICK_SECS=1
export AXON_FRESHNESS_MAX_DUE_PER_TICK=2
export AXON_FRESHNESS_MAX_CONCURRENT_RUNS=1

COLLECTION="axon-freshness-smoke-$(date +%s)"
cd "$ROOT"

require_jq() {
    if ! command -v jq >/dev/null 2>&1; then
        echo "jq is required for scripts/smoke-freshness.sh" >&2
        exit 127
    fi
}

require_jq

SCRAPE_JSON="$(./scripts/axon scrape https://example.com --collection "$COLLECTION" --fresh 1d --json --skip-embed)"
SCRAPE_ID="$(printf '%s' "$SCRAPE_JSON" | jq -r '.id')"
test "$SCRAPE_ID" != "null"
./scripts/axon fresh run-now "$SCRAPE_ID" --json | jq -e '.status == "completed" or .status == "enqueued"'

RSS_JSON="$(./scripts/axon ingest rss:https://github.com/jmagar/axon/releases.atom --collection "$COLLECTION" --fresh 1d --json)"
RSS_ID="$(printf '%s' "$RSS_JSON" | jq -r '.id')"
test "$RSS_ID" != "null"
./scripts/axon fresh run-now "$RSS_ID" --json | jq -e '.status == "completed" or .status == "enqueued"'

./scripts/axon fresh history "$SCRAPE_ID" --json | jq -e '.items | length >= 1'
