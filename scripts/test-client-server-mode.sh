#!/usr/bin/env bash
# Smoke test the host CLI -> axon serve client/server path.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
AXON_BIN="${AXON_BIN:-$REPO_ROOT/target/debug/axon}"
SERVER_URL="${AXON_SERVER_URL:-http://127.0.0.1:${AXON_MCP_HTTP_PORT:-8001}}"

if [[ ! -x "$AXON_BIN" ]]; then
    cargo build --bin axon
fi

TMP_DIR="$(mktemp -d)"
cleanup() {
    rm -rf "$TMP_DIR"
}
trap cleanup EXIT

HOST_OUTPUT="$TMP_DIR/host-scrape.md"

echo "client-server smoke: axon=$AXON_BIN"
echo "client-server smoke: server=$SERVER_URL"

status_json="$(
    AXON_SERVER_URL="$SERVER_URL" \
    AXON_DATA_DIR="$TMP_DIR/client-data" \
    "$AXON_BIN" status --json
)"
python3 - "$status_json" <<'PY'
import json, sys
payload = json.loads(sys.argv[1])
if "totals" not in payload:
    raise SystemExit("status payload missing totals")
PY

scrape_json="$(
    AXON_SERVER_URL="$SERVER_URL" \
    AXON_DATA_DIR="$TMP_DIR/client-data" \
    "$AXON_BIN" scrape https://example.com --json --embed false --output "$HOST_OUTPUT"
)"
python3 - "$scrape_json" <<'PY'
import json, sys
payload = json.loads(sys.argv[1])
if not (payload.get("artifact_handle") or payload.get("data", {}).get("artifact_handle")):
    raise SystemExit("scrape payload missing artifact_handle")
PY

if [[ -e "$HOST_OUTPUT" ]]; then
    echo "server-mode scrape unexpectedly wrote host output: $HOST_OUTPUT" >&2
    exit 1
fi

echo "client-server smoke: ok"
