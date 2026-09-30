#!/usr/bin/env bash
# Export the reviewed CLI version only. --check never touches the snapshot.
set -euo pipefail
repo_root=$(cd "$(dirname "$0")/.." && pwd)
check=false
if [ "$#" -gt 0 ]; then
    if [ "$#" -eq 1 ] && [ "$1" = "--check" ]; then check=true
    else echo "Usage: $0 [--check]; select the reviewed executable with CODEX_BIN" >&2; exit 2
    fi
fi
codex_bin="$(command -v codex || true)"
if [ -n "${CODEX_BIN:-}" ]; then codex_bin="${CODEX_BIN:-}"; fi
if [ -z "$codex_bin" ]; then
    echo "Codex schema export: codex executable missing; install the version in contracts/codex-app-server-version.txt. No output was written." >&2
    exit 2
fi
pinned=$(tr -d '\r\n' < "$repo_root/contracts/codex-app-server-version.txt")
observed=$("$codex_bin" --version | awk '{print $2}')
if [ "$observed" != "$pinned" ]; then
    echo "Codex schema export: expected CLI $pinned, observed $observed. Select a matching CODEX_BIN; review a version change separately. No output was written." >&2
    exit 2
fi
schema_dir=$(mktemp -d)
trap 'rm -rf "$schema_dir"' EXIT
"$codex_bin" app-server generate-json-schema --experimental --out "$schema_dir" >/dev/null
methods() {
    jq -ce '[.. | objects | .properties?.method?.enum? // empty | .[]] | unique | if length > 0 then . else error("empty protocol method inventory") end' "$1"
}
jq -n --sort-keys \
    --arg schema_version "v2-experimental" \
    --arg codex_cli_version "$observed" \
    --argjson client_requests "$(methods "$schema_dir/ClientRequest.json")" \
    --argjson server_requests "$(methods "$schema_dir/ServerRequest.json")" \
    --argjson server_notifications "$(methods "$schema_dir/ServerNotification.json")" \
    '{schema_version: $schema_version, codex_cli_version: $codex_cli_version,
      client_requests: $client_requests, server_requests: $server_requests,
      server_notifications: $server_notifications}' > "$schema_dir/methods.json"
output="$repo_root/docs/reference/codex-app-server-methods.json"
if $check; then
    if ! cmp -s "$output" "$schema_dir/methods.json"; then
        echo "Codex schema snapshot drift: run scripts/generate-codex-app-server-methods.sh with the pinned CLI and review the diff." >&2
        exit 1
    fi
    echo "Codex schema snapshot matches pinned CLI $pinned"
else
    cp "$schema_dir/methods.json" "$output"
    echo "Updated Codex schema snapshot from pinned CLI $pinned"
fi
