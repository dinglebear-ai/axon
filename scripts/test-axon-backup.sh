#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/bin" "$work/out"

cat >"$work/bin/curl" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
printf '%q ' "$@" >>"$CALLS"; printf '\n' >>"$CALLS"
out=""
for ((i=1; i<=$#; i++)); do
  if [ "${!i}" = "-o" ]; then j=$((i+1)); out="${!j}"; fi
done
if [ -n "$out" ]; then printf snapshot-data >"$out"; exit 0; fi
case "$*" in
  *'/snapshots/upload?'*) printf '{"status":"ok"}' ;;
  *'-X DELETE'*) printf '{"status":"ok"}' ;;
  *) printf '{"status":"ok","result":{"name":"server.snapshot"}}' ;;
esac
SH
chmod +x "$work/bin/curl"
cat >"$work/bin/sha256sum" <<'SH'
#!/usr/bin/env sh
printf 'abc  %s\n' "$1"
SH
chmod +x "$work/bin/sha256sum"

if ! CALLS="$work/calls" PATH="$work/bin:/usr/bin:/bin" HOME="$work/home" \
  QDRANT_API_KEY='secret api key' AXON_SQLITE_PATH="$work/missing.db" \
  AXON_BACKUP_DIR="$work/out" bash "$root/scripts/axon-backup.sh" --yes >"$work/stdout" 2>"$work/stderr"; then
  cat "$work/stdout" "$work/stderr" >&2
  exit 1
fi

grep -q 'SQLite: not included' "$work/stdout"
! grep -q 'secret api key' "$work/stdout" "$work/stderr"
[ "$(stat -f '%Lp' "$work/out" 2>/dev/null || stat -c '%a' "$work/out")" = 700 ]
snapshot="$(find "$work/out/qdrant" -type f -name '*.snapshot' -print -quit)"
[ "$(stat -f '%Lp' "$snapshot" 2>/dev/null || stat -c '%a' "$snapshot")" = 600 ]
[ -f "$(find "$work/out" -maxdepth 1 -name 'backup-*.json' -print -quit)" ]
[ "$(grep -c 'api-key:' "$work/calls")" -eq 3 ]

: >"$work/calls"
CALLS="$work/calls" PATH="$work/bin:/usr/bin:/bin" QDRANT_API_KEY='secret api key' \
  bash "$root/scripts/axon-backup.sh" --collection axon --restore "$snapshot" --yes >/dev/null
grep -q '/snapshots/upload' "$work/calls"
grep -q 'api-key:' "$work/calls"

echo 'ok - backup and restore contracts passed'
