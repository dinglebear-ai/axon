#!/usr/bin/env bash
# Build a Windows .exe on dookie (no repo sync) and ship it to steamy's Desktop via scp.
set -Eeuo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/build-windows.sh [--target palette-tauri|axon] [--no-ship] [--dry-run]

Build a Windows executable on the local machine (cross-compile via MinGW) and
copy it to Steamy's Windows Desktop over SSH/SCP.

Defaults:
  --target    palette-tauri
  --host      steamy-wsl
  --desktop   /mnt/c/Users/jmaga/OneDrive/Desktop

Environment overrides:
  STEAMY_HOST     SSH alias for the Windows machine's WSL
  STEAMY_DESKTOP  Destination path on the Windows filesystem
EOF
}

log()  { printf '[%(%H:%M:%S)T] %s\n' -1 "$*"; }
die()  { printf 'ERROR: %s\n' "$*" >&2; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"; }

repo_root() {
  git rev-parse --show-toplevel 2>/dev/null || \
    { cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd; }
}

# ── defaults ──────────────────────────────────────────────────────────────────
target="palette-tauri"
ship=1
dry_run=0
host="${STEAMY_HOST:-steamy-wsl}"
desktop="${STEAMY_DESKTOP:-/mnt/c/Users/jmaga/OneDrive/Desktop}"

while (($#)); do
  case "$1" in
    --target)   [[ $# -ge 2 ]] || die '--target requires a value'; target="$2"; shift 2 ;;
    --host)     [[ $# -ge 2 ]] || die '--host requires a value';   host="$2";   shift 2 ;;
    --desktop)  [[ $# -ge 2 ]] || die '--desktop requires a value'; desktop="$2"; shift 2 ;;
    --no-ship)  ship=0; shift ;;
    --dry-run)  dry_run=1; shift ;;
    -h|--help)  usage; exit 0 ;;
    *)          die "unknown argument: $1" ;;
  esac
done

need ssh
need cargo
need x86_64-w64-mingw32-gcc
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER="${CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER:-x86_64-w64-mingw32-gcc}"

repo="$(repo_root)"

log "Target:  $target"
log "Host:    $host"
log "Desktop: $desktop"

# ── build ─────────────────────────────────────────────────────────────────────
case "$target" in
  palette-tauri)
    need pnpm
    palette="$repo/apps/palette-tauri"
    log "Installing palette frontend dependencies"
    [[ "$dry_run" -eq 0 ]] && pnpm --dir "$palette" install --frozen-lockfile
    log "Building Axon Palette Windows executable"
    if [[ "$dry_run" -eq 0 ]]; then
      pnpm --dir "$palette" exec tauri build \
        --target x86_64-pc-windows-gnu \
        --no-bundle \
        --ci
    fi
    exe="$palette/src-tauri/target/x86_64-pc-windows-gnu/release/axon-palette-tauri.exe"
    dest="Axon Palette.exe"
    ;;
  axon)
    log "Building axon.exe"
    if [[ "$dry_run" -eq 0 ]]; then
      mkdir -p "$repo/apps/web/out"
      cargo build --release --locked --bin axon \
        --manifest-path "$repo/Cargo.toml" \
        --target x86_64-pc-windows-gnu
    fi
    exe="$repo/target/x86_64-pc-windows-gnu/release/axon.exe"
    dest="axon.exe"
    ;;
  *)
    die "unknown target: $target (valid: palette-tauri, axon)"
    ;;
esac

# ── ship ──────────────────────────────────────────────────────────────────────
if [[ "$ship" -eq 1 ]]; then
  if [[ "$dry_run" -eq 1 ]]; then
    log "Dry-run: would scp '$exe' -> '$host:$desktop/$dest'"
  else
    [[ -f "$exe" ]] || die "built executable not found: $exe"
    log "Shipping $dest to $host:$desktop/"
    scp "$exe" "$host:$desktop/$dest"
    log "Done — $dest is on the Desktop"
  fi
else
  log "Skipping ship (--no-ship); exe is at: $exe"
fi
