#!/bin/bash
set -euo pipefail
umask 077

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
BENCH_WORK_DIR= BENCH_COLLECTION= BENCH_COLLECTION_OWNED=0 BENCH_CLEANED=0 BENCH_CHILD_PID=

sanitize_text() { sed -E -e 's#https?://[^[:space:]"}]+#<redacted-url>#g' -e 's#([Aa]uthorization|[Bb]earer|[Tt]oken|[Kk]ey|[Ss]ecret|[Pp]assword)[^[:space:],}]*#<redacted-secret>#g' -e 's#[A-Za-z0-9_=-]{32,}#<redacted-value>#g'; }
validate_job_id() { [[ ${1-} =~ ^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$ ]]; }
validate_safe_source() { [[ ${1-} != *'@'* && ${1-} != *'$('* && ${1-} != *'`'* ]]; }
reject_stale_binary() {
  [[ ${AXON_BENCH_SKIP_STALE_CHECK:-0} == 1 ]] && return
  local newer; newer=$(find crates src build.rs Cargo.toml Cargo.lock -newer "$1" \( -name '*.rs' -o -name 'Cargo.toml' -o -name 'Cargo.lock' \) -print -quit 2>/dev/null)
  [[ -z $newer ]] || { echo "benchmark binary is older than $newer; rebuild with 'cargo build --release --bin axon'" >&2; return 1; }
}
http_json() { "${AXON_BENCH_CURL_BIN:-curl}" --connect-timeout 2 --max-time 10 --silent --show-error --fail --request "$1" --output "$3" "$2"; }
metrics_get() {
  [[ ${AXON_BENCH_MLX_URL:-http://127.0.0.1:8084} == http://127.0.0.1:* ]] || return 2
  if http_json GET "${AXON_BENCH_MLX_URL:-http://127.0.0.1:8084}/metrics" "$1" 2>/dev/null; then
    jq -e 'type=="object" and (.epoch|type=="string")' "$1" >/dev/null
    return
  fi
  local info="$1.info"
  http_json GET "${AXON_BENCH_MLX_URL:-http://127.0.0.1:8084}/info" "$info"
  jq -S '{available:false,epoch:(.model_id//.model//.model_name//"provider-info"),reason:"provider_metrics_endpoint_unavailable"}' "$info" >"$1"
}
delete_owned_collection() {
  [[ $BENCH_COLLECTION_OWNED == 1 && ${AXON_BENCH_RETAIN_COLLECTION:-0} != 1 ]] || return 0
  http_json DELETE "${AXON_BENCH_QDRANT_URL:-http://127.0.0.1:53333}/collections/$BENCH_COLLECTION" "$BENCH_WORK_DIR/delete.json" || { echo "failed to delete owned benchmark collection $BENCH_COLLECTION" >&2; return 1; }
  BENCH_COLLECTION_OWNED=0
}
cleanup_benchmark() {
  local status=${1:-$?} cleanup_status=0
  [[ $BENCH_CLEANED == 0 ]] || return "$status"; BENCH_CLEANED=1
  delete_owned_collection || cleanup_status=$?
  if [[ ${AXON_BENCH_RETAIN_WORK_DIR:-0} == 1 ]]; then
    [[ -z ${BENCH_WORK_DIR:-} ]] || echo "retained benchmark work directory: $BENCH_WORK_DIR" >&2
  else
    [[ -z ${BENCH_WORK_DIR:-} || ! -d $BENCH_WORK_DIR ]] || rm -rf -- "$BENCH_WORK_DIR"
  fi
  [[ $status -ne 0 ]] && return "$status"; return "$cleanup_status"
}
on_exit() { local s=$?; trap - EXIT HUP INT TERM; cleanup_benchmark "$s"; exit $?; }
on_signal() { local sig=$1 code=$2; trap - EXIT HUP INT TERM; [[ -z ${BENCH_CHILD_PID:-} ]] || kill -TERM "$BENCH_CHILD_PID" 2>/dev/null || true; cleanup_benchmark "$code" || true; trap - "$sig"; kill -s "$sig" "$$"; }

corpus_hash_from_sqlite() { python3 - "$1" "$2" <<'PY'
import hashlib,json,sqlite3,sys
with sqlite3.connect(sys.argv[1]) as c:
 r=c.execute("SELECT i.source_item_key,i.content_hash FROM source_items i JOIN sources s ON s.source_id=i.source_id AND s.committed_generation=i.generation WHERE i.source_id=(SELECT source_id FROM jobs WHERE job_id=?) ORDER BY i.source_item_key",(sys.argv[2],)).fetchall()
if not r: raise SystemExit("completed benchmark has no committed corpus rows")
print(hashlib.sha256(json.dumps(r,separators=(",",":")).encode()).hexdigest())
PY
}
acquisition_timings_from_log() {
  [[ -f $1 ]] || { echo 'benchmark acquisition telemetry log is missing' >&2; return 1; }
  jq -e -s '[.[]|select(.message=="web acquisition batch timing")|{timestamp,lane,item_count:(.item_count|tonumber),concurrency:(.concurrency|tonumber),wall_ms:(.wall_ms|tonumber),first_completion_ms:(.first_completion_ms|tonumber),item_p50_ms:(.item_p50_ms|tonumber),item_p95_ms:(.item_p95_ms|tonumber),item_max_ms:(.item_max_ms|tonumber),max_completion_gap_ms:(.max_completion_gap_ms|tonumber),slot_occupancy:((.slot_occupancy_permille|tonumber)/1000)}]|if length==0 then error("no acquisition timings") else . end' "$1"
}
capture_contracts() {
  local axon=$1 env=$2 state=$3 provider=$4 config=$5 environment=$6 raw=$7
  http_json GET "${AXON_BENCH_MLX_URL:-http://127.0.0.1:8084}/info" "$provider"
  jq -e 'type=="object" and ((.model_id//.model//.model_name)|type=="string")' "$provider" >/dev/null
  AXON_ENV_FILE="$env" AXON_DATA_DIR="$state" AXON_SQLITE_PATH="$state/jobs.db" "$axon" config list --json >"$raw"
  python3 - "$raw" "$config" "$provider" "$environment" <<'PY'
import hashlib,json,os,platform,re,subprocess,sys
raw,out,pp,ep=sys.argv[1:]; data=json.load(open(raw)); secret=re.compile('token|key|secret|password|credential|authorization|cookie',re.I); endpoint=re.compile('url|endpoint|host',re.I)
def clean(v,k=''):
 if secret.search(k): return '<redacted-secret>'
 if endpoint.search(k) and isinstance(v,str): return '<redacted-endpoint>'
 if isinstance(v,dict): return {x:clean(y,x) for x,y in sorted(v.items())}
 if isinstance(v,list): return [clean(x,k) for x in v]
 return v
r=clean(data); canon=json.dumps(r,sort_keys=True,separators=(',',':')); json.dump({'provenance':{'command':'axon config list --json','env_file':'managed','precedence':'cli > env > config.toml > defaults'},'resolved':r,'sha256':hashlib.sha256(canon.encode()).hexdigest()},open(out,'w'),sort_keys=True)
p=json.load(open(pp)); snap={'machine':platform.machine(),'os':platform.platform(),'cpu_count':os.cpu_count(),'load_average':list(os.getloadavg()),'provider_load':p.get('load') or p.get('queue'),'provider_identity_sha256':hashlib.sha256(json.dumps(p,sort_keys=True).encode()).hexdigest()}; snap['fingerprint_sha256']=hashlib.sha256(json.dumps({k:snap[k] for k in ('machine','os','cpu_count','provider_identity_sha256')},sort_keys=True).encode()).hexdigest(); json.dump(snap,open(ep,'w'),sort_keys=True)
PY
}

run_benchmark() {
  local mode=${AXON_BENCH_MODE:-} source=${AXON_BENCH_SOURCE:-} replay=${AXON_BENCH_REPLAY_FIXTURE:-} axon=${AXON_BENCH_AXON_BIN:-target/release/axon} output=${AXON_BENCH_OUTPUT:-/dev/stdout}
  [[ $mode == pinned-replay || $mode == live-cold-crawl ]] || { echo 'AXON_BENCH_MODE must be pinned-replay or live-cold-crawl' >&2; return 2; }
  if [[ $mode == pinned-replay ]]; then [[ -n $replay && -r $replay ]] || { echo 'pinned-replay requires AXON_BENCH_REPLAY_FIXTURE' >&2; return 2; }; source=$replay; else [[ -n $source ]] || { echo 'live-cold-crawl requires AXON_BENCH_SOURCE' >&2; return 2; }; fi
  validate_safe_source "$source" && [[ -x $axon ]] || { echo 'invalid source or binary' >&2; return 2; }; reject_stale_binary "$axon" || return 2
  BENCH_WORK_DIR=$(mktemp -d "${TMPDIR:-/tmp}/axon-source-bench.XXXXXX"); trap on_exit EXIT; trap 'on_signal HUP 129' HUP; trap 'on_signal INT 130' INT; trap 'on_signal TERM 143' TERM
  local state=$BENCH_WORK_DIR/state env probe=$BENCH_WORK_DIR/probe.json; mkdir -m 700 "$state"; env=${AXON_BENCH_ENV_FILE:-${HOME:?HOME required}/.axon/.env}; [[ -r $env ]] || { echo 'managed Axon environment unreadable' >&2; return 2; }
  if [[ -n ${AXON_BENCH_COLLECTION:-} ]]; then [[ ${AXON_BENCH_OWN_COLLECTION:-0} == 1 ]] || { echo 'explicit collection requires AXON_BENCH_OWN_COLLECTION=1' >&2; return 2; }; BENCH_COLLECTION=$AXON_BENCH_COLLECTION; else BENCH_COLLECTION="axon_bench_$(date -u +%Y%m%dT%H%M%SZ)_$$_${RANDOM}"; fi
  [[ $BENCH_COLLECTION =~ ^axon_bench_[A-Za-z0-9_]+$ ]] || { echo 'owned collection must use axon_bench_ prefix' >&2; return 2; }
  if http_json GET "${AXON_BENCH_QDRANT_URL:-http://127.0.0.1:53333}/collections/$BENCH_COLLECTION" "$probe" 2>/dev/null; then echo 'owned collection already exists' >&2; return 2; fi; BENCH_COLLECTION_OWNED=1
  local before=$BENCH_WORK_DIR/before.json after=$BENCH_WORK_DIR/after.json provider=$BENCH_WORK_DIR/provider.json config=$BENCH_WORK_DIR/config.json environment=$BENCH_WORK_DIR/environment.json raw=$BENCH_WORK_DIR/config-raw.json stdout=$BENCH_WORK_DIR/stdout.json stderr=$BENCH_WORK_DIR/stderr.log acquisition=$BENCH_WORK_DIR/acquisition.json
  capture_contracts "$axon" "$env" "$state" "$provider" "$config" "$environment" "$raw"; metrics_get "$before"
  local start end status child job hash; start=$(python3 -c 'import time;print(time.time_ns())'); set +e
  AXON_ENV_FILE="$env" AXON_DATA_DIR="$state" AXON_SQLITE_PATH="$state/jobs.db" "$axon" source "$source" --scope site --cache false --wait true --json --quiet --collection "$BENCH_COLLECTION" >"$stdout" 2>"$stderr" & child=$!; BENCH_CHILD_PID=$child; wait "$child"; status=$?; BENCH_CHILD_PID=; set -e; end=$(python3 -c 'import time;print(time.time_ns())'); metrics_get "$after"
  [[ $status == 0 ]] || { sanitize_text <"$stderr" >&2; return "$status"; }; job=$(jq -r '.job_id//.data.job_id//.id//empty' "$stdout"); validate_job_id "$job" || return 3; hash=$(corpus_hash_from_sqlite "$state/jobs.db" "$job"); acquisition_timings_from_log "$state/logs/axon.log" >"$acquisition"
  AXON_BENCH_WORK_DIR="$BENCH_WORK_DIR" PYTHONPATH="$SCRIPT_DIR" python3 "$SCRIPT_DIR/bench_source_pipeline_report.py" "$before" "$after" "$provider" "$config" "$environment" "$job" "$hash" "$start" "$end" "$state/jobs.db" "$acquisition" "$mode" >"$output"
}
[[ ${AXON_BENCH_LIBRARY_MODE:-0} == 1 ]] || run_benchmark
