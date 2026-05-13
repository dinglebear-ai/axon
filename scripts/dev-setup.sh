#!/usr/bin/env bash
# dev-setup.sh — Bootstrap a new development environment for axon.
# Idempotent: safe to re-run. Checks before installing.
# Usage: ./scripts/dev-setup.sh [--build] [--no-docker]
set -euo pipefail

if command -v realpath >/dev/null 2>&1; then
  SCRIPT_DIR="$(cd -- "$(dirname -- "$(realpath "${BASH_SOURCE[0]}")")" && pwd -P)"
else
  SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
fi
REPO="$(cd -- "$SCRIPT_DIR/.." && pwd -P)"

# ── Flags ──────────────────────────────────────────────────────────────────────
BUILD=false
NO_DOCKER=false
for arg in "$@"; do
  case "$arg" in
    --build)     BUILD=true ;;
    --no-docker) NO_DOCKER=true ;;
    --help|-h)
      echo "Usage: $0 [--build] [--no-docker]"
      echo "  --build      Also compile the release binary after setup"
      echo "  --no-docker  Skip starting Docker infrastructure"
      exit 0 ;;
    *) echo "[dev-setup] Unknown flag: $arg" >&2; exit 1 ;;
  esac
done

# ── Helpers ────────────────────────────────────────────────────────────────────
info()  { echo "[dev-setup] $*"; }
warn()  { echo "[dev-setup] WARN: $*" >&2; }
die()   { printf "[dev-setup] ERROR: %b\n" "$*" >&2; exit 1; }
ok()    { echo "[dev-setup] OK: $*"; }
sep()   { echo "[dev-setup] ────────────────────────────────────────"; }

# ── OS Detection ───────────────────────────────────────────────────────────────
sep
info "Detecting OS..."
case "$(uname -s)" in
  Linux*)  OS=linux ;;
  Darwin*) OS=macos ;;
  *)       die "Unsupported OS: $(uname -s). Only Linux and macOS are supported." ;;
esac
ok "OS: $OS"

# Portable in-place sed: macOS requires -i '' whereas GNU sed uses -i alone.
if [[ "$OS" == "macos" ]]; then
  sed_i() { sed -i '' "$@"; }
else
  sed_i() { sed -i "$@"; }
fi

pkg_install() {
  # pkg_install <pkg...>
  if [[ "$OS" == "macos" ]]; then
    brew install "$@"
  else
    sudo apt-get install -y "$@"
  fi
}

need() {
  # need <tool> [pkg-name-override]
  local tool="$1" pkg="${2:-$1}"
  if command -v "$tool" >/dev/null 2>&1; then
    ok "$tool found: $(command -v "$tool")"
  else
    info "Installing $tool..."
    pkg_install "$pkg"
    ok "$tool installed"
  fi
}

# ── System Prerequisites ───────────────────────────────────────────────────────
sep
info "Checking system prerequisites..."

if [[ "$OS" == "macos" ]] && ! command -v brew >/dev/null 2>&1; then
  die "Homebrew is required on macOS. Install from https://brew.sh then re-run."
fi

need curl
need git
need python3
need jq

# ── Docker ─────────────────────────────────────────────────────────────────────
if [[ "$NO_DOCKER" == true ]]; then
  sep
  info "Skipping Docker checks (--no-docker set)"
else
  sep
  info "Checking Docker..."
  if ! command -v docker >/dev/null 2>&1; then
    die "Docker is not installed. Install Docker Desktop (https://docs.docker.com/get-docker/) then re-run."
  fi
  if ! docker info >/dev/null 2>&1; then
    die "Docker daemon is not running (or WSL integration is not enabled).\nStart Docker Desktop and ensure WSL integration is on, then re-run."
  fi
  if ! docker compose version >/dev/null 2>&1; then
    die "docker compose (v2 plugin) not found. Update Docker Desktop or install the compose plugin."
  fi
  ok "Docker $(docker --version | awk '{print $3}' | tr -d ',')"
  ok "Docker Compose $(docker compose version --short)"
fi

# ── Rust Toolchain ─────────────────────────────────────────────────────────────
sep
info "Checking Rust toolchain..."
REQUIRED_RUST="$(grep '^channel' "$REPO/rust-toolchain.toml" | sed 's/.*"\(.*\)"/\1/')"

if ! command -v rustup >/dev/null 2>&1; then
  info "rustup not found — installing..."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --no-modify-path --default-toolchain "$REQUIRED_RUST"
  # shellcheck source=/dev/null
  source "$HOME/.cargo/env"
  ok "rustup installed"
else
  ok "rustup found"
  # shellcheck source=/dev/null
  [[ -f "$HOME/.cargo/env" ]] && source "$HOME/.cargo/env"
fi

if ! rustup toolchain list | grep -q "$REQUIRED_RUST"; then
  info "Installing Rust $REQUIRED_RUST..."
  rustup toolchain install "$REQUIRED_RUST" --component rustfmt clippy
fi

ACTIVE_RUST="$(rustc --version 2>/dev/null | awk '{print $2}' || echo "none")"
ok "Rust $ACTIVE_RUST (required: $REQUIRED_RUST)"

rustup component add rustfmt clippy --toolchain "$REQUIRED_RUST" 2>/dev/null || true

# ── Optional Rust Dev Tools ────────────────────────────────────────────────────
sep
info "Checking optional Rust dev tools..."

cargo_install_if_missing() {
  local bin="$1" crate="${2:-$1}"
  if command -v "$bin" >/dev/null 2>&1; then
    ok "$bin already installed"
  else
    info "Installing $crate (optional)..."
    cargo install --locked "$crate" && ok "$crate installed" || warn "$crate install failed — skipping"
  fi
}

# Required for Justfile targets.
# NOTE: if just is not yet installed, run ./scripts/dev-setup.sh directly —
# `just setup` is a convenience alias for when just is already available.
if ! command -v just >/dev/null 2>&1; then
  info "Installing just (task runner)..."
  if [[ "$OS" == "macos" ]]; then
    brew install just && ok "just $(just --version)"
  else
    # Prebuilt binary is much faster than cargo install, but must be resilient to failures.
    _just_ver=""
    if command -v python3 >/dev/null 2>&1; then
      if ! _just_ver="$(curl -fsSL https://api.github.com/repos/casey/just/releases/latest \
        | python3 -c "import sys,json; print(json.load(sys.stdin)['tag_name'])" 2>/dev/null)"; then
        _just_ver=""
      fi
    fi

    if [[ -n "${_just_ver:-}" ]]; then
      _arch="$(uname -m)"
      _just_triple=""
      case "$_arch" in
        x86_64)  _just_triple="x86_64-unknown-linux-musl" ;;
        aarch64) _just_triple="aarch64-unknown-linux-musl" ;;
      esac

      _installed_prebuilt=false
      if [[ -n "$_just_triple" ]]; then
        mkdir -p "$HOME/.cargo/bin"
        if curl -fsSL "https://github.com/casey/just/releases/download/${_just_ver}/just-${_just_ver}-${_just_triple}.tar.gz" \
          | tar -xz -C "$HOME/.cargo/bin" just 2>/dev/null \
          && "$HOME/.cargo/bin/just" --version >/dev/null 2>&1; then
          ok "just ${_just_ver} installed"
          _installed_prebuilt=true
        else
          warn "prebuilt install failed or binary did not run — removing and falling back to cargo install"
          rm -f "$HOME/.cargo/bin/just"
        fi
      else
        warn "no prebuilt just artifact for arch '${_arch}' — falling back to cargo install"
      fi

      if [[ "$_installed_prebuilt" == false ]]; then
        cargo install --locked just && ok "just installed"
      fi
    else
      warn "failed to detect latest just release — falling back to cargo install just"
      cargo install --locked just && ok "just installed"
    fi
  fi
else
  ok "just $(just --version)"
fi

# Required for pre-commit hooks
if ! command -v lefthook >/dev/null 2>&1; then
  info "Installing lefthook (git hooks)..."
  cargo install --locked lefthook && ok "lefthook installed" \
    || warn "lefthook install failed — install manually: cargo install lefthook"
else
  ok "lefthook $(lefthook version 2>/dev/null || echo 'found')"
fi

cargo_install_if_missing cargo-nextest
cargo_install_if_missing cargo-watch
cargo_install_if_missing sccache

if command -v mold >/dev/null 2>&1; then
  ok "mold $(mold --version | head -1)"
else
  warn "mold not found (optional fast linker) — install via: sudo apt-get install mold  OR  brew install mold"
fi

# ── Node.js + npm ──────────────────────────────────────────────────────────────
sep
info "Checking Node.js..."
REQUIRED_NODE_MAJOR=24

if command -v node >/dev/null 2>&1; then
  NODE_MAJOR="$(node --version | tr -d 'v' | cut -d. -f1)"
  if (( NODE_MAJOR >= REQUIRED_NODE_MAJOR )); then
    ok "Node.js $(node --version)"
  else
    warn "Node.js $(node --version) is below required v${REQUIRED_NODE_MAJOR}.x"
    if [[ "$OS" == "macos" ]]; then
      info "Run: brew install node@${REQUIRED_NODE_MAJOR} && brew link node@${REQUIRED_NODE_MAJOR} --force --overwrite"
    else
      info "Install via nvm: nvm install ${REQUIRED_NODE_MAJOR} && nvm use ${REQUIRED_NODE_MAJOR}"
      info "Or via NodeSource: curl -fsSL https://deb.nodesource.com/setup_${REQUIRED_NODE_MAJOR}.x | sudo -E bash - && sudo apt-get install -y nodejs"
    fi
    warn "Continuing — npm ci may fail if Node version is incompatible."
  fi
else
  if [[ "$OS" == "macos" ]]; then
    info "Installing Node.js ${REQUIRED_NODE_MAJOR} via brew..."
    brew install "node@${REQUIRED_NODE_MAJOR}" && \
      brew link "node@${REQUIRED_NODE_MAJOR}" --force --overwrite && \
      ok "Node.js installed"
  else
    die "Node.js not found. Install via nvm (recommended):\n  curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/HEAD/install.sh | bash\n  source ~/.nvm/nvm.sh && nvm install ${REQUIRED_NODE_MAJOR}"
  fi
fi

info "Checking npm..."
command -v npm >/dev/null 2>&1 || die "npm not found; install Node.js ${REQUIRED_NODE_MAJOR}+"
ok "npm $(npm --version)"

# ── Web App Dependencies ───────────────────────────────────────────────────────
sep
info "Installing web app dependencies..."
(cd "$REPO/apps/web" && npm ci)
ok "apps/web dependencies installed"

# ── Environment File ───────────────────────────────────────────────────────────
sep
AXON_HOME="${AXON_HOME:-$HOME/.axon}"
ENV_FILE="$AXON_HOME/.env"
info "Checking ~/.axon/.env..."
mkdir -p "$AXON_HOME"
chmod 700 "$AXON_HOME" 2>/dev/null || true

# Helpers — defined here so they're available for both fresh installs and reruns.
gen_secret() { python3 -c "import secrets; print(secrets.token_urlsafe(32))"; }

set_env() {
  # Replace existing key or append — always writes the supplied value.
  local key="$1" val="$2"
  if grep -q "^${key}=" "$ENV_FILE"; then
    sed_i "s|^${key}=.*|${key}=${val}|" "$ENV_FILE"
  else
    echo "${key}=${val}" >> "$ENV_FILE"
  fi
}

set_env_if_missing() {
  # Only write if the key is absent or has an empty value.
  local key="$1" val="$2"
  grep -q "^${key}=." "$ENV_FILE" 2>/dev/null || set_env "$key" "$val"
}

if [[ -f "$ENV_FILE" ]]; then
  ok "$ENV_FILE already exists — backfilling any missing entries"
else
  cp "$REPO/.env.example" "$ENV_FILE"
  chmod 600 "$ENV_FILE"
  CREATED_INITIAL_ENV=true
  ok "$ENV_FILE created from .env.example"
fi
CREATED_INITIAL_ENV="${CREATED_INITIAL_ENV:-false}"

# ── AXON_HOME / AXON_DATA_DIR — keep Compose and CLI aligned ──────────────────
SOURCE_ENV_FILE="$ENV_FILE"
EXISTING_AXON_HOME="$(grep '^AXON_HOME=' "$ENV_FILE" 2>/dev/null | cut -d= -f2- | tr -d "'\"")"
if [[ -n "$EXISTING_AXON_HOME" && "$AXON_HOME" == "$HOME/.axon" ]]; then
  AXON_HOME="${EXISTING_AXON_HOME/#\~/$HOME}"
  ENV_FILE="$AXON_HOME/.env"
  mkdir -p "$AXON_HOME"
  chmod 700 "$AXON_HOME" 2>/dev/null || true
  if [[ ! -f "$ENV_FILE" && -f "$SOURCE_ENV_FILE" ]]; then
    cp "$SOURCE_ENV_FILE" "$ENV_FILE"
    chmod 600 "$ENV_FILE"
    ok "Migrated existing env to ${ENV_FILE}"
  fi
fi

if [[ -z "$AXON_HOME" || "$AXON_HOME" == *CHANGE_ME* ]]; then
  AXON_HOME="$HOME/.axon"
fi

if [[ "$AXON_HOME" == "$HOME/.axon" ]]; then
  if [[ -t 0 ]]; then
    echo ""
    echo "  AXON_HOME — root directory for all persistent Axon data"
    echo "  Press Enter to accept the default."
    read -r -p "  AXON_HOME [${AXON_HOME}]: " USER_AXON_HOME
    AXON_HOME="${USER_AXON_HOME:-$AXON_HOME}"
  else
    info "Non-interactive mode: AXON_HOME=${AXON_HOME}"
  fi
fi
AXON_HOME="${AXON_HOME/#\~/$HOME}"
AXON_DATA_DIR="$AXON_HOME"
ENV_FILE="$AXON_HOME/.env"
mkdir -p "$AXON_HOME"
chmod 700 "$AXON_HOME" 2>/dev/null || true
if [[ ! -f "$ENV_FILE" && "$CREATED_INITIAL_ENV" == "true" && "$SOURCE_ENV_FILE" != "$ENV_FILE" && -f "$SOURCE_ENV_FILE" ]]; then
  mv "$SOURCE_ENV_FILE" "$ENV_FILE"
  rmdir "$(dirname "$SOURCE_ENV_FILE")" 2>/dev/null || true
  ok "Moved initial env to ${ENV_FILE}"
fi
if [[ ! -f "$ENV_FILE" ]]; then
  cp "$REPO/.env.example" "$ENV_FILE"
  chmod 600 "$ENV_FILE"
fi
set_env AXON_HOME "$AXON_HOME"
set_env AXON_DATA_DIR "$AXON_DATA_DIR"
ok "AXON_HOME=${AXON_HOME}"
ok "AXON_DATA_DIR=${AXON_DATA_DIR}"

# Always ensure volume-mount directories exist (idempotent).
info "Ensuring data directories under ${AXON_DATA_DIR}..."
mkdir -p \
  "${AXON_DATA_DIR}/qdrant" \
  "${AXON_DATA_DIR}/tei" \
  "${AXON_DATA_DIR}/output" \
  "${AXON_DATA_DIR}/artifacts" \
  "${AXON_DATA_DIR}/logs" \
  "${AXON_DATA_DIR}/screenshots" \
  "${AXON_DATA_DIR}/chrome-diagnostics" \
  "${AXON_DATA_DIR}/lab-auth"
ok "Data directories ready"

# ── Secrets — backfill only; never overwrite existing values ──────────────────
info "Checking secrets..."
WEB_TOKEN="$(grep '^AXON_WEB_API_TOKEN=' "$ENV_FILE" 2>/dev/null | cut -d= -f2-)"
if [[ -z "$WEB_TOKEN" ]]; then
  WEB_TOKEN="$(gen_secret)"
  set_env AXON_WEB_API_TOKEN         "$WEB_TOKEN"
  set_env NEXT_PUBLIC_AXON_API_TOKEN "$WEB_TOKEN"
fi
set_env_if_missing AXON_MCP_HTTP_TOKEN "$(gen_secret)"
ok "Secrets verified"

# ── Test infrastructure URLs — static values, backfill if absent ──────────────
set_env_if_missing AXON_TEST_QDRANT_URL "http://127.0.0.1:53335"
ok "Test service URLs verified"

CHANGE_ME_COUNT="$(grep -c 'CHANGE_ME' "$ENV_FILE" || true)"
if (( CHANGE_ME_COUNT > 0 )); then
  warn "$CHANGE_ME_COUNT value(s) in $ENV_FILE still need manual configuration:"
  grep -n 'CHANGE_ME' "$ENV_FILE" | sed 's/^/  /' >&2
fi

# ── Docker Infrastructure ──────────────────────────────────────────────────────
if [[ "$NO_DOCKER" == "false" ]]; then
  sep
  info "Starting Docker infrastructure..."
  (cd "$REPO" && just services-up)

  ok "Infrastructure containers:"
  (cd "$REPO" && docker compose --env-file "$ENV_FILE" -f docker-compose.yaml ps --format "  {{.Name}}: {{.Status}}" \
    axon-qdrant axon-tei axon-chrome 2>/dev/null \
    || docker compose --env-file "$ENV_FILE" -f docker-compose.yaml ps axon-qdrant axon-tei axon-chrome)
fi

# ── Git Hooks ──────────────────────────────────────────────────────────────────
sep
info "Installing git hooks..."
if command -v lefthook >/dev/null 2>&1; then
  "$REPO/scripts/install-git-hooks.sh"
else
  warn "lefthook not found — skipping git hooks. Install lefthook and run: ./scripts/install-git-hooks.sh"
fi

# ── Optional: Build Rust Binary ────────────────────────────────────────────────
if [[ "$BUILD" == "true" ]]; then
  sep
  info "Building release binary..."
  (cd "$REPO" && cargo build --release --locked --bin axon)
  ok "Binary: $REPO/target/release/axon"
fi

# ── Summary ────────────────────────────────────────────────────────────────────
sep
info "Done! Next steps:"
echo ""
echo "  1. Edit $ENV_FILE and fill in any remaining CHANGE_ME values"
echo "     Required for full functionality:"
echo "       TEI_URL           — text embedding service"
echo "       OPENAI_BASE_URL   — LLM endpoint (for ask/extract)"
echo "       OPENAI_API_KEY    — LLM API key"
echo "       OPENAI_MODEL      — LLM model name"
echo "       TAVILY_API_KEY    — for search/research commands"
echo ""
echo "  2. Run workers (each in its own terminal):"
echo "       cargo run --bin axon -- crawl worker"
echo "       cargo run --bin axon -- embed worker"
echo "       cargo run --bin axon -- extract worker"
echo "     Or all at once:  just workers"
echo ""
echo "  3. Run the web UI:"
echo "       cd apps/web && npm run dev    # http://localhost:49010"
echo "     Or full dev stack:  just dev"
echo ""
echo "  4. Verify services:  ./scripts/axon doctor"
echo "  5. Run checks:       just verify"
echo "  6. Infrastructure:   just services-up     # start"
echo "                       just services-down   # stop"
echo ""
