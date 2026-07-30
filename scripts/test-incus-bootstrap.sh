#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
bootstrap="$repo_root/deploy/incus/bootstrap.sh"

expected='state="$(incus info "$CONTAINER_NAME" 2>/dev/null | awk -F'\'': '\'' '\''$1 == "Status" { print $2; exit }'\'')"'
grep -Fqx "$expected" "$bootstrap"

if grep -Fq 'incus list "$CONTAINER_NAME"' "$bootstrap"; then
  echo "bootstrap must query the exact instance; incus list uses prefix/filter matching" >&2
  exit 1
fi

echo "incus bootstrap exact-instance state lookup ok"
