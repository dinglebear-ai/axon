#!/usr/bin/env bash
# Behavioral coverage reconciliation and final summary.

build_behavioral_coverage_report() {
  local root_options encoded name path_json help_log option key result evidence
  ensure_behavior_global_options
  root_options="$BEHAVIOR_GLOBAL_OPTIONS"

  : >"$BEHAVIOR_EXPECTED"
  if [ "$REGISTRY" = "$ROOT_DIR/docs/reference/cli/commands.json" ]; then
    while IFS= read -r option; do
      if [ "$option" = "--help" ] || [ "$option" = "-h" ]; then
        continue
      fi
      printf '%s\t%s\n' "@global" "$option" >>"$BEHAVIOR_EXPECTED"
    done <"$root_options"
    {
      printf '%s\t%s\n' "ask" "--continue"
      printf '%s\t%s\n' "completion alias" "__command__"
      printf '%s\t%s\n' "setup hook alias" "__command__"
    } >>"$BEHAVIOR_EXPECTED"
  fi

  while IFS= read -r encoded; do
    name="$(printf '%s' "$encoded" | base64 --decode | jq -r '.name')"
    path_json="$(printf '%s' "$encoded" | base64 --decode | jq -c '.path')"
    mapfile -t path < <(printf '%s' "$path_json" | jq -r '.[]')
    help_log="$OUTDIR/logs/behavior-help-$(printf '%s' "$name" | tr ' /' '__').log"
    "$AXON_BIN" "${path[@]}" --help >"$help_log" 2>&1
    printf '%s\t%s\n' "$name" "__command__" >>"$BEHAVIOR_EXPECTED"
    while IFS= read -r option; do
      [ "$option" = "--help" ] && continue
      if ! grep -Fqx -- "$option" "$root_options"; then
        printf '%s\t%s\n' "$name" "$option" >>"$BEHAVIOR_EXPECTED"
      fi
    done < <(
      awk '
        /^Options$/ { in_options=1; next }
        in_options && /^[^ ]/ { exit }
        in_options && /^  (-[A-Za-z], )?--[a-z0-9]/ {
          line=$0
          sub(/^  (-[A-Za-z], )?/, "", line)
          sub(/[[:space:]].*/, "", line)
          print line
        }
      ' "$help_log"
    )
    while IFS= read -r option; do
      [ "$option" = "-h" ] && continue
      if ! grep -Fqx -- "$option" "$root_options"; then
        printf '%s\t%s\n' "$name" "$option" >>"$BEHAVIOR_EXPECTED"
      fi
    done < <(
      awk '/^  -[A-Za-z], --[a-z0-9]/ { print substr($1, 1, 2) }' "$help_log"
    )
  done < <(jq -r '.commands[] | @base64' "$REGISTRY")

  sort -u -o "$BEHAVIOR_EXPECTED" "$BEHAVIOR_EXPECTED"
  sort -u -o "$BEHAVIOR_ACTUAL" "$BEHAVIOR_ACTUAL"
  sort -u -o "$BEHAVIOR_SEMANTIC" "$BEHAVIOR_SEMANTIC"
  while IFS= read -r key; do
    name="${key%%	*}"
    option="${key#*	}"
    evidence=""
    if [ "$option" = "__command__" ] && grep -Fqx -- "$key" "$BEHAVIOR_ACTUAL"; then
      result="PASS"
      evidence="executed with a command-specific contract"
    elif evidence="$(awk -F '\t' -v key="$key" '$1 FS $2 == key { print $3; exit }' "$BEHAVIOR_SEMANTIC")" \
      && [ -n "$evidence" ]; then
      result="PASS"
    else
      result="FAIL"
      failures=$((failures + 1))
    fi
    evidence="${evidence//$'\t'/ }"
    evidence="${evidence//$'\r'/ }"
    evidence="${evidence//$'\n'/ }"
    printf '%s\t%s\t%s\t%s\n' "$name" "$option" "$result" "$evidence" >>"$BEHAVIOR_REPORT"
  done <"$BEHAVIOR_EXPECTED"
}

if [ "$MODE" = "live" ] || [ "$MODE" = "scenarios" ]; then
  build_behavioral_coverage_report
fi

if ! awk -F '\t' 'NF != 6 { bad=1 } END { exit bad }' "$REPORT"; then
  echo "report structural validation failed: expected 6 TSV fields per physical line" >&2
  failures=$((failures + 1))
fi
if [ "$MODE" != "registry" ] \
  && ! awk -F '\t' 'NF != 4 { bad=1 } END { exit bad }' "$BEHAVIOR_REPORT"; then
  echo "behavior report structural validation failed: expected 4 TSV fields per physical line" >&2
  failures=$((failures + 1))
fi

passed="$(awk -F '\t' 'NR > 1 && $3 == "PASS" { count++ } END { print count + 0 }' "$REPORT")"
skipped="$(awk -F '\t' 'NR > 1 && $3 == "SKIP" { count++ } END { print count + 0 }' "$REPORT")"
behavior_passed="$(awk -F '\t' 'NR > 1 && $3 == "PASS" { count++ } END { print count + 0 }' "$BEHAVIOR_REPORT")"
behavior_failed="$(awk -F '\t' 'NR > 1 && $3 == "FAIL" { count++ } END { print count + 0 }' "$BEHAVIOR_REPORT")"
echo "Axon CLI smoke: mode=$MODE passed=$passed skipped=$skipped failed=$failures"
if [ "$MODE" = "registry" ]; then
  echo "Parser checks: $passed passed; no command behavior was executed."
else
  echo "Behavioral coverage: $behavior_passed passed, $behavior_failed missing."
  echo "Behavioral coverage report: $BEHAVIOR_REPORT"
fi
echo "Report: $REPORT"
[ "$failures" -eq 0 ] && [ "$skipped" -eq 0 ]
