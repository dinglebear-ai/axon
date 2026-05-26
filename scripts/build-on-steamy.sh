#!/usr/bin/env bash
set -Eeuo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/build-on-steamy.sh [--target palette-tauri|axon] [--no-sync]

Build the latest local Axon checkout on steamy-wsl and place the Windows .exe
on Steamy's desktop.

Defaults:
  --target palette-tauri
  --host steamy-wsl
  --remote-repo /home/jmagar/workspace/axon_rust
  --desktop /mnt/c/Users/jmaga/Desktop

Environment overrides:
  STEAMY_HOST
  STEAMY_AXON_REPO
  STEAMY_DESKTOP
EOF
}

log() {
  printf '[%(%H:%M:%S)T] %s\n' -1 "$*"
}

die() {
  printf 'ERROR: %s\n' "$*" >&2
  exit 1
}

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"
}

repo_root() {
  if command -v git >/dev/null 2>&1 && git rev-parse --show-toplevel >/dev/null 2>&1; then
    git rev-parse --show-toplevel
  else
    cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd
  fi
}

remote_build() {
  local host="$1"
  local remote_repo="$2"
  local desktop="$3"
  local target="$4"

  ssh "$host" 'bash -s' -- "$remote_repo" "$desktop" "$target" <<'REMOTE'
set -Eeuo pipefail

remote_repo="$1"
desktop="$2"
target="$3"

log() {
  printf '[%(%H:%M:%S)T] %s\n' -1 "$*"
}

die() {
  printf 'ERROR: %s\n' "$*" >&2
  exit 1
}

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "required command not found on steamy-wsl: $1"
}

ensure_windows_target() {
  need_cmd rustup
  need_cmd cargo
  need_cmd x86_64-w64-mingw32-gcc
  if ! rustup target list --installed | grep -qx 'x86_64-pc-windows-gnu'; then
    log 'Installing Rust target x86_64-pc-windows-gnu'
    rustup target add x86_64-pc-windows-gnu
  fi
  export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER="${CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER:-x86_64-w64-mingw32-gcc}"
}

copy_exe() {
  local src="$1"
  local dest_name="$2"

  [[ -f "$src" ]] || die "built executable not found: $src"
  mkdir -p "$desktop"
  cp -f "$src" "$desktop/$dest_name"
  chmod 755 "$desktop/$dest_name" 2>/dev/null || true
  log "Copied $src -> $desktop/$dest_name"
}

cd "$remote_repo" || die "remote repo does not exist: $remote_repo"

case "$target" in
  palette-tauri)
    need_cmd pnpm
    ensure_windows_target
    cd "$remote_repo/apps/palette-tauri" || die 'apps/palette-tauri is missing'
    log 'Installing palette frontend dependencies'
    pnpm install --frozen-lockfile
    log 'Building palette frontend assets'
    pnpm vite:build
    log 'Building Axon Palette Windows executable'
    cargo build --release --locked --manifest-path src-tauri/Cargo.toml --target x86_64-pc-windows-gnu
    copy_exe \
      "$remote_repo/apps/palette-tauri/src-tauri/target/x86_64-pc-windows-gnu/release/axon-palette-tauri.exe" \
      'Axon Palette.exe'
    ;;
  axon)
    ensure_windows_target
    mkdir -p "$remote_repo/apps/web/out"
    log 'Building axon.exe'
    cargo build --release --locked --bin axon --target x86_64-pc-windows-gnu
    copy_exe \
      "$remote_repo/target/x86_64-pc-windows-gnu/release/axon.exe" \
      'axon.exe'
    ;;
  *)
    die "unknown target: $target"
    ;;
esac
REMOTE
}

target="palette-tauri"
sync_tree=1
host="${STEAMY_HOST:-steamy-wsl}"
remote_repo="${STEAMY_AXON_REPO:-/home/jmagar/workspace/axon_rust}"
desktop="${STEAMY_DESKTOP:-/mnt/c/Users/jmaga/Desktop}"

while (($#)); do
  case "$1" in
    --target)
      [[ $# -ge 2 ]] || die '--target requires a value'
      target="$2"
      shift 2
      ;;
    --host)
      [[ $# -ge 2 ]] || die '--host requires a value'
      host="$2"
      shift 2
      ;;
    --remote-repo)
      [[ $# -ge 2 ]] || die '--remote-repo requires a value'
      remote_repo="$2"
      shift 2
      ;;
    --desktop)
      [[ $# -ge 2 ]] || die '--desktop requires a value'
      desktop="$2"
      shift 2
      ;;
    --no-sync)
      sync_tree=0
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      die "unknown argument: $1"
      ;;
  esac
done

case "$target" in
  palette-tauri|axon) ;;
  *) die "unknown target: $target" ;;
esac

need_cmd ssh
repo="$(repo_root)"

log "Target: $target"
log "Host: $host"
log "Remote repo: $remote_repo"
log "Desktop: $desktop"

if [[ "$sync_tree" -eq 1 ]]; then
  need_cmd rsync
  log 'Syncing current checkout to Steamy'
  ssh "$host" "mkdir -p '$remote_repo'"
  rsync -az --delete --delete-excluded \
    --exclude '.git/' \
    --exclude '.cache/' \
    --exclude '.beads/' \
    --exclude '.worktree/' \
    --exclude '.worktrees/' \
    --exclude 'target/' \
    --exclude 'apps/**/target/' \
    --exclude 'apps/**/node_modules/' \
    --exclude 'apps/**/dist/' \
    --exclude 'logs/' \
    --exclude 'storage/' \
    --exclude '.env' \
    "$repo/" "$host:$remote_repo/"
else
  log 'Skipping sync; building existing remote tree'
fi

remote_build "$host" "$remote_repo" "$desktop" "$target"
log 'Done'
