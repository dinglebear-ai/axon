#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
android_dir="$repo_root/apps/android"
sdk_root="${ANDROID_SDK_ROOT:-$HOME/Android/Sdk}"
adb_bin="${ADB:-$sdk_root/platform-tools/adb}"
driver="${ANDROIDTEST_DRIVER:-$HOME/.codex/plugins/cache/dendrite-no-mcp/testing/local/skills/android-app-testing/scripts/androidtest.sh}"
apk="$android_dir/app/build/outputs/apk/debug/app-debug.apk"
run_id="${RUN_ID:-$(date +%Y%m%d_%H%M%S)}"
run_dir="${RUN_DIR:-$HOME/.agents/docs/sessions/axon-android-test/run_$run_id}"

mkdir -p "$run_dir/evidence/logs"

case "${1:-all}" in
  build)
    AXON_AURORA_ANDROID_PATH="${AXON_AURORA_ANDROID_PATH:-$HOME/workspace/aurora/android}" \
      "$android_dir/gradlew" -p "$android_dir" :app:assembleDebug --no-daemon
    ;;
  install)
    "$driver" ready
    "$driver" install "$apk"
    ;;
  capture)
    name="${2:?capture requires an evidence name}"
    "$driver" shot "$run_dir" "$name"
    "$driver" tree "$run_dir" "tree_${name}"
    "$driver" current >"$run_dir/evidence/logs/${name}_activity.log"
    "$driver" crashes >"$run_dir/evidence/logs/${name}_crashes.log"
    ;;
  server-check)
    env_file="${AXON_ENV_FILE:-$HOME/.axon/.env}"
    server_url="${AXON_SERVER_URL:-$(sed -n 's/^AXON_PUBLIC_URL=//p' "$env_file" | tail -1)}"
    token="${AXON_HTTP_TOKEN:-$(sed -n 's/^AXON_HTTP_TOKEN=//p' "$env_file" | tail -1)}"
    [[ -n "$server_url" && -n "$token" ]] || { echo "missing server URL or token" >&2; exit 2; }
    code="$(curl -sS -o "$run_dir/evidence/logs/doctor.json" -w '%{http_code}' --max-time 20 \
      -H "Authorization: Bearer $token" "${server_url%/}/v1/doctor")"
    unset token
    [[ "$code" == 200 ]] || { echo "doctor failed: HTTP $code" >&2; exit 1; }
    echo "doctor HTTP 200"
    ;;
  all)
    "$0" build
    "$0" install
    "$driver" launch com.axon.app
    sleep 16
    RUN_DIR="$run_dir" "$0" capture launch
    RUN_DIR="$run_dir" "$0" server-check
    echo "$run_dir"
    ;;
  *)
    echo "usage: $0 {all|build|install|capture NAME|server-check}" >&2
    exit 2
    ;;
esac
