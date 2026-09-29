#!/usr/bin/env bash
# Server projection and presented call form are separate: both mode executes
# two complete sweeps, not just discovery followed by aggregate calls.
tool_selector_for_action() {
  local action="$1" subaction="${2:-}"
  if [[ "$MCP_CALL_FORM" == "atomic" ]]; then
    printf '%s.%s%s\n' "$SERVER" "$action" "${subaction:+_$subaction}"
  else
    printf '%s\n' "$SELECTOR"
  fi
}

call_tool() {
  local action="" subaction="" arg
  for arg in "$@"; do
    [[ "$arg" != action:* ]] || action="${arg#action:}"
    [[ "$arg" != subaction:* ]] || subaction="${arg#subaction:}"
  done
  [[ -n "$action" ]] || { echo "FAIL: call_tool requires action:<name>" >&2; return 2; }
  local -a command=("${MCPORTER[@]}" call "$(tool_selector_for_action "$action" "$subaction")")
  for arg in "$@"; do
    if [[ "$MCP_CALL_FORM" == "atomic" && ( "$arg" == action:* || "$arg" == subaction:* ) ]]; then
      continue
    fi
    command+=("$arg")
  done
  command+=(--output json)
  "${command[@]}"
}

call_tool_json() {
  local projected
  projected="$(python3 "$MCP_ADAPTER" project-call "$1" --form "$MCP_CALL_FORM" --selector "$SELECTOR")"
  "${MCPORTER[@]}" call "$(jq -er '.selector' <<<"$projected")" \
    --args "$(jq -c '.arguments' <<<"$projected")" --output json
}

call_tool_with_timeout() {
  local timeout_ms="$1"
  shift
  MCPORTER_CALL_TIMEOUT="$timeout_ms" call_tool "$@"
}

assert_complete_tool_inventory() {
  local schema_file="$1" help_file="$2"
  local actual="$OUTDIR/inventory-tools.json" canonical="$OUTDIR/inventory-help.json"
  json_payload "$schema_file" >"$actual"
  json_payload "$help_file" >"$canonical"
  python3 "$MCP_ADAPTER" inventory "$actual" "$canonical" --projection "$MCP_PROJECTION"
}
