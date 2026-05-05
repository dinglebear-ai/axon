set shell := ["bash", "-euo", "pipefail", "-c"]
set dotenv-load
rust_dev_env := "if command -v sccache >/dev/null 2>&1; then export RUSTC_WRAPPER=sccache; fi; if command -v mold >/dev/null 2>&1; then export RUSTFLAGS=\"${RUSTFLAGS:-} -C link-arg=-fuse-ld=mold\"; fi"

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

install:
    {{rust_dev_env}}; cargo build --release --locked
    mkdir -p ~/.local/bin
    AXON_TARGET_DIR="${CARGO_TARGET_DIR:-target}"; case "$AXON_TARGET_DIR" in /*) AXON_BIN="$AXON_TARGET_DIR/release/axon" ;; *) AXON_BIN="$(pwd)/$AXON_TARGET_DIR/release/axon" ;; esac; ln -sf "$AXON_BIN" ~/.local/bin/axon

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

fix:
    cargo fmt --all
    {{rust_dev_env}}; cargo clippy --fix --all-targets --locked --allow-dirty --allow-staged

fix-all:
    just fix

taplo-check:
    taplo fmt --check

taplo-fmt:
    taplo fmt

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
    docker compose -f config/docker-compose.services.yaml up -d

# Stop infrastructure services
services-down:
    docker compose -f config/docker-compose.services.yaml down

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
    if command -v sccache >/dev/null 2>&1; then export RUSTC_WRAPPER=sccache; fi
    if command -v mold >/dev/null 2>&1; then export RUSTFLAGS="${RUSTFLAGS:-} -C link-arg=-fuse-ld=mold"; fi
    cargo build --locked --bin axon
    AXON_BIN="${CARGO_TARGET_DIR:-$(pwd)/target}/debug/axon"
    docker compose -f config/docker-compose.services.yaml up -d --wait axon-qdrant axon-tei axon-chrome
    "$AXON_BIN" mcp
