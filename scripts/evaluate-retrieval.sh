#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
REPO="$(cd -- "$SCRIPT_DIR/.." && pwd -P)"
FIXTURES="${1:-$REPO/docs/eval/retrieval-fixtures.jsonl}"
OUT="${OUT:-$REPO/.cache/axon-rust/evals/retrieval-$(date +%Y%m%d%H%M%S).jsonl}"
SUMMARY="${SUMMARY:-${OUT%.jsonl}.summary.json}"
AXON_BIN="${AXON_BIN:-$REPO/target/debug/axon}"
ALLOW_MISS="${ALLOW_MISS:-0}"

need() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "missing required command: $1" >&2
    exit 2
  }
}

need jq
mkdir -p "$(dirname "$OUT")"
mkdir -p "$(dirname "$SUMMARY")"
: > "$OUT"

jq -s -e '
  length > 0 and
  all(.[]; type == "object"
    and ((.id // "") != "")
    and ((.domain // "") != "")
    and ((.query // "") != "")
    and (.expected as $expected
      | $expected == "selected"
        or $expected == "top_domain"
        or $expected == "known_miss"))
' "$FIXTURES" >/dev/null

while IFS= read -r row; do
  id="$(jq -r '.id' <<<"$row")"
  domain="$(jq -r '.domain' <<<"$row")"
  query="$(jq -r '.query' <<<"$row")"
  expected="$(jq -r '.expected' <<<"$row")"

  if ! payload="$("$AXON_BIN" ask --explain --diagnostics --json "$query" 2> >(sed "s/^/[axon:$id] /" >&2))"; then
    jq -n -c --arg id "$id" --arg domain "$domain" --arg query "$query" --arg expected "$expected" \
      '{id:$id,domain:$domain,query:$query,expected:$expected,status:"axon_failed",top_pass:false,selected_pass:false,pass:false}' >> "$OUT"
    continue
  fi

  jq -c \
    --arg id "$id" \
    --arg domain "$domain" \
    --arg query "$query" \
    --arg expected "$expected" \
    '
    def selected_urls:
      [.explain.candidates[]?
       | select(.selected_context_rank != null)
       | .url];
    def url_host:
      capture("^https?://(?<host>[^/:?#]+)")?.host | ascii_downcase;
    def domain_match($domain):
      ascii_downcase as $host
      | ($domain | ascii_downcase) as $expected
      | ($host == $expected or ($host | endswith("." + $expected)));
    def top_domain_host:
      split(":")[0] | ascii_downcase;
    . as $payload
    | {
        id: $id,
        domain: $domain,
        query: $query,
        expected: $expected,
        status: "ok",
        top_domains: ($payload.diagnostics.top_domains // []),
        selected_urls: (selected_urls[0:20]),
        top_pass: (($payload.diagnostics.top_domains // []) | any(top_domain_host | domain_match($domain))),
        selected_pass: (selected_urls | map(url_host) | any(. != null and domain_match($domain))),
        corpus_health: ($payload.diagnostics.corpus_health // null),
        timing_ms: ($payload.timing_ms // {})
      }
    | .pass = (
        if $expected == "selected" then (.top_pass and .selected_pass)
        elif $expected == "top_domain" then .top_pass
        elif $expected == "known_miss" then true
        else false
        end
      )
    ' <<<"$payload" >> "$OUT"
done < <(jq -c '.' "$FIXTURES")

jq -s --arg output "$OUT" '
  {
    total: length,
    pass: (map(select(.pass)) | length),
    top_pass: (map(select(.top_pass)) | length),
    selected_pass: (map(select(.selected_pass)) | length),
    runtime_failures: map(select(.status != "ok")),
    failures: map(select(.pass | not)),
    output: $output
  }
' "$OUT" > "$SUMMARY"

cat "$SUMMARY"

if jq -e '.runtime_failures | length > 0' "$SUMMARY" >/dev/null; then
  exit 1
fi

if [ "$ALLOW_MISS" != "1" ] && jq -e '.failures | length > 0' "$SUMMARY" >/dev/null; then
  exit 1
fi
