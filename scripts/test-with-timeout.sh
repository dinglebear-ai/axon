#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

cat >"$work/tree.sh" <<'SH'
#!/usr/bin/env bash
(sleep 30) &
echo "$!" >"$1"
wait
SH
chmod +x "$work/tree.sh"

set +e
AXON_TIMEOUT_FORCE_FALLBACK=1 "$root/scripts/with_timeout.sh" 1 -- "$work/tree.sh" "$work/child.pid"
status=$?
set -e
[ "$status" -eq 124 ]
child="$(cat "$work/child.pid")"
if kill -0 "$child" 2>/dev/null; then
  echo "timed-out descendant remains alive: $child" >&2
  exit 1
fi
AXON_TIMEOUT_FORCE_FALLBACK=1 "$root/scripts/with_timeout.sh" 5 -- sh -c 'exit 7' || [ "$?" -eq 7 ]
echo 'ok - timeout process-group fallback passed'
