#!/bin/bash
set -euo pipefail
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
fail() { echo "FAIL: $*" >&2; exit 1; }

export AXON_BENCH_LIBRARY_MODE=1
# shellcheck source=bench-source-pipeline.sh
source "$SCRIPT_DIR/bench-source-pipeline.sh"
validate_job_id 123e4567-e89b-12d3-a456-426614174000 || fail job-id
! validate_job_id "x' OR 1=1 --" || fail injected-id
validate_safe_source 'https://example.invalid/path with spaces' || fail safe-source
! validate_safe_source 'https://user:pass@example.invalid/' || fail userinfo
( export AXON_BENCH_MLX_URL=http://100.64.0.1:8084; ! metrics_get /tmp/forbidden 2>/dev/null ) || fail loopback

root=$(mktemp -d "${TMPDIR:-/tmp}/axon-bench-test.XXXXXX")
trap 'rm -rf -- "$root"' EXIT HUP INT TERM
mkdir "$root/bin" "$root/replay"; touch "$root/env"
curl_log=$root/curl.log; axon_log=$root/axon-argv.log; metrics_count=$root/metrics-count

# Fake curl implements Qdrant absence/deletion and MLX info/metrics.
printf '%s\n' '#!/bin/bash' 'set -euo pipefail' \
  'printf "%s\n" "$*" >>"$FAKE_CURL_LOG"' \
  'method=GET; output=; url=${!#}; while (($#)); do case $1 in --request) method=$2; shift 2;; --output) output=$2; shift 2;; *) shift;; esac; done' \
  'if [[ $url == */collections/* && $method == GET ]]; then exit 22; fi' \
  'if [[ $url == */collections/* && $method == DELETE ]]; then printf "{\"result\":true}" >"$output"; exit; fi' \
  'if [[ $url == */info ]]; then printf "{\"model_id\":\"observed/fake\",\"revision\":\"abc\",\"dimensions\":1024,\"tokenizer_limit\":8192}" >"$output"; exit; fi' \
  'n=0; [[ ! -f $FAKE_METRICS_COUNT ]] || n=$(cat "$FAKE_METRICS_COUNT"); echo $((n+1)) >"$FAKE_METRICS_COUNT"' \
  'if ((n==0)); then req=0; tok=0; busy=0; else req=2; tok=200; busy=100000; fi' \
  'printf "{\"epoch\":\"0123456789abcdef0123456789abcdef\",\"requests\":%s,\"useful_tokens\":%s,\"padded_tokens\":%s,\"dispatches\":%s,\"partial_dispatches\":0,\"row_capacity\":%s,\"token_capacity\":%s,\"tokenize_us\":0,\"serialize_us\":0,\"request_wall_us\":200000,\"metal_busy_us\":%s,\"dispatcher_idle_us\":100000,\"rows_total\":%s}" "$req" "$tok" "$tok" "$req" "$req" "$tok" "$busy" "$req" >"$output"' >"$root/bin/curl"
chmod +x "$root/bin/curl"

# Fake Axon proves argv and creates the minimum committed evidence database.
printf '%s\n' '#!/bin/bash' 'set -euo pipefail' 'printf "%s\n" "$*" >>"$FAKE_AXON_LOG"' \
  'if [[ ${1-} == config ]]; then printf "{\"pipeline\":{\"depth\":3},\"providers\":{\"embedding\":{\"url\":\"http://secret\",\"api_key\":\"do-not-leak\",\"batch_size\":16}}}"; exit; fi' \
  '[[ ${FAKE_AXON_SLEEP:-0} == 0 ]] || sleep "$FAKE_AXON_SLEEP"' '[[ ${FAKE_AXON_FAIL:-0} == 0 ]] || { echo "Bearer-super-secret https://secret.invalid" >&2; exit 17; }' \
  'python3 - "$AXON_SQLITE_PATH" <<"PY"' 'import os,sqlite3,sys' 'os.makedirs(os.path.dirname(sys.argv[1])+"/logs",exist_ok=True)' 'with sqlite3.connect(sys.argv[1]) as c:' ' c.executescript("CREATE TABLE jobs(job_id TEXT,source_id TEXT);CREATE TABLE sources(source_id TEXT,committed_generation TEXT);CREATE TABLE source_items(source_id TEXT,source_item_key TEXT,generation TEXT,content_hash TEXT);CREATE TABLE job_stages(job_id TEXT,phase TEXT,started_at TEXT,completed_at TEXT);CREATE TABLE job_events(job_id TEXT,phase TEXT,timestamp TEXT);INSERT INTO jobs VALUES(\"123e4567-e89b-12d3-a456-426614174000\",\"s\");INSERT INTO sources VALUES(\"s\",\"g\");INSERT INTO source_items VALUES(\"s\",\"a\",\"g\",\"h\");INSERT INTO job_stages VALUES(\"123e4567-e89b-12d3-a456-426614174000\",\"fetch\",\"2026-08-31T00:00:00Z\",\"2026-08-31T00:00:00.04Z\");INSERT INTO job_stages VALUES(\"123e4567-e89b-12d3-a456-426614174000\",\"prepare\",\"2026-08-31T00:00:00.02Z\",\"2026-08-31T00:00:00.06Z\");INSERT INTO job_stages VALUES(\"123e4567-e89b-12d3-a456-426614174000\",\"publish\",\"2026-08-31T00:00:00.08Z\",\"2026-08-31T00:00:00.10Z\");INSERT INTO job_stages VALUES(\"123e4567-e89b-12d3-a456-426614174000\",\"publish\",\"2026-08-31T00:00:00.08Z\",\"2026-08-31T00:00:00.10Z\");INSERT INTO job_stages VALUES(\"123e4567-e89b-12d3-a456-426614174000\",\"missing\",NULL,NULL);INSERT INTO job_stages VALUES(\"other-job\",\"cross-job\",\"2026-08-31T00:00:00Z\",\"2026-08-31T00:01:40Z\");INSERT INTO job_events VALUES(\"123e4567-e89b-12d3-a456-426614174000\",\"fetch\",\"2026-08-31T00:00:00Z\");INSERT INTO job_events VALUES(\"other-job\",\"cross-job\",\"2026-08-31T00:00:00Z\");")' 'open(os.path.dirname(sys.argv[1])+"/logs/axon.log","w").write(__import__("json").dumps({"timestamp":"2026-08-31T00:00:00Z","message":"web acquisition batch timing","lane":"concurrent","item_count":"1","concurrency":"1","wall_ms":"1","first_completion_ms":"1","item_p50_ms":"1","item_p95_ms":"1","item_max_ms":"1","max_completion_gap_ms":"0","slot_occupancy_permille":"1000"})+"\n")' 'PY' 'printf "{\"job_id\":\"123e4567-e89b-12d3-a456-426614174000\"}"' >"$root/bin/axon"
chmod +x "$root/bin/axon"

run_case() {
  rm -f "$metrics_count"; : >"$curl_log"; : >"$axon_log"
  FAKE_CURL_LOG="$curl_log" FAKE_METRICS_COUNT="$metrics_count" FAKE_AXON_LOG="$axon_log" \
  AXON_BENCH_LIBRARY_MODE=0 \
  AXON_BENCH_RETAIN_COLLECTION="${AXON_BENCH_RETAIN_COLLECTION:-0}" \
  AXON_BENCH_MODE=pinned-replay AXON_BENCH_REPLAY_FIXTURE="$root/replay" AXON_BENCH_AXON_BIN="$root/bin/axon" \
  AXON_BENCH_CURL_BIN="$root/bin/curl" AXON_BENCH_ENV_FILE="$root/env" AXON_BENCH_SKIP_STALE_CHECK=1 \
  AXON_BENCH_OUTPUT="$root/result.json" bash "$SCRIPT_DIR/bench-source-pipeline.sh"
}
run_case
jq -e '.benchmark_mode=="pinned-replay" and .collection_owned and (.provider_contract.model_id=="observed/fake") and (.throughput_configuration.sha256|length==64) and (.metal_busy_interval.seconds==0.1) and (.wall_minus_metal_busy_seconds>=0) and (.critical_path_seconds==0.1) and (.overlap_seconds==0.04) and (.unattributed_seconds==0.02) and (.unattributed_ratio==0.2) and (.attribution_ratio==0.8) and (.attribution_gate==false) and (.evidence_gate==false) and (.environment_comparable==false) and (.timing.stage_active|has("cross-job")|not) and (.timing.stage_active|has("missing")|not) and (.evidence_reasons|index("critical_path_attribution_below_95_percent")!=null)' "$root/result.json" >/dev/null || fail report-attribution-contract
! rg -q 'do-not-leak|http://secret' "$root/result.json" || fail config-secret
rg -q -- 'source .+ --scope site --cache false --wait true --json --quiet --collection axon_bench_' "$axon_log" || fail argv
[[ $(rg -c -- '--request DELETE .*/collections/axon_bench_' "$curl_log") == 1 ]] || fail success-cleanup

AXON_BENCH_RETAIN_COLLECTION=1 run_case
jq -e '.collection_retained==true' "$root/result.json" >/dev/null || fail retain-result-contract
! rg -q -- '--request DELETE .*/collections/axon_bench_' "$curl_log" || fail retain-opt-in

rm -f "$metrics_count"; : >"$curl_log"; set +e
AXON_BENCH_LIBRARY_MODE=0 FAKE_AXON_FAIL=1 FAKE_CURL_LOG="$curl_log" FAKE_METRICS_COUNT="$metrics_count" FAKE_AXON_LOG="$axon_log" AXON_BENCH_MODE=live-cold-crawl AXON_BENCH_SOURCE=https://example.invalid AXON_BENCH_AXON_BIN="$root/bin/axon" AXON_BENCH_CURL_BIN="$root/bin/curl" AXON_BENCH_ENV_FILE="$root/env" AXON_BENCH_SKIP_STALE_CHECK=1 bash "$SCRIPT_DIR/bench-source-pipeline.sh" 2>"$root/failure.log"; status=$?; set -e
[[ $status == 17 ]] || fail failure-status
[[ $(rg -c -- '--request DELETE .*/collections/axon_bench_' "$curl_log") == 1 ]] || fail failure-cleanup
! rg -q 'super-secret|secret.invalid' "$root/failure.log" || fail failure-redaction

rm -f "$metrics_count"; : >"$curl_log"
AXON_BENCH_LIBRARY_MODE=0 FAKE_AXON_SLEEP=30 FAKE_CURL_LOG="$curl_log" FAKE_METRICS_COUNT="$metrics_count" FAKE_AXON_LOG="$axon_log" AXON_BENCH_MODE=live-cold-crawl AXON_BENCH_SOURCE=https://example.invalid AXON_BENCH_AXON_BIN="$root/bin/axon" AXON_BENCH_CURL_BIN="$root/bin/curl" AXON_BENCH_ENV_FILE="$root/env" AXON_BENCH_SKIP_STALE_CHECK=1 bash "$SCRIPT_DIR/bench-source-pipeline.sh" & pid=$!
for _ in {1..100}; do rg -q -- '/collections/axon_bench_' "$curl_log" && break; sleep 0.02; done
kill -TERM "$pid"; set +e; wait "$pid"; status=$?; set -e
[[ $status == 143 ]] || fail signal-status
[[ $(rg -c -- '--request DELETE .*/collections/axon_bench_' "$curl_log") == 1 ]] || fail signal-cleanup

echo 'bench-source-pipeline tests passed'
