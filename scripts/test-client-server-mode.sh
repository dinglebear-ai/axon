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
LAST_JSON=""

run_json() {
    local name="$1"
    shift
    LAST_JSON="$TMP_DIR/axon-${name}.json"
    "$@" >"$LAST_JSON"
}

echo "client-server smoke: axon=$AXON_BIN"
echo "client-server smoke: server=$SERVER_URL"

run_json "status" \
    env AXON_SERVER_URL="$SERVER_URL" \
    AXON_DATA_DIR="$TMP_DIR/client-data" \
    "$AXON_BIN" status --json
python3 - "$LAST_JSON" <<'PY'
import json, sys
payload = json.load(open(sys.argv[1]))
if "totals" not in payload:
    raise SystemExit("status payload missing totals")
PY

run_json "scrape" \
    env AXON_SERVER_URL="$SERVER_URL" \
    AXON_DATA_DIR="$TMP_DIR/client-data" \
    "$AXON_BIN" scrape https://example.com --json --skip-embed --output "$HOST_OUTPUT"
python3 - "$LAST_JSON" <<'PY'
import json, sys
payload = json.load(open(sys.argv[1]))
if not (payload.get("artifact_handle") or payload.get("data", {}).get("artifact_handle") or payload.get("url")):
    raise SystemExit("scrape payload missing artifact handle or url")
PY

run_json "extract_wait_json_rest" \
    env AXON_SERVER_URL="$SERVER_URL" \
    AXON_DATA_DIR="$TMP_DIR/client-data" \
    "$AXON_BIN" extract https://www.rfc-editor.org/rfc/rfc9110.txt \
    --query "Extract title and document type" \
    --wait true \
    --json \
    --skip-embed \
    --render-mode http
python3 - "$LAST_JSON" <<'PY'
import json, sys
payload = json.load(open(sys.argv[1]))
result = payload.get("result", payload)
extract_result = result.get("extract_result", result)
if extract_result.get("total_items", 0) < 1:
    raise SystemExit("extract wait payload missing extracted items")
PY

if [[ -e "$HOST_OUTPUT" ]]; then
    echo "server-mode scrape unexpectedly wrote host output: $HOST_OUTPUT" >&2
    exit 1
fi

echo "client-server smoke: ok"
