set shell := ["bash", "-euo", "pipefail", "-c"]
rust_dev_env := "export SCCACHE_SERVER_UDS=${SCCACHE_SERVER_UDS:-/tmp/sccache-${USER:-$(id -u)}.sock}; export SCCACHE_LOG=${SCCACHE_LOG:-error}; if [ -n \"${HOME:-}\" ] && [ -x \"${HOME}/.local/bin/sccache-wrapper\" ]; then export RUSTC_WRAPPER=\"${HOME}/.local/bin/sccache-wrapper\"; elif command -v sccache >/dev/null 2>&1; then export RUSTC_WRAPPER=sccache; fi; if command -v mold >/dev/null 2>&1; then export RUSTFLAGS=\"${RUSTFLAGS:-} -C link-arg=-fuse-ld=mold\"; fi"

default:
    @just --list

# Bootstrap a new development environment (checks + installs all dependencies).
# No just? Run ./scripts/dev-setup.sh directly — it installs just for you.
setup *args:
    ./scripts/dev-setup.sh {{args}}

check:
    {{rust_dev_env}}; cargo check -q --locked

check-tests:
    {{rust_dev_env}}; cargo check -q --tests --locked

test:
    if cargo nextest --version >/dev/null 2>&1; then {{rust_dev_env}}; cargo nextest run --locked --workspace -E 'not test(/worker_e2e/)'; else echo "cargo-nextest not installed; falling back to cargo test"; {{rust_dev_env}}; cargo test -q --locked -- --skip worker_e2e; fi

test-fast:
    if cargo nextest --version >/dev/null 2>&1; then {{rust_dev_env}}; cargo nextest run --locked --lib -E 'not test(/worker_e2e/)'; else {{rust_dev_env}}; cargo test -q --lib --locked -- --skip worker_e2e; fi

test-infra:
    {{rust_dev_env}}; cargo test --locked worker_e2e -- --ignored --nocapture

mcp-smoke:
    ./scripts/test-mcp-tools-mcporter.sh

client-server-smoke:
    ./scripts/test-client-server-mode.sh

test-all:
    {{rust_dev_env}}; cargo test --all-targets --all-features --locked

nextest-install:
    {{rust_dev_env}}; cargo install cargo-nextest --locked

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

clippy:
    {{rust_dev_env}}; cargo clippy --all-targets --locked -- -D warnings

build:
    {{rust_dev_env}}; cargo build --release --locked
    mkdir -p bin
    AXON_TARGET_DIR="${CARGO_TARGET_DIR:-target}"; cp "$AXON_TARGET_DIR/release/axon" bin/axon

debug:
    {{rust_dev_env}}; cargo build --locked --bin axon

install:
    {{rust_dev_env}}; cargo build --release --locked
    mkdir -p ~/.local/bin
    AXON_TARGET_DIR="${CARGO_TARGET_DIR:-target}"; case "$AXON_TARGET_DIR" in /*) AXON_BIN="$AXON_TARGET_DIR/release/axon" ;; *) AXON_BIN="$(pwd)/$AXON_TARGET_DIR/release/axon" ;; esac; ln -sf "$AXON_BIN" ~/.local/bin/axon; PLUGIN_BIN="${AXON_PLUGIN_BIN:-$HOME/.claude/plugins/cache/jmagar-lab/axon/575c090bcaf5/bin/axon}"; if [ -e "$PLUGIN_BIN" ] || [ -L "$PLUGIN_BIN" ]; then mkdir -p "$(dirname "$PLUGIN_BIN")"; ln -sf "$AXON_BIN" "$PLUGIN_BIN"; systemctl --user restart axon-mcp 2>/dev/null || true; fi

install-debug:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p ~/.local/bin
    AXON_TARGET_DIR="${CARGO_TARGET_DIR:-target}"
    case "$AXON_TARGET_DIR" in
      /*) AXON_BIN="$AXON_TARGET_DIR/debug/axon" ;;
      *) AXON_BIN="$(pwd)/$AXON_TARGET_DIR/debug/axon" ;;
    esac
    stale=0
    if [ ! -x "$AXON_BIN" ]; then
      stale=1
    else
      while IFS= read -r -d '' input; do
        if [ "$input" -nt "$AXON_BIN" ]; then
          stale=1
          break
        fi
      done < <(git ls-files -z -- Cargo.toml Cargo.lock rust-toolchain.toml .cargo src config.example.toml docker-compose.yaml config migrations)
    fi
    if [ "$stale" -eq 1 ]; then
      just debug
    else
      echo "debug binary is current: $AXON_BIN"
    fi
    ln -sf "$AXON_BIN" ~/.local/bin/axon
    PLUGIN_BIN="${AXON_PLUGIN_BIN:-$HOME/.claude/plugins/cache/jmagar-lab/axon/575c090bcaf5/bin/axon}"
    if [ -e "$PLUGIN_BIN" ] || [ -L "$PLUGIN_BIN" ]; then
      mkdir -p "$(dirname "$PLUGIN_BIN")"
      ln -sf "$AXON_BIN" "$PLUGIN_BIN"
      if systemctl --user list-unit-files axon-mcp.service >/dev/null 2>&1; then
        systemctl --user restart axon-mcp || true
      fi
    fi

lint-all:
    just fmt-check
    just clippy

verify:
    just fmt-check
    just clippy
    just check
    just test

ci:
    just verify

precommit:
    python3 scripts/enforce_no_legacy_symbols.py
    if [ -f "$HOME/.claude/hooks/enforce_monoliths.py" ]; then python3 "$HOME/.claude/hooks/enforce_monoliths.py" --staged; elif [ -f "scripts/enforce_monoliths.py" ]; then python3 scripts/enforce_monoliths.py --staged; else echo "ERROR: enforce_monoliths.py not found" && exit 1; fi
    just fmt-check
    just clippy
    just check
    just test

# Whole-repo monolith size report (informational, exits 0).
# Lists every oversized file/function not already in .monolith-allowlist.
# Pass --include-allowlisted to also surface allowlisted entries.
monolith-report *ARGS:
    python3 scripts/enforce_monoliths.py --whole-repo {{ARGS}}

fix:
    cargo fmt --all
    {{rust_dev_env}}; cargo clippy --fix --all-targets --locked --allow-dirty --allow-staged

fix-all:
    just fix

taplo-check:
    if command -v taplo >/dev/null 2>&1; then taplo fmt --check; else echo "taplo not installed. Run: cargo install taplo-cli --locked"; exit 1; fi

taplo-fmt:
    if command -v taplo >/dev/null 2>&1; then taplo fmt; else echo "taplo not installed. Run: cargo install taplo-cli --locked"; exit 1; fi

llvm-cov-install:
    {{rust_dev_env}}; cargo install cargo-llvm-cov --locked

coverage-branch:
    if cargo llvm-cov --version >/dev/null 2>&1; then {{rust_dev_env}}; cargo llvm-cov --locked --workspace --all-features --lcov --output-path .cache/coverage/lcov.info; else echo "cargo-llvm-cov not installed. Run: just llvm-cov-install"; exit 1; fi

# ── Codegen ───────────────────────────────────────────────────

gen-mcp-schema *ARGS:
    python3 scripts/generate_mcp_schema_doc.py {{ARGS}}

clean:
    cargo clean

# Start infrastructure services (qdrant, tei, chrome)
services-up:
    docker compose --env-file "${AXON_ENV_FILE:-$HOME/.axon/.env}" -f docker-compose.yaml up -d axon-qdrant axon-tei axon-chrome

# Stop infrastructure services
services-down:
    docker compose --env-file "${AXON_ENV_FILE:-$HOME/.axon/.env}" -f docker-compose.yaml stop axon-qdrant axon-tei axon-chrome
    docker compose --env-file "${AXON_ENV_FILE:-$HOME/.axon/.env}" -f docker-compose.yaml rm -f axon-qdrant axon-tei axon-chrome

# Backward-compatible aliases used by setup/docs for local infra.
test-infra-up:
    just services-up

test-infra-down:
    just services-down

watch-check:
    cargo watch -x 'check -q --locked' -x 'check -q --tests --locked' -x 'test -q --lib --locked -- --skip worker_e2e'

rebuild:
    just check
    just test

# ── Local dev ────────────────────────────────────────────────────

# Kill any running axon mcp or workers
stop:
    -pkill -f 'axon.*(mcp|crawl worker|embed worker|extract worker|ingest worker)' 2>/dev/null || true
    @echo "Stopped running servers and workers"

# Start infra (Qdrant, TEI, Chrome), then run axon mcp as the worker daemon.
# Fire-and-forget CLI jobs require axon mcp running to be processed.
dev:
    #!/usr/bin/env bash
    set -euo pipefail
    just stop
    sleep 1
    export RUST_LOG="${RUST_LOG:-info,axon.mcp.oauth=info,axon::crates::mcp=info}"
    {{rust_dev_env}};
    cargo build --locked --bin axon
    AXON_BIN="${CARGO_TARGET_DIR:-$(pwd)/target}/debug/axon"
    docker compose --env-file "${AXON_ENV_FILE:-$HOME/.axon/.env}" -f docker-compose.yaml up -d --wait axon-qdrant axon-tei axon-chrome
    "$AXON_BIN" mcp

# ── Perf bench ────────────────────────────────────────────────────────────────

# Run the ask perf bench harness. Defaults: 30 runs, both cold+warm modes.
# See docs/perf/README.md for sample-size guidance and prereqs.
bench-ask runs="30" mode="cold":
    @bash scripts/bench-ask.sh --runs {{runs}} --mode {{mode}}
