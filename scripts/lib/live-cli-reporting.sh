#!/usr/bin/env bash
# Reporting and semantic coverage helpers.

cleanup_live_fixtures() {
  local collection _attempt member member_pgid chrome_group_killed
  cleanup_warning() {
    printf 'warning: failed to clean up %s; inspect %s\n' "$1" "$2" >&2
  }
  chrome_group_has_session_token() {
    [ -n "${live_chrome_pgid:-}" ] && [ -n "${live_chrome_session_token:-}" ] || return 1
    # Fast pid-level path: the spawned leader itself still carries the token.
    # Robust against transient `ps` failures and process churn from
    # concurrently running harness instances.
    if tr '\0' '\n' <"/proc/$live_chrome_pid/environ" 2>/dev/null \
      | grep -Fqx "AXON_LIVE_CHROME_SESSION_TOKEN=$live_chrome_session_token"; then
      return 0
    fi
    while read -r member member_pgid; do
      [ "$member_pgid" = "$live_chrome_pgid" ] || continue
      [ -r "/proc/$member/environ" ] || continue
      if tr '\0' '\n' <"/proc/$member/environ" 2>/dev/null \
        | grep -Fqx "AXON_LIVE_CHROME_SESSION_TOKEN=$live_chrome_session_token"; then
        return 0
      fi
    done < <(ps -eo pid=,pgid= 2>/dev/null)
    return 1
  }
  # A single probe can fail transiently (fork pressure, churned /proc entries)
  # while the group is still verifiably ours; a false refusal here leaks a
  # deliberately TERM-immune Chrome group. Retry before concluding anything.
  chrome_group_ownership_confirmed() {
    local _ownership_attempt
    for _ownership_attempt in 1 2 3; do
      if chrome_group_has_session_token; then
        return 0
      fi
      kill -0 -- "-$live_chrome_pgid" 2>/dev/null || return 1
      sleep 0.1
    done
    return 1
  }
  chrome_pid_identity_intact() {
    local _identity_attempt observed_start_time
    [ -n "${live_chrome_start_time:-}" ] || return 1
    for _identity_attempt in 1 2 3; do
      observed_start_time="$(awk '{print $22}' "/proc/$live_chrome_pid/stat" 2>/dev/null)"
      if [ -n "$observed_start_time" ]; then
        [ "$observed_start_time" = "$live_chrome_start_time" ]
        return
      fi
      kill -0 "$live_chrome_pid" 2>/dev/null || return 1
      sleep 0.1
    done
    return 1
  }
  if [ -n "${live_chrome_pid:-}" ] \
    && [ -n "${live_chrome_pgid:-}" ] \
    && chrome_pid_identity_intact \
    && chrome_group_ownership_confirmed; then
    chrome_group_killed=1
    kill -TERM -- "-$live_chrome_pgid" 2>/dev/null || true
    for _attempt in $(seq 1 20); do
      kill -0 -- "-$live_chrome_pgid" 2>/dev/null || break
      sleep 0.1
    done
    if kill -0 -- "-$live_chrome_pgid" 2>/dev/null; then
      if chrome_group_ownership_confirmed; then
        kill -KILL -- "-$live_chrome_pgid" 2>/dev/null || true
      else
        chrome_group_killed=0
        cleanup_warning "Chrome process group (ownership identity changed before KILL)" \
          "$OUTDIR/logs/chrome.stderr.log"
      fi
    fi
    # Reap the direct child only when the group was actually signalled dead.
    # An unconditional `wait` here parked the harness forever whenever the
    # KILL was refused: the fixture Chrome ignores TERM by design, so the
    # wait had nothing left that could ever end it.
    if [ "$chrome_group_killed" -eq 1 ]; then
      wait "$live_chrome_pid" 2>/dev/null || true
    fi
  elif [ -n "${live_chrome_pid:-}" ]; then
    cleanup_warning "Chrome process group (ownership identity unavailable)" \
      "$OUTDIR/logs/chrome.stderr.log"
  fi
  if [[ "$isolated_compose_project" == axon-live-* ]] \
    && [[ "$isolated_compose_network" == axon-live-* ]] \
    && [ -f "${SETUP_HOME:-}/.axon/.env" ] \
    && [ -f "${SETUP_HOME:-}/.axon/compose/docker-compose.yaml" ]; then
    timeout "${AXON_LIVE_CLEANUP_TIMEOUT_SECS:-30}s" docker compose --env-file "$SETUP_HOME/.axon/.env" \
      -f "$SETUP_HOME/.axon/compose/docker-compose.yaml" \
      -f "$SETUP_HOME/.axon/compose/docker-compose.external-qdrant.yaml" \
      -f "$SETUP_HOME/.axon/compose/docker-compose.external-providers.yaml" \
      down --remove-orphans \
      >"$OUTDIR/logs/cleanup-compose.log" 2>"$OUTDIR/logs/cleanup-compose.stderr.log" \
      || cleanup_warning "compose stack" "$OUTDIR/logs/cleanup-compose.stderr.log"
    timeout "${AXON_LIVE_CLEANUP_TIMEOUT_SECS:-30}s" docker network rm "$isolated_compose_network" \
      >"$OUTDIR/logs/cleanup-network.log" 2>"$OUTDIR/logs/cleanup-network.stderr.log" \
      || cleanup_warning "Docker network $isolated_compose_network" "$OUTDIR/logs/cleanup-network.stderr.log"
  fi
  for collection in "${isolated_collections[@]}"; do
    if [[ "$collection" == axon_live_* ]] && [ -n "${QDRANT_URL:-}" ]; then
      curl -fsS -X DELETE \
        --connect-timeout 2 --max-time 10 --retry 2 --retry-connrefused \
        "${QDRANT_URL%/}/collections/$collection" \
        >>"$OUTDIR/logs/cleanup-qdrant.json" 2>>"$OUTDIR/logs/cleanup-qdrant.stderr.log" \
        || cleanup_warning "Qdrant collection $collection" "$OUTDIR/logs/cleanup-qdrant.stderr.log"
    fi
  done
  if [ -n "${PORT_LEASE_DIR:-}" ] && [ -d "$PORT_LEASE_DIR" ]; then
    rmdir "$PORT_LEASE_DIR" 2>/dev/null \
      || cleanup_warning "port lease $PORT_LEASE_DIR" "$OUTDIR/logs"
  fi
}
trap cleanup_live_fixtures EXIT

worktree_content_fingerprint() {
  (
    cd "$ROOT_DIR" || exit 1
    local -a pathspec=("$@")

    # Keep symlink targets distinct from regular-file content. NUL delimiters
    # preserve unusual but valid path and target bytes without ambiguity.
    printf 'links\0'
    while IFS= read -r -d '' path; do
      [ -L "$path" ] || continue
      printf '%s\0%s\0' "$path" "$(readlink -- "$path")"
    done < <(git ls-files -co --exclude-standard -z -- "${pathspec[@]}" | LC_ALL=C sort -z)

    # Hash regular files in batches. The previous implementation launched one
    # sha256sum process per path, which made the 4,000+ file worktree
    # fingerprint exceed nextest's 120-second timeout under load.
    printf 'files\0'
    while IFS= read -r -d '' path; do
      if [ -f "$path" ] && [ ! -L "$path" ]; then
        printf '%s\0' "$path"
      fi
    done < <(git ls-files -co --exclude-standard -z -- "${pathspec[@]}" | LC_ALL=C sort -z) \
      | xargs -0 -r sha256sum -z --
  ) | sha256sum | awk '{print $1}'
}

record() {
  local first=1 field line=""
  for field in "$@"; do
    field="${field//$'\t'/ }"
    field="${field//$'\r'/ }"
    field="${field//$'\n'/ }"
    if [ "$first" -eq 1 ]; then
      first=0
    else
      line+=$'\t'
    fi
    line+="$field"
  done
  {
    flock 9
    printf '%s\n' "$line" >&9
  } 9>>"$REPORT"
}

record_timing() {
  local started_ms="$1" phase="$2" name="$3" invocation="$4"
  local elapsed_ms field line=""
  elapsed_ms=$(( $(now_millis) - started_ms ))
  for field in "$elapsed_ms" "$phase" "$name" "$invocation"; do
    field="${field//$'\t'/ }"
    field="${field//$'\r'/ }"
    field="${field//$'\n'/ }"
    [ -n "$line" ] && line+=$'\t'
    line+="$field"
  done
  {
    flock 9
    printf '%s\n' "$line" >&9
  } 9>>"$TIMINGS_REPORT"
}

now_millis() {
  local epoch_ns
  epoch_ns="$(date +%s%N)"
  printf '%s\n' "$((epoch_ns / 1000000))"
}

ensure_behavior_global_options() {
  [ -s "$BEHAVIOR_GLOBAL_OPTIONS" ] && return
  : >"$BEHAVIOR_GLOBAL_VALUE_OPTIONS"
  timeout "${TIMEOUT_SECS}s" "$AXON_BIN" --help >"$OUTDIR/logs/behavior-global-help.log" 2>&1 || true
  awk -v values="$BEHAVIOR_GLOBAL_VALUE_OPTIONS" '
    /^  Global Options$/ { in_options=1; next }
    in_options && /^  Commands$/ { exit }
    in_options && /^  (-[A-Za-z], )?--[a-z0-9]/ {
      line=$0
      option=line
      sub(/^  (-[A-Za-z], )?/, "", option)
      sub(/[[:space:]].*/, "", option)
      print option
      if (line ~ /<[^>]+>/) print option >> values
    }
  ' "$OUTDIR/logs/behavior-global-help.log" >"$BEHAVIOR_GLOBAL_OPTIONS"
  awk '
    /^  -[A-Za-z], --[a-z0-9]/ { print substr($1, 1, 2) }
  ' "$OUTDIR/logs/behavior-global-help.log" >>"$BEHAVIOR_GLOBAL_OPTIONS"

  local option
  for option in     --automation-script --batch-concurrency --block-assets --budget --cache     --cache-http-only --chrome-screenshot --chrome-wait-for-selector --color     --cron-every-seconds --cron-max-runs --etag-conditional --exclude-path     --exclude-path-prefix --exclude-selector --format --normalize --output-dir     --performance-profile --quiet --root-selector --screenshot-full-page     --sitemap-only --url-glob --urls --viewport --warc --yes; do
    echo "$option" >>"$BEHAVIOR_GLOBAL_OPTIONS"
  done
  for option in     --automation-script --batch-concurrency --block-assets --budget --cache     --chrome-screenshot --chrome-wait-for-selector --color     --cron-every-seconds --cron-max-runs --exclude-path --exclude-path-prefix     --exclude-selector --format --normalize --output-dir --performance-profile     --root-selector --screenshot-full-page --url-glob --urls --viewport --warc; do
    echo "$option" >>"$BEHAVIOR_GLOBAL_VALUE_OPTIONS"
  done
  sort -u -o "$BEHAVIOR_GLOBAL_OPTIONS" "$BEHAVIOR_GLOBAL_OPTIONS"
  sort -u -o "$BEHAVIOR_GLOBAL_VALUE_OPTIONS" "$BEHAVIOR_GLOBAL_VALUE_OPTIONS"
}

record_behavior_args() {
  local name="$1" arg option before_command=1 consume_value=0
  shift
  ensure_behavior_global_options
  printf '%s	%s
' "$name" "__command__" >>"$BEHAVIOR_ACTUAL"
  for arg in "$@"; do
    if [ "$consume_value" -eq 1 ]; then
      consume_value=0
      continue
    fi
    if [ "$before_command" -eq 1 ]; then
      case "$arg" in
        --*)
          option="${arg%%=*}"
          if grep -Fqx -- "$option" "$BEHAVIOR_GLOBAL_OPTIONS"; then
            printf '%s	%s
' "@global" "$option" >>"$BEHAVIOR_ACTUAL"
            if [[ "$arg" != *=* ]]               && grep -Fqx -- "$option" "$BEHAVIOR_GLOBAL_VALUE_OPTIONS"; then
              consume_value=1
            fi
            continue
          fi
          ;;
        -[A-Za-z])
          if grep -Fqx -- "$arg" "$BEHAVIOR_GLOBAL_OPTIONS"; then
            printf '%s	%s
' "@global" "$arg" >>"$BEHAVIOR_ACTUAL"
            continue
          fi
          ;;
        *)
          before_command=0
          ;;
      esac
    fi
    [ "$before_command" -eq 1 ] && continue
    case "$arg" in
      --*)
        option="${arg%%=*}"
        if ! grep -Fqx -- "$option" "$BEHAVIOR_GLOBAL_OPTIONS"; then
          printf '%s	%s
' "$name" "$option" >>"$BEHAVIOR_ACTUAL"
        fi
        ;;
      -[A-Za-z])
        if ! grep -Fqx -- "$arg" "$BEHAVIOR_GLOBAL_OPTIONS"; then
          printf '%s	%s
' "$name" "$arg" >>"$BEHAVIOR_ACTUAL"
        fi
        ;;
    esac
  done
}

set_pending_behavior() {
  LAST_BEHAVIOR_NAME="$1"
  shift
  LAST_BEHAVIOR_ARGS=("$@")
}

confirm_pending_behavior() {
  if [ -n "$LAST_BEHAVIOR_NAME" ]; then
    record_behavior_args "$LAST_BEHAVIOR_NAME" "${LAST_BEHAVIOR_ARGS[@]}"
    LAST_BEHAVIOR_NAME=""
    LAST_BEHAVIOR_ARGS=()
  fi
}

prove_option_behavior() {
  local name="$1" option="$2" evidence="$3" key
  key="$(printf '%s	%s' "$name" "$option")"
  if grep -Fqx -- "$key" "$BEHAVIOR_ACTUAL" && [ -n "$evidence" ]; then
    printf '%s	%s	%s
' "$name" "$option" "$evidence" >>"$BEHAVIOR_SEMANTIC"
  fi
}

prove_options() {
  local name="$1" evidence="$2" option
  shift 2
  for option in "$@"; do
    prove_option_behavior "$name" "$option" "$evidence"
  done
}

prove_mapped_command_options() {
  local name="$1"
  case "$name" in
    "watch create"|"watch update") prove_options "$name" "watch schedule persisted and was read back by lifecycle commands" --every-seconds ;;
    "map") prove_options "@global" \
      "map returned bounded sitemap results with the requested path exclusions" \
      --max-pages --sitemap-only --exclude-path-prefix ;;
    "extract") prove_options "@global" "detached extraction returned a durable job descriptor" --wait ;;
    "search") prove_options "@global" "search returned a bounded, time-filtered result collection" --limit --search-time-range ;;
    "research") prove_options "@global" "research returned synthesis and source results at requested depth" --research-depth ;;
    "scrape")
      prove_options "$name" "one-page projection returned inline content while explicitly skipping embeddings" --inline --no-embed
      prove_options "@global" "one-page source honored wait, collection, and isolated output directory" --wait --collection --output-dir
      ;;
    "query")
      prove_options "$name" "query emitted retrieval diagnostics for the bounded result" --diagnostics
      prove_options "@global" \
        "query used explicit text, provider URLs, temporal bounds, dense-only mode, and script-safe output" \
        --limit --collection --query --tei-url --qdrant-url --since --before \
        --no-hybrid-search --color --quiet
      ;;
    "retrieve")
      prove_options "$name" "retrieve returned bounded stored chunks for the requested URL" --max-points
      prove_options "@global" "retrieve used the isolated collection" --collection
      ;;
    "ask")
      prove_options "$name" \
        "ask exercised diagnostic, explanation, stream control, and named session lifecycle modes" \
        --diagnostics --explain --stream --no-stream --session --reset-session \
        --new-session --list-sessions --resume
      prove_options "@global" "grounded answer returned bounded citations from isolated collection" --limit --collection
      ;;
    "summarize"|"suggest") prove_options "@global" "RAG operation completed against isolated collection" --collection ;;
    "evaluate")
      prove_options "$name" \
        "evaluation returned comparison responses plus diagnostic and retrieval A/B projections" \
        --responses-mode --diagnostics --retrieval-ab
      prove_options "@global" "evaluation used isolated collection" --collection
      ;;
    "train")
      prove_options "$name" "training event persisted selected preference and notes" --best --notes
      prove_options "@global" "training event used isolated collection" --collection
      ;;
    "jobs events") prove_options "$name" "event page honored sequence and cursor bounds" --after-sequence --cursor ;;
    "jobs stream") prove_options "$name" "stream page honored sequence bound" --after-sequence ;;
    "jobs cancel") prove_options "$name" "cancellation persisted the caller supplied reason" --reason ;;
    "jobs retry") prove_options "$name" "retry response linked original and new attempts" --mode ;;
    "jobs recover") prove_options "$name" "recovery applied the stale cutoff and requested job kind" --stale-before --kind ;;
    "jobs cleanup") prove_options "$name" "cleanup applied terminal status, job kind, age, and preview policy" --status --kind --older-than --dry-run ;;
    "jobs clear") prove_options "$name" "clear returned an explicit deletion count" --confirm ;;
    "jobs worker") prove_options "$name" "worker exited cleanly at the requested idle bound" --idle-exit-secs ;;
    "memory remember")
      prove_options "$name" \
        "memory persisted the requested title, project, type, repository, file, and confidence metadata" \
        --title --project --type --repo --file --confidence
      ;;
    "memory search") prove_options "$name" "memory search applied project, repository, and file filters" --project --repo --file ;;
    "memory link") prove_options "$name" "memory edge persisted requested relationship type" --type ;;
    "memory context")
      prove_options "$name" "memory context applied project, repository, file, result, and token bounds" \
        --project --repo --file --token-budget
      ;;
    "sessions") prove_options "$name" "session scan returned only the requested provider/project projection" --claude --codex --gemini --project ;;
    "sources") prove_options "$name" "source inventory applied the explicit all/domain projection" --all --domain ;;
    "domains") prove_options "$name" "domain inventory applied the requested domain filter" --domain ;;
    "endpoints")
      prove_options "$name" \
        "endpoint discovery completed with bundle, uniqueness, scan-bound, verification, network, and RPC controls" \
        --include-bundles --first-party-only --unique-only --max-scripts \
        --max-scan-bytes --verify --capture-network --probe-rpc --probe-rpc-subdomains
      ;;
    "prune exec") prove_options "$name" "reviewed generation-scoped prune plan executed only after confirmation" --generation --confirm ;;
    "setup plugin-hook"|"setup hook alias") prove_options "$name" "hook-safe setup returned explicit no-setup exit policy" --no-setup ;;
    "setup config rewrite") prove_options "$name" "config rewrite preview completed without modifying isolated files" --dry-run ;;
    "setup init")
      prove_options "$name" \
        "isolated setup persisted bind, auth, OAuth, and optional provider credentials" \
        --mcp-host --mcp-port --auth-mode --mcp-token --oauth-public-url \
        --google-client-id --google-client-secret --auth-admin-email \
        --tavily-api-key --github-token --reddit-client-id --reddit-client-secret
      ;;
    "migrate") prove_options "$name" "named collection migration reported source, destination, and migrated points" --from --to ;;
    "config list"|"config get") prove_options "$name" "isolated config read honored storage layer and reveal policy" --env --toml --reveal ;;
    "config set"|"config unset") prove_options "$name" "isolated config mutation honored the explicit storage layer" --env --toml ;;
    "update") prove_options "$name" "forced version-pinned update used the requested repository and isolated destination without container sync" --repo --version --force --no-container ;;
    "palette") prove_options "$name" "palette install resolved requested acquisition method in isolated bin dir" --method ;;
    "artifacts get") prove_options "$name" "artifact detail included the requested authenticated content URL" --include-content-url ;;
    "artifacts content")
      prove_options "$name" "artifact content returned the requested byte range and default download" --range --download
      prove_options "@global" "artifact bytes were copied to the requested isolated output" --output
      ;;
    "artifacts list") prove_options "$name" "artifact inventory applied kind, source, job, and cursor filters" --kind --source-id --job-id --cursor ;;
    "uploads create") prove_options "$name" "upload staging record persisted purpose and source hint" --purpose --source-hint ;;
    "uploads complete") prove_options "$name" "upload completion verified the digest and produced an artifact/source reference" --sha256 --source-option ;;
    "collections get")
      prove_options "$name" "collection detail included schema and payload-index projections" --include-schema --include-indexes
      prove_options "@global" "collection detail matched isolated collection" --collection
      ;;
    "graph resolve")
      prove_options "$name" "graph identifier resolution applied the requested node kind" --kind
      prove_options "@global" "graph resolution returned bounded resolved/missing sets" --limit
      ;;
    "graph query")
      prove_options "$name" "graph traversal continued from the returned edge cursor" --cursor
      prove_options "@global" "graph query returned bounded nodes and edges" --limit
      ;;
    "graph node") prove_options "$name" "node detail included requested edges and evidence" --include-edges --include-evidence ;;
    "graph edge") prove_options "$name" "edge detail included evidence" --include-evidence ;;
    "graph source") prove_options "$name" "source traversal honored depth and edge-kind filter" --depth --edge-kind ;;
    "providers get") prove_options "$name" "provider detail included health and limits" --include-health --include-limits ;;
    "status")
      prove_options "@global" \
        "status returned a valid isolated job projection with the requested presentation and state filter" \
        --active --recent --reclaimed --color --quiet
      ;;
    "jobs list")
      prove_options "$name" "job listing applied status, kind, and cursor pagination filters" \
        --status --kind --cursor
      ;;
    "memory list")
      prove_options "$name" "memory list applied project, repository, file, type, and status filters" \
        --project --repo --file --type --status
      ;;
    "preflight") prove_options "$name" "config-only preflight completed without provider probes" --config ;;
    "providers list") prove_options "$name" "provider inventory applied kind and health-status filters" --kind --status ;;
    "prune plan") prove_options "$name" "prune plan remained scoped to the requested generation" --generation ;;
    "uploads list") prove_options "$name" "upload inventory applied status and cursor pagination filters" --status --cursor ;;
    "uploads abort") prove_options "$name" "upload abort receipt retained the caller supplied reason" --reason ;;
  esac
}
