#!/usr/bin/env bash
set -Eeuo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/build-on-steamy.sh [--target palette-tauri|axon] [--no-sync] [--dry-run] [--destructive-sync]

Build the latest local Axon checkout on steamy-wsl and place the Windows .exe
on Steamy's desktop.

Defaults:
  --target palette-tauri
  --host steamy-wsl
  --remote-repo /home/jmagar/.cache/axon/build-on-steamy/axon_rust
  --desktop /mnt/c/Users/jmaga/Desktop

Sync safety:
  The default remote repo is a disposable mirror marked with:
    .build-on-steamy-disposable

  rsync uses --delete --delete-excluded only when the remote target has that
  marker, or when --destructive-sync is passed explicitly. Use --dry-run to
  preview the rsync change/deletion set without building.

Environment overrides:
  STEAMY_HOST
  STEAMY_AXON_REPO (custom paths must already contain the marker unless
                    --destructive-sync is passed)
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
    cargo build --release --manifest-path src-tauri/Cargo.toml --target x86_64-pc-windows-gnu
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

prepare_remote_sync_target() {
  local host="$1"
  local remote_repo="$2"
  local default_remote_repo="$3"
  local destructive_sync="$4"
  local dry_run="$5"

  ssh "$host" 'bash -s' -- "$remote_repo" "$default_remote_repo" "$destructive_sync" "$dry_run" <<'REMOTE'
set -Eeuo pipefail

remote_repo="$1"
default_remote_repo="$2"
destructive_sync="$3"
dry_run="$4"
sentinel='.build-on-steamy-disposable'

die() {
  printf 'ERROR: %s\n' "$*" >&2
  exit 1
}

if [[ "$dry_run" == 1 ]]; then
  exit 0
fi

mkdir -p "$remote_repo"

if [[ "$remote_repo" == "$default_remote_repo" ]]; then
  printf 'disposable mirror for scripts/build-on-steamy.sh\n' > "$remote_repo/$sentinel"
fi

if [[ "$destructive_sync" != 1 && ! -f "$remote_repo/$sentinel" ]]; then
  die "refusing destructive rsync into unmarked target: $remote_repo
Create $remote_repo/$sentinel if this is a disposable mirror, or pass --destructive-sync to opt into deleting remote-only files."
fi
REMOTE
}

target="palette-tauri"
sync_tree=1
dry_run=0
destructive_sync=0
host="${STEAMY_HOST:-steamy-wsl}"
default_remote_repo="/home/jmagar/.cache/axon/build-on-steamy/axon_rust"
remote_repo="${STEAMY_AXON_REPO:-$default_remote_repo}"
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
    --dry-run)
      dry_run=1
      shift
      ;;
    --destructive-sync)
      destructive_sync=1
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
  prepare_remote_sync_target "$host" "$remote_repo" "$default_remote_repo" "$destructive_sync" "$dry_run"

  rsync_args=(
    -az
    --delete
    --delete-excluded
    --filter 'P .build-on-steamy-disposable'
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
  )

  if [[ "$dry_run" -eq 1 ]]; then
    log 'Dry-run: previewing sync changes/deletions; build will be skipped'
    rsync_args=(-n --itemize-changes "${rsync_args[@]}")
  else
    log 'Syncing current checkout to Steamy disposable mirror'
  fi

  rsync "${rsync_args[@]}"
elif [[ "$dry_run" -eq 1 ]]; then
  log 'Dry-run requested with --no-sync; build will be skipped'
fi

if [[ "$dry_run" -eq 1 ]]; then
  log 'Dry-run complete'
  exit 0
elif [[ "$sync_tree" -eq 0 ]]; then
  log 'Skipping sync; building existing remote tree'
fi

remote_build "$host" "$remote_repo" "$desktop" "$target"
log 'Done'
