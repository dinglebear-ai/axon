#!/usr/bin/env bash

tool_selector_for_action() {
  local action="$1"
  if [[ "$MCP_PROJECTION" == "atomic" ]]; then
    printf '%s.axon_%s\n' "$SERVER" "$action"
  else
    printf '%s\n' "$SELECTOR"
  fi
}
