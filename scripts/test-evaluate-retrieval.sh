#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
REPO="$(cd -- "$SCRIPT_DIR/.." && pwd -P)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

cat >"$TMP/fake-axon" <<'FAKE'
#!/usr/bin/env bash
set -euo pipefail
query="${@: -1}"
case "$query" in
  "selected")
    cat <<'JSON'
{"diagnostics":{"top_domains":["docs.example.com:2"]},"explain":{"candidates":[{"url":"https://docs.example.com/page","selected_context_rank":1}]},"timing_ms":{}}
JSON
    ;;
  "top-only")
    cat <<'JSON'
{"diagnostics":{"top_domains":["docs.example.com:2"]},"explain":{"candidates":[{"url":"https://other.example.com/page","selected_context_rank":1}]},"timing_ms":{}}
JSON
    ;;
  "known-miss" | "miss")
    cat <<'JSON'
{"diagnostics":{"top_domains":["other.example.com:2"]},"explain":{"candidates":[{"url":"https://other.example.com/page","selected_context_rank":1}]},"timing_ms":{}}
JSON
    ;;
  *)
    exit 9
    ;;
esac
FAKE
chmod +x "$TMP/fake-axon"

cat >"$TMP/fixtures.jsonl" <<'JSONL'
{"id":"selected","domain":"docs.example.com","query":"selected","expected":"selected"}
{"id":"top","domain":"docs.example.com","query":"top-only","expected":"top_domain"}
{"id":"known","domain":"docs.example.com","query":"known-miss","expected":"known_miss"}
{"id":"miss","domain":"docs.example.com","query":"miss","expected":"selected"}
JSONL

if AXON_BIN="$TMP/fake-axon" OUT="$TMP/default.jsonl" "$REPO/scripts/evaluate-retrieval.sh" "$TMP/fixtures.jsonl" >"$TMP/default-summary.json"; then
  echo "expected default run to fail on miss" >&2
  exit 1
fi

ALLOW_MISS=1 AXON_BIN="$TMP/fake-axon" OUT="$TMP/allow-miss.jsonl" "$REPO/scripts/evaluate-retrieval.sh" "$TMP/fixtures.jsonl" >"$TMP/summary.json"
jq -e '.total == 4 and .pass == 3 and (.failures | length == 1)' "$TMP/summary.json" >/dev/null

: >"$TMP/empty.jsonl"
if ALLOW_MISS=1 AXON_BIN="$TMP/fake-axon" OUT="$TMP/empty-output.jsonl" "$REPO/scripts/evaluate-retrieval.sh" "$TMP/empty.jsonl" >/dev/null 2>&1; then
  echo "expected empty fixture file to fail validation" >&2
  exit 1
fi
