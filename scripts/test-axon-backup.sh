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
[ "$(stat -c '%a' "$work/out" 2>/dev/null || stat -f '%Lp' "$work/out")" = 700 ]
snapshot="$(find "$work/out/qdrant" -type f -name '*.snapshot' -print -quit)"
[ "$(stat -c '%a' "$snapshot" 2>/dev/null || stat -f '%Lp' "$snapshot")" = 600 ]
[ -f "$(find "$work/out" -maxdepth 1 -name 'backup-*.json' -print -quit)" ]
[ "$(grep -c 'api-key:' "$work/calls")" -eq 3 ]

# Exercise the BSD fallback even on Linux CI: reject GNU -c and provide the
# BSD ownership and permission formats used by axon-backup.sh.
mkdir -p "$work/bsd-bin" "$work/bsd-out"
cat >"$work/bsd-bin/stat" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
if [ "$1" = "-c" ]; then exit 1; fi
[ "$1" = "-f" ] || exit 2
printf '%s\n' "$2" >>"$STAT_CALLS"
case "$2" in
  '%u') id -u ;;
  '%Lp')
    if [ "$(uname -s)" = Darwin ]; then
      /usr/bin/stat -f '%Lp' "$3"
    else
      /usr/bin/stat -c '%a' "$3"
    fi
    ;;
  *) exit 2 ;;
esac
SH
chmod +x "$work/bsd-bin/stat"
: >"$work/bsd-stat-calls"
CALLS="$work/bsd-calls" STAT_CALLS="$work/bsd-stat-calls" \
  PATH="$work/bsd-bin:$work/bin:/usr/bin:/bin" HOME="$work/bsd-home" \
  AXON_SQLITE_PATH="$work/missing.db" AXON_BACKUP_DIR="$work/bsd-out" \
  bash "$root/scripts/axon-backup.sh" --yes >/dev/null
grep -qx '%u' "$work/bsd-stat-calls"
[ "$(grep -c '^%Lp$' "$work/bsd-stat-calls")" -eq 2 ]

: >"$work/calls"
CALLS="$work/calls" PATH="$work/bin:/usr/bin:/bin" QDRANT_API_KEY='secret api key' \
  bash "$root/scripts/axon-backup.sh" --collection axon --restore "$snapshot" --yes >/dev/null
grep -q '/snapshots/upload' "$work/calls"
grep -q 'api-key:' "$work/calls"

echo 'ok - backup and restore contracts passed'
