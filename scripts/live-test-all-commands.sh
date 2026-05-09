#!/usr/bin/env bash
set -u

# Full Axon CLI live test harness
# - Executes all implemented commands/subcommands
# - Uses default behavior and only required positional/required flags
# - Excludes only Qdrant-deleting command: dedupe

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$ROOT_DIR" || exit 1

TS="$(date +%Y%m%d-%H%M%S)"
OUTDIR="${AXON_LIVE_TEST_OUTDIR:-$ROOT_DIR/.cache/live-test/$TS}"
mkdir -p "$OUTDIR/logs"
REPORT="$OUTDIR/report.tsv"
SUMMARY="$OUTDIR/summary.txt"

printf "id\tcommand\tinvocation\texit\tresult\tembed_expected\tembed_verified\tqdrant_before\tqdrant_after\tjob_family\tjob_id\tnotes\tlog\n" > "$REPORT"

# shellcheck source=scripts/lib/axon-env.sh
source "$ROOT_DIR/scripts/lib/axon-env.sh"
ENV_FILE="$(resolve_axon_env_file "$ROOT_DIR")"

QURL="$(grep -E '^QDRANT_URL=' "$ENV_FILE" 2>/dev/null | tail -n1 | cut -d= -f2-)"
COL="$(grep -E '^AXON_COLLECTION=' "$ENV_FILE" 2>/dev/null | tail -n1 | cut -d= -f2-)"
[ -z "$COL" ] && COL="cortex"
if echo "$QURL" | grep -q 'axon-qdrant'; then
  QURL="http://127.0.0.1:53333"
fi
[ -z "$QURL" ] && QURL="http://127.0.0.1:53333"

BASE_URL="${AXON_TEST_BASE_URL:-https://neverssl.com}"
BATCH_URL_1="${AXON_TEST_BATCH_URL_1:-https://neverssl.com}"
BATCH_URL_2="${AXON_TEST_BATCH_URL_2:-https://www.rust-lang.org/learn}"
BATCH_URL_3="${AXON_TEST_BATCH_URL_3:-https://www.rust-lang.org/tools/install}"
SEARCH_TEXT="${AXON_TEST_SEARCH_TEXT:-rust programming language}"
QUERY_TEXT="${AXON_TEST_QUERY_TEXT:-rust ownership model}"
ASK_TEXT="${AXON_TEST_ASK_TEXT:-How do lifetimes work in Rust?}"
EXTRACT_PROMPT="${AXON_TEST_EXTRACT_PROMPT:-Extract the key topics and summarize the page in bullet points}"
YOUTUBE_URL="${AXON_TEST_YOUTUBE_URL:-https://www.youtube.com/watch?v=dQw4w9WgXcQ}"
GITHUB_REPO="${AXON_TEST_GITHUB_REPO:-rust-lang/rust}"
REDDIT_TARGET="${AXON_TEST_REDDIT_TARGET:-rust}"

qcount() {
  local count
  count=$(curl -sS "$QURL/collections/$COL/points/count" \
    -H 'content-type: application/json' \
    -d '{"exact":true}' 2>/dev/null \
    | sed -n 's/.*"count"[[:space:]]*:[[:space:]]*\([0-9][0-9]*\).*/\1/p' \
    | head -n1)
  [ -z "$count" ] && count="NA"
  echo "$count"
}

extract_job_id() {
  local logfile="$1"
  grep -Eo '[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}' "$logfile" | tail -n1
}

record() {
  printf "%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n" \
    "$1" "$2" "$3" "$4" "$5" "$6" "$7" "$8" "$9" "${10}" "${11}" "${12}" "${13}" >> "$REPORT"
}

run_case() {
  local id="$1" cmdname="$2" embed_expected="$3" family="$4"
  shift 4
  local invocation="$*"
  local logfile="$OUTDIR/logs/${id}.log"
  local qb qa exitc result embed_verified jid notes

  qb=$(qcount)
  bash -lc "$*" >"$logfile" 2>&1
  exitc=$?
  qa=$(qcount)

  [ "$exitc" -eq 0 ] && result="PASS" || result="FAIL"

  embed_verified="N/A"
  if [ "$embed_expected" = "yes" ]; then
    if [ "$qb" != "NA" ] && [ "$qa" != "NA" ] && [ "$qa" -gt "$qb" ] 2>/dev/null; then
      embed_verified="true"
    else
      embed_verified="false"
    fi
  fi

  jid=""
  notes=""
  if [ "$family" != "-" ]; then
    jid=$(extract_job_id "$logfile")
    [ -z "$jid" ] && notes="no-job-id-found"
  fi

  record "$id" "$cmdname" "$invocation" "$exitc" "$result" "$embed_expected" "$embed_verified" "$qb" "$qa" "$family" "$jid" "$notes" "$logfile"
  echo "$id|$result|$jid"
}

wait_for_job_terminal() {
  local id="$1" family="$2" jid="$3" timeout_secs="${4:-180}"
  local logfile="$OUTDIR/logs/${id}.log"
  local start now status_out

  start=$(date +%s)
  while true; do
    ./scripts/axon "$family" status "$jid" >"$logfile" 2>&1
    status_out=$(tr '\n' ' ' < "$logfile")

    if echo "$status_out" | grep -Eiq 'completed|failed|canceled'; then
      break
    fi

    now=$(date +%s)
    if [ $((now - start)) -ge "$timeout_secs" ]; then
      echo "timeout waiting job $family $jid after ${timeout_secs}s" >> "$logfile"
      break
    fi
    sleep 5
  done
}

run_worker_case() {
  local id="$1" family="$2"
  local logfile="$OUTDIR/logs/${id}.log"
  local qb qa exitc result notes

  qb=$(qcount)
  timeout 20s ./scripts/axon "$family" worker >"$logfile" 2>&1
  exitc=$?
  qa=$(qcount)

  if [ "$exitc" -eq 124 ]; then
    result="PASS"
    notes="worker-daemon-timeout-expected"
  elif [ "$exitc" -eq 0 ]; then
    result="PASS"
    notes="worker-exited-cleanly"
  else
    result="FAIL"
    notes="worker-exit-$exitc"
  fi

  record "$id" "$family worker" "timeout 20s ./scripts/axon $family worker" "$exitc" "$result" "no" "N/A" "$qb" "$qa" "$family" "" "$notes" "$logfile"
}

run_family_set() {
  local fam="$1" seed_cmd="$2"
  local jid nil
  nil="00000000-0000-0000-0000-000000000000"

  run_case "${fam}_S1" "$fam list" no "$fam" "./scripts/axon $fam list" >/dev/null
  run_case "${fam}_S2" "$fam recover" no "$fam" "./scripts/axon $fam recover" >/dev/null
  run_case "${fam}_S3" "$fam cleanup" no "$fam" "./scripts/axon $fam cleanup" >/dev/null

  jid=$(run_case "${fam}_S4" "$fam enqueue" no "$fam" "$seed_cmd" | awk -F'|' '{print $3}')
  [ -z "$jid" ] && jid="$nil"

  run_case "${fam}_S5" "$fam status" no "$fam" "./scripts/axon $fam status $jid" >/dev/null
  run_case "${fam}_S6" "$fam cancel" no "$fam" "./scripts/axon $fam cancel $jid" >/dev/null
  run_case "${fam}_S7" "$fam status-post-cancel" no "$fam" "./scripts/axon $fam status $jid" >/dev/null
  run_case "${fam}_S8" "$fam errors" no "$fam" "./scripts/axon $fam errors $jid" >/dev/null
  run_case "${fam}_S9" "$fam doctor" no "$fam" "./scripts/axon $fam doctor" >/dev/null

  local clear_log qb qa exitc result
  clear_log="$OUTDIR/logs/${fam}_S10.log"
  qb=$(qcount)
  printf 'y\n' | ./scripts/axon "$fam" clear >"$clear_log" 2>&1
  exitc=$?
  qa=$(qcount)
  [ "$exitc" -eq 0 ] && result="PASS" || result="FAIL"
  record "${fam}_S10" "$fam clear" "printf 'y\\n' | ./scripts/axon $fam clear" "$exitc" "$result" "no" "N/A" "$qb" "$qa" "$fam" "" "" "$clear_log"

  run_worker_case "${fam}_S11" "$fam"
}

echo "Run started: $TS" | tee "$SUMMARY"
echo "Qdrant: $QURL collection=$COL" | tee -a "$SUMMARY"
echo "Baseline count: $(qcount)" | tee -a "$SUMMARY"

# Top-level commands
C1=$(run_case T01 doctor no - "./scripts/axon doctor")
C2=$(run_case T02 status no - "./scripts/axon status")
C3=$(run_case T03 scrape yes - "./scripts/axon scrape $BASE_URL")
C4=$(run_case T04 map no - "./scripts/axon map $BASE_URL")
C5=$(run_case T05 crawl yes crawl "./scripts/axon crawl $BASE_URL")
C6=$(run_case T06 batch yes batch "./scripts/axon batch $BATCH_URL_1 $BATCH_URL_2 $BATCH_URL_3")
C7=$(run_case T07 extract no extract "./scripts/axon extract $BASE_URL --query '$EXTRACT_PROMPT'")
C8=$(run_case T08 search no - "./scripts/axon search $SEARCH_TEXT")
C9=$(run_case T09 embed yes embed "./scripts/axon embed $BASE_URL")
C10=$(run_case T10 query no - "./scripts/axon query $QUERY_TEXT")
C11=$(run_case T11 retrieve no - "./scripts/axon retrieve $BASE_URL")
C12=$(run_case T12 ask no - "./scripts/axon ask '$ASK_TEXT'")
C13=$(run_case T13 evaluate no - "./scripts/axon evaluate '$ASK_TEXT'")
C14=$(run_case T14 suggest no - "./scripts/axon suggest")
C15=$(run_case T15 sources no - "./scripts/axon sources")
C16=$(run_case T16 domains no - "./scripts/axon domains")
C17=$(run_case T17 stats no - "./scripts/axon stats")
C18=$(run_case T18 debug no - "./scripts/axon debug")
C19=$(run_case T19 github yes github "./scripts/axon github $GITHUB_REPO")
C20=$(run_case T20 reddit yes reddit "./scripts/axon reddit $REDDIT_TARGET")
C21=$(run_case T21 youtube yes youtube "./scripts/axon youtube $YOUTUBE_URL")

J_CRAWL=$(echo "$C5" | awk -F'|' '{print $3}')
J_BATCH=$(echo "$C6" | awk -F'|' '{print $3}')
J_EXTRACT=$(echo "$C7" | awk -F'|' '{print $3}')
J_EMBED=$(echo "$C9" | awk -F'|' '{print $3}')
J_GITHUB=$(echo "$C19" | awk -F'|' '{print $3}')
J_REDDIT=$(echo "$C20" | awk -F'|' '{print $3}')
J_YOUTUBE=$(echo "$C21" | awk -F'|' '{print $3}')

# Async waits (ingest families get shorter cap because ingest workers may not be running)
[ -n "$J_CRAWL" ] && wait_for_job_terminal AJ01 crawl "$J_CRAWL" 180
[ -n "$J_BATCH" ] && wait_for_job_terminal AJ02 batch "$J_BATCH" 180
[ -n "$J_EXTRACT" ] && wait_for_job_terminal AJ03 extract "$J_EXTRACT" 180
[ -n "$J_EMBED" ] && wait_for_job_terminal AJ04 embed "$J_EMBED" 180
[ -n "$J_GITHUB" ] && wait_for_job_terminal AJ05 github "$J_GITHUB" 45
[ -n "$J_REDDIT" ] && wait_for_job_terminal AJ06 reddit "$J_REDDIT" 45
[ -n "$J_YOUTUBE" ] && wait_for_job_terminal AJ07 youtube "$J_YOUTUBE" 45

# Subcommand families
run_family_set crawl "./scripts/axon crawl $BASE_URL"
run_family_set batch "./scripts/axon batch $BATCH_URL_1 $BATCH_URL_2"
run_family_set extract "./scripts/axon extract $BASE_URL --query '$EXTRACT_PROMPT'"
run_family_set embed "./scripts/axon embed $BASE_URL"
run_family_set github "./scripts/axon github $GITHUB_REPO"
run_family_set reddit "./scripts/axon reddit $REDDIT_TARGET"
run_family_set youtube "./scripts/axon youtube $YOUTUBE_URL"

# Explicit exclusion per request
printf "EXCLUDED\tdedupe\t./scripts/axon dedupe\tNA\tSKIP\tno\tN/A\tNA\tNA\t-\t-\tExcluded per request: deletes Qdrant data\t-\n" >> "$REPORT"

echo "Final count: $(qcount)" | tee -a "$SUMMARY"
echo "Report: $REPORT" | tee -a "$SUMMARY"
echo "Summary: $SUMMARY" | tee -a "$SUMMARY"

echo "Done."
