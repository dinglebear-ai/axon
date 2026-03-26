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

install:
    {{rust_dev_env}}; cargo build --release --locked
    mkdir -p ~/.local/bin
    ln -sf "$(pwd)/target/release/axon" ~/.local/bin/axon

lint-all:
    just fmt-check
    just clippy
    cd apps/web && pnpm lint

verify:
    ./scripts/check_dockerignore_guards.sh
    just fmt-check
    just clippy
    just check
    just test

ci:
    just verify

precommit:
    python3 scripts/enforce_no_legacy_symbols.py
    ./scripts/check_dockerignore_guards.sh
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
    cd apps/web && pnpm format

llvm-cov-install:
    {{rust_dev_env}}; cargo install cargo-llvm-cov --locked

coverage-branch:
    if cargo llvm-cov --version >/dev/null 2>&1; then {{rust_dev_env}}; cargo llvm-cov --locked --workspace --all-features --lcov --output-path .cache/coverage/lcov.info; else echo "cargo-llvm-cov not installed. Run: just llvm-cov-install"; exit 1; fi

# ── Codegen ───────────────────────────────────────────────────

gen-mcp-schema *ARGS:
    python3 scripts/generate_mcp_schema_doc.py {{ARGS}}

clean:
    cargo clean

docker-build tag="axon:local":
    docker build -f docker/Dockerfile -t {{tag}} .

# Start infrastructure services (postgres, redis, rabbitmq, qdrant, tei, chrome)
services-up:
    docker compose -f docker-compose.services.yaml up -d

# Stop infrastructure services
services-down:
    docker compose -f docker-compose.services.yaml down

# Start app containers (workers + web) — requires services-up first
up:
    ./scripts/rebuild-fresh.sh

# Stop app containers
down:
    docker compose down

# Stop everything (app + infra)
down-all:
    docker compose down
    docker compose -f docker-compose.services.yaml down

test-infra-up:
    docker compose -f docker-compose.test.yaml up -d

test-infra-down:
    docker compose -f docker-compose.test.yaml down -v

docker-up:
    ./scripts/rebuild-fresh.sh

docker-down:
    docker compose down
    docker compose -f docker-compose.services.yaml down

rebuild-fresh:
    ./scripts/rebuild-fresh.sh

cache-status:
    ./scripts/cache-guard.sh status

cache-prune:
    ./scripts/cache-guard.sh prune

docker-context-probe:
    ./scripts/check_docker_context_size.sh

check-container-revisions:
    ./scripts/check-container-revisions.sh

watch-check:
    cargo watch -x 'check -q --locked' -x 'check -q --tests --locked' -x 'test -q --lib --locked -- --skip worker_e2e'

rebuild:
    just check
    just test
    just docker-build

# ── Local stack supervisor ────────────────────────────────────────

serve port="49000":
    {{rust_dev_env}}; AXON_SERVE_HOST=0.0.0.0 cargo run --locked --bin axon -- serve --port {{port}}

serve-release port="49000":
    {{rust_dev_env}}; AXON_SERVE_HOST=0.0.0.0 cargo run --release --locked --bin axon -- serve --port {{port}}

# ── Web UI (Next.js dashboard) ────────────────────────────────────

web-dev:
    cd apps/web && pnpm dev

web-build:
    cd apps/web && pnpm build

web-lint:
    cd apps/web && pnpm lint

web-format:
    cd apps/web && pnpm format

# ── Full stack ────────────────────────────────────────────────────

# Kill any running axon serve, mcp, workers, or Next.js dev processes
stop:
    -pkill -f 'axon.*(serve|mcp|crawl worker|embed worker|extract worker|ingest worker|refresh worker|graph worker)' 2>/dev/null || true
    -pkill -f 'next dev' 2>/dev/null || true
    -pkill -f 'shell-server.mjs' 2>/dev/null || true
    @echo "Stopped running servers and workers"

# Start workers only (crawl, embed, extract, ingest, refresh, graph)
workers:
    #!/usr/bin/env bash
    set -euo pipefail
    if command -v sccache >/dev/null 2>&1; then export RUSTC_WRAPPER=sccache; fi
    if command -v mold >/dev/null 2>&1; then export RUSTFLAGS="${RUSTFLAGS:-} -C link-arg=-fuse-ld=mold"; fi
    cargo build --locked --bin axon
    AXON_BIN="${CARGO_TARGET_DIR:-$(pwd)/target}/debug/axon"
    PIDS=()
    cleanup() { kill "${PIDS[@]}" 2>/dev/null || true; }
    trap cleanup INT TERM EXIT
    "$AXON_BIN" crawl worker & PIDS+=($!)
    "$AXON_BIN" embed worker & PIDS+=($!)
    "$AXON_BIN" extract worker & PIDS+=($!)
    "$AXON_BIN" ingest worker & PIDS+=($!)
    "$AXON_BIN" refresh worker & PIDS+=($!)
    "$AXON_BIN" graph worker & PIDS+=($!)
    EXIT=0
    for pid in "${PIDS[@]}"; do wait "$pid" || EXIT=$?; done
    exit "$EXIT"

# Start infra, then hand off to the Rust supervisor.
# `axon serve` now owns the backend bridge, MCP HTTP server, workers, shell server, and Next.js.
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
    docker compose -f docker-compose.services.yaml up -d --wait axon-postgres axon-redis axon-rabbitmq axon-qdrant axon-tei axon-chrome
    AXON_SERVE_HOST=0.0.0.0 "$AXON_BIN" serve --port 49000
