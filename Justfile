set shell := ["bash", "-euo", "pipefail", "-c"]
rust_dev_env := "if command -v mold >/dev/null 2>&1; then export RUSTFLAGS=\"${RUSTFLAGS:-} -C link-arg=-fuse-ld=mold\"; fi"

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

test-watch:
    {{rust_dev_env}}; RUST_MIN_STACK=16777216 cargo test -q --lib --locked jobs::watch
    {{rust_dev_env}}; cargo test -q --lib --locked cli::commands::watch
    {{rust_dev_env}}; cargo test -q --lib --locked parse_watch
    {{rust_dev_env}}; cargo test -q --lib --locked web::server::handlers::rest::tests::watch_

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

web-build:
    cd apps/web && npm run build

web-check:
    cd apps/web && npm run lint
    cd apps/web && npm run openapi:check

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
    just link-bin

debug:
    {{rust_dev_env}}; cargo build --locked --bin axon

# Symlink the compiled release binary into PATH and all known plugin cache slots.
# Called automatically by `just build` and `just install`. Safe to call manually
# after `cargo build --release` so that `axon` on $PATH always matches the DB state.
link-bin:
    #!/usr/bin/env bash
    set -euo pipefail
    AXON_TARGET_DIR="${CARGO_TARGET_DIR:-target}"
    case "$AXON_TARGET_DIR" in
      /*) AXON_BIN="$AXON_TARGET_DIR/release/axon" ;;
      *)  AXON_BIN="$(pwd)/$AXON_TARGET_DIR/release/axon" ;;
    esac
    if [ ! -x "$AXON_BIN" ]; then
      echo "release binary not found at $AXON_BIN — run 'just build' first" >&2
      exit 1
    fi
    mkdir -p ~/.local/bin
    ln -sf "$AXON_BIN" ~/.local/bin/axon
    # Update every versioned slot under the plugin cache so whichever version
    # the plugin manager activates, it always runs the workspace binary.
    while IFS= read -r -d '' plugin_bin; do
      ln -sf "$AXON_BIN" "$plugin_bin"
    done < <(find "${HOME}/.claude/plugins/cache/jmagar-lab/axon" -maxdepth 3 -name "axon" \( -type f -o -type l \) -print0 2>/dev/null)
    systemctl --user restart axon-mcp 2>/dev/null || true
    echo "axon → $AXON_BIN"

install:
    {{rust_dev_env}}; cargo build --release --locked
    just link-bin

# Build the release binary and bundle it into the Claude Code plugin (Git LFS),
# so plugins/axon/bin/axon ships a prebuilt binary like the rest of the family.
build-plugin:
    {{rust_dev_env}}; cargo build --release --locked --bin axon
    mkdir -p plugins/axon/bin
    AXON_TARGET_DIR="${CARGO_TARGET_DIR:-target}"; install -m 755 "$AXON_TARGET_DIR/release/axon" plugins/axon/bin/axon
    @echo "Bundled plugins/axon/bin/axon"

# Build the local dev runtime image from this checkout.
container-build:
    #!/usr/bin/env bash
    set -euo pipefail
    source scripts/lib/axon-env.sh
    repo="$(pwd)"
    env_file="$(resolve_axon_env_file "$repo")"
    compose=(docker compose)
    if [ -f "$env_file" ]; then
      compose+=(--env-file "$env_file")
    fi
    compose+=(-f docker-compose.yaml)
    "${compose[@]}" build axon

# Recreate the axon service with the locally built debug binary bind-mounted.
container-up:
    #!/usr/bin/env bash
    set -euo pipefail
    source scripts/lib/axon-env.sh
    repo="$(pwd)"
    env_file="$(resolve_axon_env_file "$repo")"
    cargo build --locked --bin axon
    compose=(docker compose)
    if [ -f "$env_file" ]; then
      compose+=(--env-file "$env_file")
    fi
    export AXON_DEV_TARGET_DIR="${CARGO_TARGET_DIR:-$repo/target}/debug"
    compose+=(-f docker-compose.yaml)
    "${compose[@]}" up -d axon --no-deps
    "${compose[@]}" ps axon

# Build release binary, sync PATH symlinks, rebuild local dev runtime image, restart container.
# Synchronous version of what `scripts/axon` does automatically in the background.
sync-container:
    #!/usr/bin/env bash
    set -euo pipefail
    source scripts/lib/axon-env.sh
    repo="$(pwd)"
    load_axon_env_file "$repo"
    env_file="$(resolve_axon_env_file "$repo")"
    if command -v mold >/dev/null 2>&1; then
      export RUSTFLAGS="${RUSTFLAGS:-} -C link-arg=-fuse-ld=mold"
    fi
    cargo build --release --locked --bin axon

    AXON_TARGET_DIR="${CARGO_TARGET_DIR:-target}"
    case "$AXON_TARGET_DIR" in
      /*) AXON_BIN="$AXON_TARGET_DIR/release/axon" ;;
      *) AXON_BIN="$repo/$AXON_TARGET_DIR/release/axon" ;;
    esac
    mkdir -p ~/.local/bin
    ln -sf "$AXON_BIN" ~/.local/bin/axon
    while IFS= read -r -d '' plugin_bin; do
      ln -sf "$AXON_BIN" "$plugin_bin"
    done < <(find "${HOME}/.claude/plugins/cache/jmagar-lab/axon" -maxdepth 3 -name "axon" \( -type f -o -type l \) -print0 2>/dev/null)
    systemctl --user restart axon-mcp 2>/dev/null || true
    echo "axon -> $AXON_BIN"

    compose=(docker compose)
    if [ -f "$env_file" ]; then
      compose+=(--env-file "$env_file")
    fi
    export AXON_DEV_TARGET_DIR="$(dirname "$AXON_BIN")"
    compose+=(-f docker-compose.yaml)
    "${compose[@]}" build axon
    "${compose[@]}" up -d axon --no-deps
    touch "${AXON_TARGET_DIR}/.container-built"
    "${compose[@]}" ps axon
    echo "container synced"

container-sync: sync-container

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
      done < <(git ls-files -z -- Cargo.toml Cargo.lock rust-toolchain.toml .cargo src config.example.toml docker-compose.prod.yaml docker-compose.yaml config migrations)
    fi
    if [ "$stale" -eq 1 ]; then
      just debug
    else
      echo "debug binary is current: $AXON_BIN"
    fi
    mkdir -p ~/.local/bin
    ln -sf "$AXON_BIN" ~/.local/bin/axon
    while IFS= read -r -d '' plugin_bin; do
      ln -sf "$AXON_BIN" "$plugin_bin"
    done < <(find "${HOME}/.claude/plugins/cache/jmagar-lab/axon" -maxdepth 3 -name "axon" \( -type f -o -type l \) -print0 2>/dev/null)
    systemctl --user restart axon-mcp 2>/dev/null || true
    echo "axon → $AXON_BIN"

lint-all:
    just fmt-check
    just clippy

legacy-runtime-check:
    ./scripts/check_legacy_runtime_terms.sh

validate-plugin:
    #!/usr/bin/env bash
    set -euo pipefail
    python3 - <<'PY'
    import json
    from pathlib import Path

    # The plugin manifest lives under plugins/axon/ (split out of the repo root
    # in 557591eb); fall back to the legacy root path for older checkouts.
    manifest = Path("plugins/axon/.claude-plugin/plugin.json")
    if not manifest.exists():
        manifest = Path(".claude-plugin/plugin.json")
    plugin = json.loads(manifest.read_text())
    for key in ["name", "description", "author"]:
        if not plugin.get(key):
            raise SystemExit(f"MISSING: {manifest} {key}")
    if "version" in plugin:
        raise SystemExit(f"FORBIDDEN: {manifest} version")

    monitors = manifest.parent / "monitors" / "monitors.json"
    if not monitors.exists():
        raise SystemExit(f"MISSING: {monitors}")
    json.loads(monitors.read_text())
    PY
    echo "OK"

runtime-current:
    ./scripts/axon doctor

verify:
    just legacy-runtime-check
    just validate-plugin
    just web-check
    just fmt-check
    just clippy
    just check
    just test

ci:
    just verify

precommit:
    python3 scripts/check_compose_port_bindings.py --staged
    python3 scripts/enforce_no_legacy_symbols.py
    just legacy-runtime-check
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
