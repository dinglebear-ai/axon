#!/bin/bash
set -euo pipefail

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
export AXON_BENCH_LIBRARY_MODE=1
# shellcheck source=bench-source-pipeline.sh
source "$SCRIPT_DIR/bench-source-pipeline.sh"

fail() { echo "FAIL: $*" >&2; exit 1; }
validate_job_id '123e4567-e89b-12d3-a456-426614174000' || fail valid-job-id
! validate_job_id "x' OR 1=1 --" || fail injected-job-id
validate_safe_source 'https://example.invalid/path with spaces?q="quoted"' || fail safe-source
! validate_safe_source 'https://user:password@example.invalid/' || fail userinfo-source
! validate_safe_source 'https://example.invalid/$(touch /tmp/nope)' || fail substitution-source
( export AXON_BENCH_MLX_URL=http://100.64.0.1:8084; ! metrics_get /tmp/axon-forbidden-metrics 2>/dev/null ) || fail non-loopback-metrics

fixture_dir=$(mktemp -d "${TMPDIR:-/tmp}/axon-bench-test.XXXXXX")
trap 'rm -rf -- "$fixture_dir"' EXIT HUP INT TERM
python3 - "$fixture_dir/jobs.db" <<'PY'
import sqlite3
import sys

with sqlite3.connect(sys.argv[1]) as connection:
    connection.executescript("""
        CREATE TABLE jobs (job_id TEXT PRIMARY KEY, source_id TEXT);
        CREATE TABLE sources (source_id TEXT PRIMARY KEY, committed_generation TEXT);
        CREATE TABLE source_items (
            source_id TEXT, source_item_key TEXT, generation TEXT, content_hash TEXT
        );
        INSERT INTO jobs VALUES ('123e4567-e89b-12d3-a456-426614174000', 'source-1');
        INSERT INTO sources VALUES ('source-1', 'generation-1');
        INSERT INTO source_items VALUES ('source-1', 'b', 'generation-1', 'hash-b');
        INSERT INTO source_items VALUES ('source-1', 'a', 'generation-1', 'hash-a');
    """)
PY
first_hash=$(corpus_hash_from_sqlite "$fixture_dir/jobs.db" '123e4567-e89b-12d3-a456-426614174000')
second_hash=$(corpus_hash_from_sqlite "$fixture_dir/jobs.db" '123e4567-e89b-12d3-a456-426614174000')
[[ $first_hash == "$second_hash" && ${#first_hash} -eq 64 ]] || fail corpus-hash

sanitized=$(printf '%s\n' 'Authorization:Bearer-secret https://user:pass@example.test abcdefghijklmnopqrstuvwxyz123456' | sanitize_text)
[[ $sanitized != *Bearer-secret* ]] || fail bearer-redaction
[[ $sanitized != *example.test* ]] || fail url-redaction
[[ $sanitized != *abcdefghijklmnopqrstuvwxyz123456* ]] || fail long-value-redaction

cat >"$fixture_dir/axon.log" <<'EOF'
{"timestamp":"2026-08-28T00:00:00Z","level":"INFO","message":"web acquisition batch timing","lane":"concurrent","item_count":"16","concurrency":"16","wall_ms":"420","first_completion_ms":"35","item_p50_ms":"80","item_p95_ms":"390","item_max_ms":"410","max_completion_gap_ms":"210","slot_occupancy_permille":"440"}
{"timestamp":"2026-08-28T00:00:01Z","level":"INFO","message":"unrelated"}
EOF
timings=$(acquisition_timings_from_log "$fixture_dir/axon.log")
[[ $(jq 'length' <<<"$timings") -eq 1 ]] || fail acquisition-timing-count
[[ $(jq -r '.[0].item_p95_ms' <<<"$timings") == 390 ]] || fail acquisition-timing-p95
[[ $(jq -r '.[0].slot_occupancy' <<<"$timings") == 0.44 ]] || fail acquisition-slot-occupancy

grep -q 'umask 077' "$SCRIPT_DIR/bench-source-pipeline.sh" || fail umask
grep -q 'mktemp -d' "$SCRIPT_DIR/bench-source-pipeline.sh" || fail private-temp
! grep -Eq '(^|[[:space:]])set -x([[:space:]]|$)' "$SCRIPT_DIR/bench-source-pipeline.sh" || fail xtrace
! grep -Eq '(^|[[:space:]])eval([[:space:]]|$)' "$SCRIPT_DIR/bench-source-pipeline.sh" || fail dynamic-evaluation

echo 'bench-source-pipeline tests passed'
