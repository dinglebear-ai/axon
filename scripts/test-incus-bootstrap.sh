#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
bootstrap="$repo_root/deploy/incus/bootstrap.sh"

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

echo "incus bootstrap exact-instance state lookup ok"
