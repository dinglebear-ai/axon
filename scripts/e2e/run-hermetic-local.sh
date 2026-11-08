#!/usr/bin/env bash
set -euo pipefail

# Permission-denial fixtures must run as the invoking user, including inside
# the Linux network namespace. Root is needed only to configure that namespace.
if [[ "$(id -u)" == "0" ]]; then
  echo "Run the hermetic launcher as a non-root user; it escalates only for network isolation. Root bypasses the read-only upgrade fixture." >&2
  exit 2
fi
invoking_user="$(id -un)"

# Native macOS development may use Homebrew OpenSSL, pkg-config discovery, or
# the vendored openssl-src fallback selected by Cargo. Never depend on a
# machine-specific /tmp prefix.
if [[ -n "${OPENSSL_DIR:-}" ]]; then
  if [[ ! -f "$OPENSSL_DIR/include/openssl/ssl.h" || ! -d "$OPENSSL_DIR/lib" ]]; then
    echo "OPENSSL_DIR does not contain a complete OpenSSL development prefix: $OPENSSL_DIR" >&2
    exit 2
  fi
elif [[ "$(uname -s)" == "Darwin" ]] && command -v brew >/dev/null 2>&1; then
  openssl_prefix="$(brew --prefix openssl@3 2>/dev/null || true)"
  if [[ -n "$openssl_prefix" && -d "$openssl_prefix" ]]; then
    export OPENSSL_DIR="$openssl_prefix"
    export PKG_CONFIG_PATH="$openssl_prefix/lib/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
  fi
fi

if [[ "$(uname -s)" == "Darwin" && -z "${OPENSSL_DIR:-}" ]] && ! command -v pkg-config >/dev/null 2>&1; then
  echo "OpenSSL development files were not discovered. Install openssl@3 and pkgconf, or set OPENSSL_DIR to a complete trusted prefix." >&2
  exit 2
fi

cargo build --locked --bin axon
if [[ "$(uname -s)" == "Darwin" ]]; then
  AXON_E2E_NATIVE_ISOLATION=1 sandbox-exec -f scripts/e2e/hermetic.sb just e2e-hermetic-inner
else
  sudo --preserve-env=PATH unshare --net --fork bash -c \
    'set -euo pipefail; ip link set lo up; exec runuser -u "$1" -- env PATH="$PATH" AXON_E2E_NATIVE_ISOLATION=1 just e2e-hermetic-inner' \
    axon-hermetic-namespace "$invoking_user"
fi
