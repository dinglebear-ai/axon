#!/usr/bin/env sh
# install.sh — thin bootstrap: acquire the axon binary, then hand off to `axon setup`.
# All prerequisite checks (docker, nvidia-smi, gemini) happen inside `axon setup preflight`.
set -eu

REPO="${AXON_INSTALL_REPO:-jmagar/axon}"
VERSION="${AXON_VERSION:-latest}"
PREFIX="${AXON_INSTALL_PREFIX:-$HOME/.local}"
BIN_DIR="$PREFIX/bin"
BIN="$BIN_DIR/axon"
DRY_RUN="${AXON_INSTALL_DRY_RUN:-0}"
SKIP_SETUP="${AXON_INSTALL_SKIP_SETUP:-0}"
# METHOD: pull (download GitHub release tarball) or build (cargo build --release).
METHOD="${AXON_INSTALL_METHOD:-pull}"

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

# Asset naming MUST match .github/workflows/release.yml, which packages the
# linux build as `axon-linux-x86_64.tar.gz` (+ `.sha256`).
detect_target() {
  os="$(uname -s | tr '[:upper:]' '[:lower:]')"
  arch="$(uname -m)"
  case "$os:$arch" in
    linux:x86_64|linux:amd64) printf 'linux-x86_64' ;;
    *) fail "unsupported platform $os/$arch" ;;
  esac
}

asset_base_url() {
  target="$1"
  if [ "$VERSION" = "latest" ]; then
    printf 'https://github.com/%s/releases/latest/download/axon-%s.tar.gz' "$REPO" "$target"
  else
    printf 'https://github.com/%s/releases/download/%s/axon-%s.tar.gz' "$REPO" "$VERSION" "$target"
  fi
}

check_prereqs() {
  need curl
  need sha256sum
  need install
  need tar
}

build_from_source() {
  need cargo
  say "Building axon from source (cargo build --release)..."
  cargo build --release --bin axon
  BUILT_PATH="$(pwd)/target/release/axon"
  [ -f "$BUILT_PATH" ] || fail "cargo build succeeded but axon binary not found at $BUILT_PATH"
  DOWNLOADED_PATH="$BUILT_PATH"
  DOWNLOAD_TMPDIR=""
  DOWNLOAD_TMPDIR_CREATED=0
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
  archive="$tmpdir/axon.tar.gz"
  checksum="$tmpdir/axon.tar.gz.sha256"

  say "Downloading $bin_url"
  curl -fsSL "$bin_url" -o "$archive"
  say "Downloading $sha_url"
  curl -fsSL "$sha_url" -o "$checksum"

  expected="$(awk '{print $1; exit}' "$checksum")"
  [ -n "$expected" ] || fail "checksum file is empty"
  actual="$(sha256sum "$archive" | awk '{print $1}')"
  [ "$expected" = "$actual" ] || fail "checksum mismatch for downloaded axon archive"

  # Extract the `axon` binary from the release tarball (release.yml tars a single
  # `axon` member at the archive root).
  tar -xzf "$archive" -C "$tmpdir" axon || fail "failed to extract axon from archive"
  [ -f "$tmpdir/axon" ] || fail "release archive did not contain an axon binary"
  chmod +x "$tmpdir/axon"
  DOWNLOADED_PATH="$tmpdir/axon"
  DOWNLOAD_TMPDIR="$tmpdir"
  DOWNLOAD_TMPDIR_CREATED="$CREATED_TMPDIR"
}

cleanup_download() {
  if [ "${DOWNLOAD_TMPDIR_CREATED:-0}" = "1" ] && [ -n "${DOWNLOAD_TMPDIR:-}" ]; then
    rm -rf "$DOWNLOAD_TMPDIR"
  fi
}

main() {
  if [ "$DRY_RUN" = "1" ]; then
    target="$(detect_target)"
    say "Dry run OK: target=$target prefix=$PREFIX repo=$REPO version=$VERSION method=$METHOD"
    exit 0
  fi

  case "$METHOD" in
    pull)
      target="$(detect_target)"
      check_prereqs
      download_and_verify "$target"
      trap cleanup_download EXIT HUP INT TERM
      ;;
    build)
      build_from_source
      ;;
    *)
      fail "unknown METHOD '$METHOD'; expected pull or build"
      ;;
  esac

  mkdir -p "$BIN_DIR"
  install -m 0755 "$DOWNLOADED_PATH" "$BIN"
  say "Installed $BIN"

  if [ "$SKIP_SETUP" != "1" ]; then
    # Hand off to axon setup wizard. All prerequisite checks (docker, nvidia-smi,
    # gemini CLI) happen inside `axon setup preflight` rather than in this script.
    "$BIN" setup --method "$METHOD"
  fi
}

main "$@"
