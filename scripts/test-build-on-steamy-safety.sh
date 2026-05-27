#!/usr/bin/env bash
set -Eeuo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

fakebin="$tmp/bin"
mkdir -p "$fakebin"

cat > "$fakebin/ssh" <<'EOF'
#!/usr/bin/env bash
shift
cmd="${1:-}"
shift || true
[[ "$cmd" == "bash -s" ]] || { echo "unexpected ssh command: $cmd" >&2; exit 99; }
[[ "${1:-}" == "--" ]] && shift
bash -s -- "$@"
EOF

cat > "$fakebin/rsync" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$*" >> "${RSYNC_LOG:?}"
EOF

cat > "$fakebin/rustup" <<'EOF'
#!/usr/bin/env bash
if [[ "${1:-}" == "target" && "${2:-}" == "list" && "${3:-}" == "--installed" ]]; then
  echo x86_64-pc-windows-gnu
  exit 0
fi
exit 0
EOF

cat > "$fakebin/cargo" <<'EOF'
#!/usr/bin/env bash
mkdir -p target/x86_64-pc-windows-gnu/release
touch target/x86_64-pc-windows-gnu/release/axon.exe
EOF

cat > "$fakebin/x86_64-w64-mingw32-gcc" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF

chmod +x "$fakebin/"*

export PATH="$fakebin:$PATH"
export RSYNC_LOG="$tmp/rsync.log"
export STEAMY_HOST="fake-steamy"
export STEAMY_DESKTOP="$tmp/Desktop"

unmarked="$tmp/unmarked"
mkdir -p "$unmarked"
if "$repo_root/scripts/build-on-steamy.sh" --target axon --remote-repo "$unmarked" >"$tmp/unmarked.out" 2>"$tmp/unmarked.err"; then
  echo "expected unmarked custom remote repo to fail" >&2
  exit 1
fi
grep -q "refusing destructive rsync into unmarked target" "$tmp/unmarked.err"
[[ ! -s "$RSYNC_LOG" ]] || { echo "rsync ran for unmarked target" >&2; exit 1; }

marked="$tmp/marked"
mkdir -p "$marked"
touch "$marked/.build-on-steamy-disposable"
"$repo_root/scripts/build-on-steamy.sh" --target axon --remote-repo "$marked" >"$tmp/marked.out" 2>"$tmp/marked.err"

grep -q -- "--delete" "$RSYNC_LOG"
[[ -f "$STEAMY_DESKTOP/axon.exe" ]] || { echo "expected built axon.exe on fake desktop" >&2; exit 1; }
