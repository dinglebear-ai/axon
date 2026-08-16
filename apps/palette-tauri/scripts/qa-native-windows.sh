#!/usr/bin/env bash
set -euo pipefail

# Native Windows smoke harness for the real Tauri binary on Agent OS.
#
# Prerequisites:
#   - agent-os SSH alias and interactive console session
#   - windows-mcp listening on agent-os:8000
#   - WMCP_TOKEN in the environment (never written by this script)
#   - a Windows palette executable (override PALETTE_EXE as needed)
#
# This intentionally does not seed settings.json: connection/auth must be
# exercised through the shipped UI. Evidence is written outside the repo by
# default so bearer tokens and workstation details cannot be committed.

app_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
palette_exe=${PALETTE_EXE:-"$app_root/src-tauri/target/x86_64-pc-windows-gnu/release/axon-palette-tauri.exe"}
palette_dir=$(dirname "$palette_exe")
run_id=${RUN_ID:-"$(date +%Y%m%d_%H%M%S)"}
evidence_dir=${EVIDENCE_DIR:-"$HOME/.agents/docs/sessions/axon-palette-desktop-test/run_${run_id}/evidence"}
guest_dir='C:/Users/Docker/AxonPaletteQA'
guest_exe="$guest_dir/axon-palette-tauri.exe"

: "${WMCP_TOKEN:?set WMCP_TOKEN to the Agent OS windows-mcp bearer token}"
[[ -f "$palette_exe" ]] || { echo "missing palette executable: $palette_exe" >&2; exit 2; }
[[ -f "$palette_dir/WebView2Loader.dll" ]] || {
  echo "missing required Windows runtime beside executable: $palette_dir/WebView2Loader.dll" >&2
  exit 2
}
mkdir -p "$evidence_dir/logs"

wmcp() {
  local tool=$1 args=$2
  mcporter call --http-url http://agent-os:8000/mcp --allow-http \
    --header "Authorization=Bearer ${WMCP_TOKEN}" --tool "$tool" \
    --args "$args" --output json
}

ssh agent-os "powershell -NoProfile -Command \"New-Item -ItemType Directory -Force '$guest_dir' | Out-Null; Stop-Process -Name axon-palette-tauri -Force -ErrorAction SilentlyContinue\""
scp "$palette_exe" "agent-os:$guest_exe"
scp "$palette_dir/WebView2Loader.dll" "agent-os:$guest_dir/WebView2Loader.dll"
ssh agent-os "powershell -NoProfile -Command \"Unblock-File '$guest_exe'\""

wmcp PowerShell "{\"command\":\"Start-Process '$guest_exe'; Start-Sleep 2; (Get-Process axon-palette-tauri -ErrorAction SilentlyContinue | Select-Object -First 1 -ExpandProperty Id)\"}" \
  >"$evidence_dir/logs/launch.json"
wmcp Shortcut '{"shortcut":"ctrl+shift+space"}' >"$evidence_dir/logs/show.json"
sleep 2
wmcp Snapshot '{"use_ui_tree":true}' >"$evidence_dir/tree01_initial.json"
mcporter call --http-url http://agent-os:8000/mcp --allow-http \
  --header "Authorization=Bearer ${WMCP_TOKEN}" --tool Screenshot --args '{}' \
  --save-images "$evidence_dir" --output json >"$evidence_dir/cp01_initial.json"

echo "Native palette launched on Agent OS. Initial evidence: $evidence_dir"
