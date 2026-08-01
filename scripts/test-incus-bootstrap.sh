#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
bootstrap="$repo_root/deploy/incus/bootstrap.sh"
# shellcheck source=deploy/incus/proxy-devices.sh
source "$repo_root/deploy/incus/proxy-devices.sh"

expected='state="$(incus info "$CONTAINER_NAME" 2>/dev/null | awk -F'\'': '\'' '\''$1 == "Status" { status=$2 } END { print status }'\'')"'
grep -Fqx "$expected" "$bootstrap"

parsed_state="$(printf 'Name: axon\nStatus: RUNNING\nResources:\n  Processes: 3\n' |
  awk -F': ' '$1 == "Status" { status=$2 } END { print status }')"
[ "$parsed_state" = "RUNNING" ]

if grep -Fq 'incus list "$CONTAINER_NAME"' "$bootstrap"; then
  echo "bootstrap must query the exact instance; incus list uses prefix/filter matching" >&2
  exit 1
fi

grep -Fq 'container_data_path="$(incus profile device get "$PROFILE_NAME" axon-data path' "$bootstrap"
grep -Fq 'container_data_source="$(incus profile device get "$PROFILE_NAME" axon-data source' "$bootstrap"
grep -Fq '"$CONTAINER_NAME$container_data_path/.env"' "$bootstrap"
grep -Fq 'EnvironmentFile=$container_data_path/.env' "$bootstrap"
grep -Fq 'if [ "$host_env_canonical" = "$mounted_env_canonical" ]; then' "$bootstrap"
grep -Fq 'RUN_INCUS_SERVER="${AXON_INCUS_RUN_SERVER:-false}"' "$bootstrap"
grep -Fq 'host_glibc="$(getconf GNU_LIBC_VERSION)"' "$bootstrap"
grep -Fq 'incus exec "$CONTAINER_NAME" -- /usr/local/bin/axon --version' "$bootstrap"
grep -Fq 'systemctl disable --now axon-native.service' "$bootstrap"
grep -Fq 'removing mcp-publish because axon-native is disabled' "$bootstrap"
grep -Fq 'unit_tmp="$(mktemp "${TMPDIR:-/tmp}/axon-native.service.XXXXXX")"' "$bootstrap"
if grep -Fq 'cat > /tmp/axon-native.service' "$bootstrap"; then
  echo "bootstrap must not write a predictable privileged file under /tmp" >&2
  exit 1
fi

calls="$(mktemp)"
incus() {
  printf '%s\n' "$*" >> "$calls"
  if [ "${DEVICE_EXISTS:-1}" = "0" ] && [ "$*" = "config device set axon chrome-publish listen=tcp:127.0.0.1:6000 connect=tcp:127.0.0.1:6000" ]; then
    return 1
  fi
}
ensure_loopback_proxy axon chrome-publish tcp:127.0.0.1:6000 tcp:127.0.0.1:6000
grep -Fqx \
  'config device set axon chrome-publish listen=tcp:127.0.0.1:6000 connect=tcp:127.0.0.1:6000' \
  "$calls"
if grep -Fq 'config device remove' "$calls"; then
  echo "proxy reconciliation must not remove a working device first" >&2
  exit 1
fi
DEVICE_EXISTS=0 ensure_loopback_proxy axon chrome-publish tcp:127.0.0.1:6000 tcp:127.0.0.1:6000
grep -Fqx \
  'config device add axon chrome-publish proxy listen=tcp:127.0.0.1:6000 connect=tcp:127.0.0.1:6000' \
  "$calls"
if ensure_loopback_proxy axon chrome-publish tcp:0.0.0.0:6000 tcp:127.0.0.1:6000; then
  echo "Chrome proxy must refuse a non-loopback listener" >&2
  exit 1
fi
rm -f "$calls"

echo "incus bootstrap exact-instance state lookup ok"
