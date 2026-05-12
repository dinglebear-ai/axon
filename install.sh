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
  need docker
  docker compose version >/dev/null 2>&1 || fail "docker compose is required"
  command -v nvidia-smi >/dev/null 2>&1 || fail "nvidia-smi is required for the RTX 4070 production target"
  command -v gemini >/dev/null 2>&1 || fail "gemini CLI is required and must already be authenticated"
}

download_and_verify() {
  target="$1"
  tmpdir="${AXON_INSTALL_TMPDIR:-$(mktemp -d)}"
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
}

main() {
  target="$(detect_target)"
  check_prereqs
  if [ "$DRY_RUN" = "1" ]; then
    say "Dry run OK: target=$target prefix=$PREFIX repo=$REPO version=$VERSION"
    exit 0
  fi

  download_and_verify "$target"
  trap 'rm -rf "$DOWNLOAD_TMPDIR"' EXIT HUP INT TERM
  mkdir -p "$BIN_DIR"
  install -m 0755 "$DOWNLOADED_PATH" "$BIN"
  say "Installed $BIN"

  if [ "$SKIP_SETUP" != "1" ]; then
    "$BIN" setup
  fi
}

main "$@"
