set shell := ["bash", "-euo", "pipefail", "-c"]
rust_dev_env := "if command -v mold >/dev/null 2>&1; then export RUSTFLAGS=\"${RUSTFLAGS:-} -C link-arg=-fuse-ld=mold\"; fi"
local_release_profile := "release-fast"

default:
    @just --list

# Safely inspect and tune the tootie TEI deployment.
tei-status:
    ./scripts/tei-tune.py status

tei-presets:
    ./scripts/tei-tune.py presets

tei-apply preset="rtx4070-axon" *args:
    ./scripts/tei-tune.py apply {{preset}} {{args}}

tei-benchmark *args:
    ./scripts/tei-tune.py benchmark {{args}}

tei-rollback:
    ./scripts/tei-tune.py rollback

# Bootstrap a new development environment (checks + installs all dependencies).
# No just? Run ./scripts/dev-setup.sh directly — it installs just for you.
setup *args:
    ./scripts/dev-setup.sh {{args}}

check:
    {{rust_dev_env}}; cargo check -q --locked

check-tests:
    {{rust_dev_env}}; cargo check -q --tests --locked

test:
    if cargo nextest --version >/dev/null 2>&1; then {{rust_dev_env}}; cargo nextest run --locked --workspace; else echo "cargo-nextest not installed; falling back to cargo test"; {{rust_dev_env}}; cargo test -q --locked; fi

test-fast:
    if cargo nextest --version >/dev/null 2>&1; then {{rust_dev_env}}; cargo nextest run --locked --lib; else {{rust_dev_env}}; cargo test -q --lib --locked; fi

# Deterministic, network-free drift and schema gate for the shared E2E catalog.
e2e-catalog-check:
    ./scripts/e2e/validate-catalog.py --report
    python3 -m unittest tests/e2e/catalog/test_validate_catalog.py

# Portable, network-free contracts for run allocation and provider doubles.
e2e-isolation-check:
    python3 tests/e2e/run_isolation_tests.py

# Execute an allocation plan through authoritative teardown and emit the sole
# canonical JSON + JUnit execution/evidence report.
e2e-supervised-report plan report="target/e2e/report.json" junit="target/e2e/junit.xml":
    python3 scripts/e2e/run-supervised-suite.py {{plan}} --report {{report}} --junit {{junit}}

# Supported schema-epoch upgrade contract: digest-pinned synthetic fixture,
# current tested binary, semantic persistence, negative cases, and teardown.
e2e-upgrade:
    python3 -m unittest discover -s tests/e2e/upgrade -p 'test_*.py'
    python3 scripts/e2e/run-upgrade.py --binary target/debug/axon

# Early, non-required measured E2E slice. Provider doubles and loopback-only
# routing are mandatory; the runner always emits a cleanup/budget report.
e2e-hermetic:
    ./scripts/e2e/run-hermetic-local.sh

e2e-hermetic-inner:
    CARGO_NET_OFFLINE=true AXON_E2E_HERMETIC=1 AXON_E2E_LIVE=0 AXON_E2E_PROVIDER_MODE=double AXON_E2E_STAGE_GATES=1 AXON_E2E_NETWORK_POLICY=loopback-only AXON_E2E_EVIDENCE_POLICY=sanitized-only python3 scripts/e2e/run-hermetic.py --report target/e2e/hermetic-report.json

e2e-mutations subset="representative":
    python3 scripts/e2e/run-mutations.py --subset {{subset}}

# Reproducible local command for the bounded Linux/macOS/Windows subset. The
# canonical report is an input to release qualification; this never uses live
# providers, privileged credentials, or homelab connectivity.
e2e-platform-smoke report="target/e2e/platform-smoke-local.json":
    python3 scripts/e2e/run-platform-smoke.py --binary target/debug/axon --report {{report}} --tested-sha "$(git rev-parse HEAD)"

# Fail-closed quarantine/attempt validation and rolling reliability projection
# for any canonical report. Required test configuration starts at zero entries.
e2e-flake-governance report="target/e2e/platform-smoke-local.json" environment="local" out="target/e2e/reliability.json":
    python3 scripts/e2e/flake-governance.py --report {{report}} --environment {{environment}} --reliability-out {{out}}

# Reporting-first representative performance sample. Contended observations
# are retained as infrastructure-classified, baseline-ineligible evidence.
e2e-performance samples="5" out="target/e2e/performance/report.json" contention="--allow-contended":
    python3 scripts/e2e/measure-real-performance.py --samples {{samples}} {{contention}} --out {{out}}

# Deterministic unsigned release projection over already-saved evidence. This
# command never reruns a lane or receives live-provider/signing credentials.
e2e-qualification index evidence_root out="target/e2e/qualification.json" summary="target/e2e/qualification.md" checksums="target/e2e/SHA256SUMS":
    python3 scripts/e2e/build-qualification-manifest.py --index {{index}} --evidence-root {{evidence_root}} --out {{out}} --summary {{summary}} --checksums {{checksums}}

test-watch:
    {{rust_dev_env}}; RUST_MIN_STACK=16777216 cargo test -q --lib --locked jobs::watch
    {{rust_dev_env}}; cargo test -q --lib --locked cli::commands::watch
    {{rust_dev_env}}; cargo test -q --lib --locked parse_watch
    {{rust_dev_env}}; cargo test -q --lib --locked web::server::handlers::rest::tests::watch_

# No `worker_e2e`-named tests currently exist in the workspace (the ignored
# SQLite/in-process worker E2E suite this recipe used to run was removed).
# Kept as a documented no-op so `just test-infra` doesn't hard-fail for
# anyone following existing docs/session notes; replace this body if/when a
# real ignored infra-integration suite is reintroduced.
test-infra:
    @echo "no worker_e2e tests exist in this workspace; test-infra is currently a no-op"

mcp-smoke:
    ./scripts/test-mcp-tools-mcporter.sh

test-all:
    {{rust_dev_env}}; cargo test --all-targets --all-features --locked

web-build:
    cd apps/web && npm run build

web-check:
    cd apps/web && npm run lint
    cargo xtask check-openapi-drift

openapi-check:
    cargo xtask check-openapi-drift

# Refresh schemas and every generated Markdown projection in dependency order.
generated-contracts-refresh:
    cargo xtask generated-contracts refresh

# Verify schema provenance and all dependent generated docs without writing.
generated-contracts-check:
    cargo xtask generated-contracts check

# Package the Chrome extension into dist/axon-page-scraper-<version>.zip
package-extension:
    ./apps/chrome-extension/package.sh

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

clippy:
    {{rust_dev_env}}; cargo clippy --all-targets --locked -- -D warnings

# Fail on unused crate dependencies (keeps per-crate manifests honest after the
# workspace extraction). Graceful no-op when cargo-machete is not installed.
machete:
    @if command -v cargo-machete >/dev/null 2>&1; then cargo machete; else echo "skip: cargo-machete not installed (run: mise install cargo:cargo-machete)"; fi

build:
    {{rust_dev_env}}; cargo build --profile {{local_release_profile}} --locked
    just link-bin {{local_release_profile}}

release-build:
    {{rust_dev_env}}; cargo build --release --locked
    just link-bin release

debug:
    {{rust_dev_env}}; cargo build --locked --bin axon

# Install a built binary into ~/.local/bin/axon. Local recipes default to the
# release-fast profile; pass `release` for real release artifacts.
link-bin profile=local_release_profile:
    #!/usr/bin/env bash
    set -euo pipefail
    profile="{{profile}}"
    profile_dir="$profile"
    if [ "$profile" = "release" ]; then
      profile_dir="release"
    fi
    AXON_TARGET_DIR="${CARGO_TARGET_DIR:-target}"
    case "$AXON_TARGET_DIR" in
      /*) AXON_BIN="$AXON_TARGET_DIR/$profile_dir/axon" ;;
      *)  AXON_BIN="$(pwd)/$AXON_TARGET_DIR/$profile_dir/axon" ;;
    esac
    if [ ! -x "$AXON_BIN" ]; then
      echo "$profile binary not found at $AXON_BIN — run 'just build' first" >&2
      exit 1
    fi
    variant="$profile"
    if [ "$profile" = "release-fast" ]; then
      variant="fast-release"
    fi
    mkdir -p bin
    cp -f "$AXON_BIN" "bin/axon-$variant"
    chmod 755 "bin/axon-$variant"
    mkdir -p ~/.local/bin
    ln -sf "$AXON_BIN" ~/.local/bin/axon
    systemctl --user restart axon-mcp 2>/dev/null || true
    echo "axon → $AXON_BIN"
    echo "artifact → bin/axon-$variant"

install:
    {{rust_dev_env}}; cargo build --profile {{local_release_profile}} --locked
    just link-bin {{local_release_profile}}

install-release:
    {{rust_dev_env}}; cargo build --release --locked
    just link-bin release

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

# Build local release-fast binary when stale, sync PATH symlinks, refresh local dev runtime if needed, restart container.
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

    profile="{{local_release_profile}}"
    AXON_TARGET_DIR="${CARGO_TARGET_DIR:-target}"
    case "$AXON_TARGET_DIR" in
      /*) AXON_BIN="$AXON_TARGET_DIR/$profile/axon" ;;
      *) AXON_BIN="$repo/$AXON_TARGET_DIR/$profile/axon" ;;
    esac

    release_stale=0
    if [ ! -x "$AXON_BIN" ]; then
      release_stale=1
    else
      while IFS= read -r -d '' input; do
        if [ "$input" -nt "$AXON_BIN" ]; then
          release_stale=1
          break
        fi
      done < <(git ls-files -z -- Cargo.toml Cargo.lock rust-toolchain.toml .cargo build.rs src crates config.example.toml config migrations apps/web/out assets)
    fi
    if [ "$release_stale" -eq 1 ]; then
      cargo build --profile "$profile" --locked --bin axon
    else
      echo "release binary is current: $AXON_BIN"
    fi

    mkdir -p ~/.local/bin
    ln -sf "$AXON_BIN" ~/.local/bin/axon
    systemctl --user restart axon-mcp 2>/dev/null || true
    echo "axon -> $AXON_BIN"

    compose=(docker compose)
    if [ -f "$env_file" ]; then
      compose+=(--env-file "$env_file")
    fi
    export AXON_DEV_TARGET_DIR="$(dirname "$AXON_BIN")"
    compose+=(-f docker-compose.yaml)
    container_sentinel="$AXON_TARGET_DIR/.container-built"
    image_stale=0
    if ! docker image inspect axon:dev-runtime >/dev/null 2>&1; then
      image_stale=1
    else
      while IFS= read -r -d '' input; do
        if [ "$input" -nt "$container_sentinel" ] 2>/dev/null; then
          image_stale=1
          break
        fi
      done < <(git ls-files -z -- config/Dockerfile docker-compose.prod.yaml docker-compose.yaml config/chrome)
    fi
    if [ "$image_stale" -eq 1 ]; then
      "${compose[@]}" build axon
      touch "$container_sentinel"
      "${compose[@]}" up -d axon --no-deps
    else
      echo "dev runtime image is current"
      "${compose[@]}" up -d axon --no-deps --no-build
    fi
    "${compose[@]}" restart axon
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
      done < <(git ls-files -z -- Cargo.toml Cargo.lock rust-toolchain.toml .cargo src crates config.example.toml docker-compose.prod.yaml docker-compose.yaml config migrations)
    fi
    if [ "$stale" -eq 1 ]; then
      just debug
    else
      echo "debug binary is current: $AXON_BIN"
    fi
    mkdir -p ~/.local/bin
    ln -sf "$AXON_BIN" ~/.local/bin/axon
    systemctl --user restart axon-mcp 2>/dev/null || true
    echo "axon → $AXON_BIN"

lint-all:
    just fmt-check
    just clippy

legacy-runtime-check:
    ./scripts/check_legacy_runtime_terms.sh

# Web acquisition must go through axon_core::http::fetch_web (one ladder,
# one place to fix). Fails on a new client built outside the allowlist.
fetch-divergence-check:
    cargo xtask check-fetch-divergence

# TEST-M2 / PERF-C1: fail if block_on/block_in_place appears in the ask/retrieval hot path.
blocking-async-check:
    ./scripts/check_no_blocking_async.sh

primitive-inventory-check:
    python3 scripts/check_aurora_primitive_inventory.py

validate-plugin:
    python3 scripts/validate_plugin.py

# Machine-readable operational-test catalog. The token following each path is
# the execution class consumed by repository contract review.
# test-catalog: scripts/test-axon-env.sh hermetic-required
# test-catalog: scripts/test-axon-wrapper-fast-path.sh hermetic-required
# test-catalog: scripts/test-axon-backup.sh hermetic-required
# test-catalog: scripts/test-bench-source-pipeline.sh hermetic-required
# test-catalog: scripts/test-chrome-extension-agent-os.sh live-optional
# test-catalog: scripts/test-evaluate-retrieval.sh hermetic-required
# test-catalog: scripts/test-install-behavior.sh platform-required:linux
# test-catalog: scripts/test-incus-bootstrap.sh hermetic-required
# test-catalog: scripts/test-live-cli-portability.sh hermetic-required
# test-catalog: scripts/test-mcp-tasks-wire.py live-optional
# test-catalog: scripts/test-mlx-metrics.py hermetic-required
# test-catalog: scripts/test_mcp_doc_renderer.py hermetic-required
# test-catalog: scripts/test_qdrant_quality.py hermetic-required
# test-catalog: scripts/test_qdrant_tune.py hermetic-required
# test-catalog: scripts/test_tei_tune.py hermetic-required
# test-catalog: scripts/test_dockerfile_supply_chain.py hermetic-required
# test-catalog: scripts/test_operational_docs.py hermetic-required
# test-catalog: scripts/test-with-timeout.sh hermetic-required
operational-test-contracts:
    scripts/test-axon-env.sh
    scripts/test-axon-wrapper-fast-path.sh
    scripts/test-axon-backup.sh
    scripts/test-bench-source-pipeline.sh
    scripts/test-evaluate-retrieval.sh
    if [ "$(uname -s)" = "Linux" ]; then scripts/test-install-behavior.sh; else echo "SKIP test-install-behavior.sh (Linux platform required)"; fi
    scripts/test-incus-bootstrap.sh
    scripts/test-live-cli-portability.sh
    python3 scripts/test-mlx-metrics.py
    python3 scripts/test_mcp_doc_renderer.py
    python3 scripts/test_qdrant_quality.py
    python3 scripts/test_qdrant_tune.py
    python3 scripts/test_tei_tune.py
    python3 scripts/test_dockerfile_supply_chain.py
    python3 scripts/test_operational_docs.py
    scripts/test-with-timeout.sh

runtime-current:
    ./scripts/axon doctor

verify:
    just legacy-runtime-check
    just fetch-divergence-check
    just blocking-async-check
    just primitive-inventory-check
    just validate-plugin
    just operational-test-contracts
    just web-check
    just fmt-check
    just clippy
    just machete
    just check
    just test

ci:
    just verify

precommit:
    cargo xtask check-secrets
    python3 scripts/check_compose_port_bindings.py --staged
    python3 scripts/enforce_no_legacy_symbols.py
    just legacy-runtime-check
    just fetch-divergence-check
    just primitive-inventory-check
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
    if command -v taplo >/dev/null 2>&1; then taplo fmt --check; else echo "taplo not installed. Run: mise install cargo:taplo-cli"; exit 1; fi

taplo-fmt:
    if command -v taplo >/dev/null 2>&1; then taplo fmt; else echo "taplo not installed. Run: mise install cargo:taplo-cli"; exit 1; fi

coverage-branch:
    if cargo llvm-cov --version >/dev/null 2>&1; then {{rust_dev_env}}; cargo llvm-cov --locked --workspace --all-features --lcov --output-path .cache/coverage/lcov.info; else echo "cargo-llvm-cov not installed. Run: mise install cargo:cargo-llvm-cov"; exit 1; fi

# ── Codegen ───────────────────────────────────────────────────

gen-mcp-schema *ARGS:
    python3 scripts/generate_mcp_schema_doc.py {{ARGS}}

clean:
    cargo clean

# Backward-compatible, self-contained local infrastructure default.
services-up:
    just services-up-local

services-up-local:
    just ensure-compose-network
    docker compose --env-file "${AXON_ENV_FILE:-$HOME/.axon/.env}" -f docker-compose.yaml --profile local-qdrant up -d axon-qdrant axon-tei axon-chrome

services-up-external-qdrant:
    #!/usr/bin/env bash
    set -euo pipefail
    just ensure-compose-network
    source scripts/lib/axon-env.sh
    repo="$(pwd)"
    load_axon_env_file "$repo"
    : "${AXON_EXTERNAL_QDRANT_URL:?AXON_EXTERNAL_QDRANT_URL must be set}"
    env_file="$(resolve_axon_env_file "$repo")"
    echo "external Qdrant: $AXON_EXTERNAL_QDRANT_URL"
    docker compose --env-file "$env_file" -f docker-compose.yaml up -d axon-tei axon-chrome

# Stop infrastructure services
services-down:
    docker compose --env-file "${AXON_ENV_FILE:-$HOME/.axon/.env}" -f docker-compose.yaml --profile local-qdrant stop axon-qdrant axon-tei axon-chrome
    docker compose --env-file "${AXON_ENV_FILE:-$HOME/.axon/.env}" -f docker-compose.yaml --profile local-qdrant rm -f axon-qdrant axon-tei axon-chrome
    just remove-compose-network-if-owned

# Start/stop an explicitly local Qdrant. Use this only for local test data or
# when AXON_QDRANT_URL=http://axon-qdrant:6333 is set for the axon container.
qdrant-up:
    just ensure-compose-network
    docker compose --env-file "${AXON_ENV_FILE:-$HOME/.axon/.env}" -f docker-compose.yaml --profile local-qdrant up -d axon-qdrant

qdrant-down:
    docker compose --env-file "${AXON_ENV_FILE:-$HOME/.axon/.env}" -f docker-compose.yaml --profile local-qdrant stop axon-qdrant
    docker compose --env-file "${AXON_ENV_FILE:-$HOME/.axon/.env}" -f docker-compose.yaml --profile local-qdrant rm -f axon-qdrant
    just remove-compose-network-if-owned

# Production stack (docker-compose.prod.yaml), bundled qdrant mode — the default.
# Every invocation guarantees --env-file so .env's values actually reach
# Compose interpolation (a bare `docker compose up` from the wrong cwd can
# miss the file entirely). Note this guarantees .env is READ, not that no
# default-drift is possible: docker-compose.prod.yaml's own
# TEI_MAX_CONCURRENT_REQUESTS:-512 fallback is a separate value from TEI's own
# built-in default — this repo has hit that exact two-layer drift before
# (32 vs 256 permits) and --env-file alone doesn't prevent a repeat if the
# YAML's own fallback and .env's intended value ever diverge again.
ensure-compose-network:
    #!/usr/bin/env bash
    set -euo pipefail
    source scripts/lib/axon-env.sh
    repo="$(pwd)"
    load_axon_env_file "$repo"
    network="${DOCKER_NETWORK:-axon}"
    if docker network inspect "$network" >/dev/null 2>&1; then
      driver="$(docker network inspect "$network" | python3 -c 'import json,sys; print(json.load(sys.stdin)[0]["Driver"])')"
      [ "$driver" = bridge ] || { echo "network $network exists with incompatible driver $driver" >&2; exit 1; }
    else
      docker network create --driver bridge --label ai.dinglebear.axon.network-owner=preflight "$network" >/dev/null
    fi

remove-compose-network-if-owned:
    #!/usr/bin/env bash
    set -euo pipefail
    source scripts/lib/axon-env.sh
    repo="$(pwd)"
    load_axon_env_file "$repo"
    network="${DOCKER_NETWORK:-axon}"
    owner="$(docker network inspect "$network" 2>/dev/null | python3 -c 'import json,sys; data=json.load(sys.stdin); print((data[0].get("Labels") or {}).get("ai.dinglebear.axon.network-owner", ""))' || true)"
    if [ "$owner" = preflight ]; then
      docker network rm "$network" >/dev/null
    fi

prod-up:
    #!/usr/bin/env bash
    set -euo pipefail
    just ensure-compose-network
    source scripts/lib/axon-env.sh
    repo="$(pwd)"
    env_file="$(resolve_axon_env_file "$repo")"
    if [ -f "$env_file" ]; then
      perm=$(stat -c '%a' "$env_file" 2>/dev/null || stat -f '%Lp' "$env_file")
      if [ "${perm: -2}" != "00" ]; then
        echo "warn: $env_file is group/world-readable (mode $perm) — tighten with chmod 600" >&2
      fi
    fi
    echo "=== bundled qdrant IS starting locally (default mode) ==="
    docker compose --env-file "$env_file" -f docker-compose.prod.yaml up -d

prod-down:
    #!/usr/bin/env bash
    set -euo pipefail
    source scripts/lib/axon-env.sh
    env_file="$(resolve_axon_env_file "$(pwd)")"
    docker compose --env-file "$env_file" -f docker-compose.prod.yaml down
    just remove-compose-network-if-owned

# Production stack, external-qdrant override — this deployment's mode (qdrant
# lives on tootie). Requires AXON_EXTERNAL_QDRANT_URL; fails loudly if unset.
prod-up-external-qdrant:
    #!/usr/bin/env bash
    set -euo pipefail
    just ensure-compose-network
    source scripts/lib/axon-env.sh
    repo="$(pwd)"
    load_axon_env_file "$repo"
    env_file="$(resolve_axon_env_file "$repo")"
    echo "=== bundled qdrant is NOT starting, using external QDRANT_URL=${AXON_EXTERNAL_QDRANT_URL:?AXON_EXTERNAL_QDRANT_URL must be set} ==="
    docker compose --env-file "$env_file" -f docker-compose.prod.yaml -f docker-compose.external-qdrant.yaml up -d

prod-down-external-qdrant:
    #!/usr/bin/env bash
    set -euo pipefail
    source scripts/lib/axon-env.sh
    env_file="$(resolve_axon_env_file "$(pwd)")"
    docker compose --env-file "$env_file" -f docker-compose.prod.yaml -f docker-compose.external-qdrant.yaml down
    just remove-compose-network-if-owned

# Backward-compatible aliases used by setup/docs for local infra.
test-infra-up:
    just services-up

test-infra-down:
    just services-down

watch-check:
    cargo watch -x 'check -q --locked' -x 'check -q --tests --locked' -x 'test -q --lib --locked'

rebuild:
    just check
    just test

# ── Local dev ────────────────────────────────────────────────────

# Kill any running axon mcp or workers
stop:
    -pkill -f 'axon.*(mcp|jobs worker)' 2>/dev/null || true
    @echo "Stopped running servers and workers"

# Start local infra (TEI, Chrome), then run axon mcp as the worker daemon.
# Qdrant is expected at AXON_QDRANT_URL/QDRANT_URL, defaulting to tootie.
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
    docker compose --env-file "${AXON_ENV_FILE:-$HOME/.axon/.env}" -f docker-compose.yaml up -d --wait axon-tei axon-chrome
    "$AXON_BIN" mcp

# ── Perf bench ────────────────────────────────────────────────────────────────

# Run the ask perf bench harness. Defaults: 30 runs, both cold+warm modes.
# See docs/perf/README.md for sample-size guidance and prereqs.
bench-ask runs="30" mode="cold":
    @bash scripts/bench-ask.sh --runs {{runs}} --mode {{mode}}
