#!/usr/bin/env sh
set -eu

REPO="${AXON_INSTALL_REPO:-jmagar/axon}"
VERSION="${AXON_VERSION:-latest}"
PREFIX="${AXON_INSTALL_PREFIX:-$HOME/.local}"
BIN_DIR="$PREFIX/bin"
BIN="$BIN_DIR/axon"
DRY_RUN="${AXON_INSTALL_DRY_RUN:-0}"
SKIP_SETUP="${AXON_INSTALL_SKIP_SETUP:-0}"

say() {
  printf '%s\n' "$*" >&2
}

fail() {
  say "axon install: $*"
  exit 1
}

need() {
  command -v "$1" >/dev/null 2>&1 || fail "$1 is required"
}

detect_target() {
  os="$(uname -s | tr '[:upper:]' '[:lower:]')"
  arch="$(uname -m)"
  case "$os:$arch" in
    linux:x86_64|linux:amd64) printf 'x86_64-unknown-linux-gnu' ;;
    *) fail "unsupported platform $os/$arch" ;;
  esac
}

asset_base_url() {
  target="$1"
  if [ "$VERSION" = "latest" ]; then
    printf 'https://github.com/%s/releases/latest/download/axon-%s' "$REPO" "$target"
  else
    printf 'https://github.com/%s/releases/download/%s/axon-%s' "$REPO" "$VERSION" "$target"
  fi
}

check_prereqs() {
  need curl
  need sha256sum
  need install
}

check_setup_prereqs() {
  need docker
  docker compose version >/dev/null 2>&1 || fail "docker compose is required"
  command -v nvidia-smi >/dev/null 2>&1 || fail "nvidia-smi is required for the RTX 4070 production target"
  command -v gemini >/dev/null 2>&1 || fail "gemini CLI is required on PATH; axon setup ask-smoke verifies auth and completion"
  say "Gemini CLI found; axon setup ask-smoke verifies auth and completion"
}

download_and_verify() {
  target="$1"
  CREATED_TMPDIR=0
  if [ "${AXON_INSTALL_TMPDIR:-}" ]; then
    tmpdir="$AXON_INSTALL_TMPDIR"
    case "$tmpdir" in
      ""|"/") fail "unsafe AXON_INSTALL_TMPDIR: $tmpdir" ;;
    esac
    [ -d "$tmpdir" ] || fail "AXON_INSTALL_TMPDIR must be an existing directory"
    tmp_owner="$(ls -nd "$tmpdir" | awk '{print $3}')"
    [ "$tmp_owner" = "$(id -u)" ] || fail "AXON_INSTALL_TMPDIR must be owned by the current user"
  else
    tmpdir="$(mktemp -d)"
    CREATED_TMPDIR=1
  fi
  bin_url="${AXON_INSTALL_BIN_URL:-$(asset_base_url "$target")}"
  sha_url="${AXON_INSTALL_SHA256_URL:-$bin_url.sha256}"
  archive="$tmpdir/axon"
  checksum="$tmpdir/axon.sha256"

  say "Downloading $bin_url"
  curl -fsSL "$bin_url" -o "$archive"
  say "Downloading $sha_url"
  curl -fsSL "$sha_url" -o "$checksum"

  expected="$(awk '{print $1; exit}' "$checksum")"
  [ -n "$expected" ] || fail "checksum file is empty"
  actual="$(sha256sum "$archive" | awk '{print $1}')"
  [ "$expected" = "$actual" ] || fail "checksum mismatch for downloaded axon binary"
  chmod +x "$archive"
  DOWNLOADED_PATH="$archive"
  DOWNLOAD_TMPDIR="$tmpdir"
  DOWNLOAD_TMPDIR_CREATED="$CREATED_TMPDIR"
}

cleanup_download() {
  if [ "${DOWNLOAD_TMPDIR_CREATED:-0}" = "1" ] && [ -n "${DOWNLOAD_TMPDIR:-}" ]; then
    rm -rf "$DOWNLOAD_TMPDIR"
  fi
}

main() {
  target="$(detect_target)"
  check_prereqs
  if [ "$DRY_RUN" = "1" ]; then
    say "Dry run OK: target=$target prefix=$PREFIX repo=$REPO version=$VERSION"
    exit 0
  fi
  if [ "$SKIP_SETUP" != "1" ]; then
    check_setup_prereqs
  fi

  download_and_verify "$target"
  trap cleanup_download EXIT HUP INT TERM
  mkdir -p "$BIN_DIR"
  install -m 0755 "$DOWNLOADED_PATH" "$BIN"
  say "Installed $BIN"

  if [ "$SKIP_SETUP" != "1" ]; then
    "$BIN" setup
  fi
}

main "$@"
