#!/usr/bin/env bash
# Non-executing command and option recognition coverage.

run_help_path() {
  local encoded="$1" name path_json logfile exit_code result
  name="$(printf '%s' "$encoded" | base64 --decode | jq -r '.name')"
  path_json="$(printf '%s' "$encoded" | base64 --decode | jq -c '.path')"
  logfile="$OUTDIR/logs/help-$(printf '%s' "$name" | tr ' /' '__').log"
  mapfile -t path < <(printf '%s' "$path_json" | jq -r '.[]')

  timeout "${TIMEOUT_SECS}s" "$AXON_BIN" "${path[@]}" --help >"$logfile" 2>&1
  exit_code=$?
  if [ "$exit_code" -eq 0 ] && grep -Fq "axon ${path[*]}" "$logfile"; then
    result="PASS"
  else
    result="FAIL"
    failures=$((failures + 1))
  fi
  record "$name" "parser-command" "$result" "$exit_code" "${path[*]} --help" "$logfile"
}

run_root_help() {
  local logfile="$OUTDIR/logs/help-root.log" exit_code result
  timeout "${TIMEOUT_SECS}s" "$AXON_BIN" --help >"$logfile" 2>&1
  exit_code=$?
  if [ "$exit_code" -eq 0 ]; then
    result="PASS"
  else
    result="FAIL"
    failures=$((failures + 1))
  fi
  record "<root>" "parser-command" "$result" "$exit_code" "--help" "$logfile"
}

option_value() {
  local command_name="$1" option="$2" value_name="${3^^}"
  case "$option" in
    --wait|--cache|--include-subdomains|--normalize|--block-assets|--chrome-screenshot|--screenshot-full-page)
      echo "true"
      return
      ;;
    --performance-profile) echo "high-stable"; return ;;
    --render-mode) echo "http"; return ;;
    --format) echo "json"; return ;;
    --color) echo "never"; return ;;
    --search-time-range) echo "day"; return ;;
    --responses-mode) echo "side-by-side"; return ;;
    --transport) echo "stdio"; return ;;
    --method) echo "build"; return ;;
    --auth-mode) echo "bearer"; return ;;
    --stale-before|--older-than) echo "2026-01-01T00:00:00Z"; return ;;
    --purpose) echo "source_artifact"; return ;;
    --shell) echo "bash"; return ;;
    --mode)
      case "$command_name" in
        "jobs retry") echo "same_config" ;;
        *) echo "http" ;;
      esac
      return
      ;;
  esac
  case "$value_name" in
    *BOOL*) echo "true" ;;
    *URL*) echo "http://127.0.0.1:8001" ;;
    *DATE*|*SINCE*) echo "2026-01-01" ;;
    *BEFORE*) echo "2026-01-01T00:00:00Z" ;;
    *HEADER*) echo "X-Axon-Smoke: enabled" ;;
    *COLLECTION*) echo "axon" ;;
    *TRANSPORT*) echo "stdio" ;;
    *SHELL*) echo "bash" ;;
    *COLOR*) echo "never" ;;
    *RENDER_MODE*) echo "http" ;;
    *RESPONSES_MODE*) echo "side-by-side" ;;
    *PROFILE*) echo "high-stable" ;;
    *SEARCH_TIME_RANGE*|*RANGE*) echo "day" ;;
    *STATUS*) echo "completed" ;;
    *KIND*) echo "source" ;;
    *SCOPE*) echo "page" ;;
    *FORMAT*) echo "json" ;;
    *PATH=N*) echo "*=1" ;;
    *VIEWPORT*) echo "1280x720" ;;
    *SELECTOR*) echo "body" ;;
    *PATTERN*) echo "https://example.com/*" ;;
    *FILE*|*PATH*|*DIR*) echo "/tmp/axon-live-cli-contract" ;;
    *EMAIL*) echo "smoke@example.com" ;;
    *HOST*) echo "127.0.0.1" ;;
    *PORT*) echo "38001" ;;
    *ID*) echo "00000000-0000-0000-0000-000000000000" ;;
    *TEXT*|*QUERY*|*NAME*|*NOTES*) echo "smoke" ;;
    *TOKEN*|*SECRET*|*KEY*) echo "axon-live-smoke-secret" ;;
    *SUBCOMMAND*) echo "launch" ;;
    *) echo "1" ;;
  esac
}

run_option_probe() {
  local name="$1" option="$2" value_name="$3"
  shift 3
  local logfile exit_code result invocation
  local -a path=("$@")
  logfile="$OUTDIR/logs/option-$(printf '%s-%s' "$name" "$option" | tr ' /' '___').log"
  invocation="${path[*]} $option"
  if [ -n "$value_name" ]; then
    local value
    value="$(option_value "$name" "$option" "$value_name")"
    timeout "${TIMEOUT_SECS}s" "$AXON_BIN" "${path[@]}" "$option" "$value" --help >"$logfile" 2>&1
    invocation="$invocation $value --help"
  else
    timeout "${TIMEOUT_SECS}s" "$AXON_BIN" "${path[@]}" "$option" --help >"$logfile" 2>&1
    invocation="$invocation --help"
  fi
  exit_code=$?
  if [ "$exit_code" -eq 0 ]; then
    result="PASS"
  else
    result="FAIL"
    failures=$((failures + 1))
  fi
  record "$name" "parser-option" "$result" "$exit_code" "$invocation" "$logfile"
}

run_parser_rejection() {
  local name="$1" expected="$2"
  shift 2
  local logfile exit_code result
  logfile="$OUTDIR/logs/parser-contract-$(printf '%s' "$name" | tr ' /' '__').log"
  timeout "${TIMEOUT_SECS}s" "$AXON_BIN" "$@" >"$logfile" 2>&1
  exit_code=$?
  if [ "$exit_code" -eq 2 ] && grep -Fq -- "$expected" "$logfile"; then
    result="PASS"
  else
    result="FAIL"
    failures=$((failures + 1))
  fi
  record "$name" "parser-contract" "$result" "$exit_code" "$*" "$logfile"
}

probe_help_options() {
  local name="$1" logfile="$2"
  shift 2
  local -a path=("$@")
  local spec long short value_name
  while IFS= read -r spec; do
    long="$(grep -oE -- '--[a-z0-9][a-z0-9-]*' <<<"$spec" | head -1 || true)"
    short="$(grep -oE -- '(^|, )-[A-Za-z](,| |$)' <<<"$spec" | grep -oE -- '-[A-Za-z]' | head -1 || true)"
    value_name="$(grep -oE '<[^>]+>' <<<"$spec" | head -1 | tr -d '<>' || true)"
    if [ -n "$long" ] && [ "$long" != "--help" ]; then
      run_option_probe "$name" "$long" "$value_name" "${path[@]}"
    fi
    if [ -n "$short" ] && [ "$short" != "-h" ]; then
      run_option_probe "$name" "$short" "$value_name" "${path[@]}"
    fi
  done < <(
    awk '
      /^Options$/ { in_options=1; next }
      in_options && /^  -/ {
        line=$0
        sub(/^  /, "", line)
        sub(/[[:space:]][[:space:]]+.*/, "", line)
        print line
      }
      in_options && /^[^ ]/ { in_options=0 }
    ' "$logfile"
  )
}

if [ "$MODE" != "scenarios" ]; then
  run_root_help
  probe_help_options "<root>" "$OUTDIR/logs/help-root.log"

  while IFS= read -r encoded; do
    run_help_path "$encoded"
    name="$(printf '%s' "$encoded" | base64 --decode | jq -r '.name')"
    path_json="$(printf '%s' "$encoded" | base64 --decode | jq -c '.path')"
    mapfile -t path < <(printf '%s' "$path_json" | jq -r '.[]')
    help_log="$OUTDIR/logs/help-$(printf '%s' "$name" | tr ' /' '__').log"
    probe_help_options "$name" "$help_log" "${path[@]}"
  done < <(jq -r '.commands[] | @base64' "$REGISTRY")
fi

probe_hidden_global_options() {
  run_option_probe "<root>" "--automation-script" "PATH"
  run_option_probe "<root>" "--batch-concurrency" "N"
  run_option_probe "<root>" "--block-assets" "BOOL"
  run_option_probe "<root>" "--budget" "PATH=N"
  run_option_probe "<root>" "--cache" "BOOL"
  run_option_probe "<root>" "--cache-http-only" ""
  run_option_probe "<root>" "--chrome-screenshot" "BOOL"
  run_option_probe "<root>" "--chrome-wait-for-selector" "SELECTOR"
  run_option_probe "<root>" "--color" "COLOR"
  run_option_probe "<root>" "--cron-every-seconds" "N"
  run_option_probe "<root>" "--cron-max-runs" "N"
  run_option_probe "<root>" "--etag-conditional" ""
  run_option_probe "<root>" "--exclude-path" "PATH"
  run_option_probe "<root>" "--exclude-path-prefix" "PATH"
  run_option_probe "<root>" "--exclude-selector" "SELECTOR"
  run_option_probe "<root>" "--format" "FORMAT"
  run_option_probe "<root>" "--normalize" "BOOL"
  run_option_probe "<root>" "--output-dir" "DIR"
  run_option_probe "<root>" "--performance-profile" "PROFILE"
  run_option_probe "<root>" "--quiet" ""
  run_option_probe "<root>" "--root-selector" "SELECTOR"
  run_option_probe "<root>" "--screenshot-full-page" "BOOL"
  run_option_probe "<root>" "--sitemap-only" ""
  run_option_probe "<root>" "--url-glob" "PATTERN"
  run_option_probe "<root>" "--urls" "URLS"
  run_option_probe "<root>" "--viewport" "VIEWPORT"
  run_option_probe "<root>" "--warc" "PATH"
  run_option_probe "<root>" "--yes" ""
}

if [ "$MODE" != "scenarios" ]; then
  probe_hidden_global_options
  run_option_probe "ask" "--continue" "" ask
  if [ "$REGISTRY" = "$ROOT_DIR/docs/reference/cli/commands.json" ]; then
    run_parser_rejection "invalid render mode" "invalid value 'bogus'"       source https://example.com --render-mode bogus --json
    run_parser_rejection "conflicting status filters" "cannot be used with '--recent'"       status --active --recent --json
  fi
fi

probe_command_alias() {
  local name="$1"
  shift
  local logfile
  local exit_code result
  logfile="$OUTDIR/logs/alias-$(printf '%s' "$name" | tr ' /' '__').log"
  timeout "${TIMEOUT_SECS}s" "$AXON_BIN" "$@" --help >"$logfile" 2>&1
  exit_code=$?
  if [ "$exit_code" -eq 0 ]; then
    result="PASS"
  else
    result="FAIL"
    failures=$((failures + 1))
  fi
  record "$name" "parser-alias" "$result" "$exit_code" "$* --help" "$logfile"
}

if [ "$MODE" != "scenarios" ]; then
  probe_command_alias "completions" completion
  probe_command_alias "setup plugin-hook" setup hook
fi

