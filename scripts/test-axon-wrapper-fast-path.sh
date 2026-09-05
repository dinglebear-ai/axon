#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
wrapper="$root/scripts/axon"
fast_line="$(grep -n 'if \[ "${AXON_NO_BUILD:-0}" = 1 \]' "$wrapper" | cut -d: -f1)"
build_line="$(grep -n '^cargo build ' "$wrapper" | cut -d: -f1)"
[ -n "$fast_line" ] && [ -n "$build_line" ] && [ "$fast_line" -lt "$build_line" ]
grep -Fq 'exec "$AXON_BIN" "$@"' "$wrapper"
grep -Fq "AXON_NO_BUILD=1 ./scripts/axon status" "$root/docs/operations/operations.md"
echo 'ok - axon monitoring fast-path contract passed'
