#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
schema_dir=$(mktemp -d)
trap 'rm -rf "$schema_dir"' EXIT

codex_version=$(codex --version | awk '{print $2}')
codex app-server generate-json-schema --experimental --out "$schema_dir" >/dev/null

methods() {
  jq -c '[.. | objects | .properties?.method?.enum? // empty | .[]] | unique' "$1"
}

jq -n \
  --arg schema_version "v2-experimental" \
  --arg codex_cli_version "$codex_version" \
  --argjson client_requests "$(methods "$schema_dir/ClientRequest.json")" \
  --argjson server_requests "$(methods "$schema_dir/ServerRequest.json")" \
  --argjson server_notifications "$(methods "$schema_dir/ServerNotification.json")" \
  '{schema_version: $schema_version, codex_cli_version: $codex_cli_version,
    client_requests: $client_requests, server_requests: $server_requests,
    server_notifications: $server_notifications}' \
  > "$repo_root/docs/reference/codex-app-server-methods.json"

jq --sort-keys . "$repo_root/docs/reference/codex-app-server-methods.json" \
  > "$repo_root/docs/reference/codex-app-server-methods.json.tmp"
mv "$repo_root/docs/reference/codex-app-server-methods.json.tmp" \
  "$repo_root/docs/reference/codex-app-server-methods.json"
