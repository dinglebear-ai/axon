# Testing Guide
Last Modified: 2026-03-09

This document defines how to run tests locally and in CI for `axon`.

## Goals
- Keep the default local loop fast.
- Keep infra-backed tests explicit and reproducible.
- Ensure CI and local workflows stay aligned.

## Test Lanes

### Fast local lane (default)
Use this for most edits:

```bash
just test
```

Behavior:
- Uses `cargo nextest` when available.
- Falls back to `cargo test` if `cargo-nextest` is not installed.
- Skips `worker_e2e` tests.
- Enforces lockfile reproducibility (`--locked`).

### Fastest inner loop (lib-focused)

```bash
just test-fast
```

Use while iterating on library logic; excludes `worker_e2e`.

### Infra lane (explicit)
Use this when touching queue/worker/DB/integration behavior:

```bash
just test-infra
```

Behavior:
- Runs ignored `worker_e2e` tests explicitly.
- Requires local infra dependencies to be reachable.

### Client/server smoke lane

Use this when touching `AXON_SERVER_URL`, `/v1/actions`, artifact handles, or
Docker/systemd runtime wiring. The smoke must not edit `~/.axon/.env` or
`~/.axon/config.toml`; pass temporary env overrides in the command invocation.

```bash
AXON_SERVER_URL=http://127.0.0.1:8001 axon status --json
AXON_SERVER_URL=http://127.0.0.1:8001 axon scrape https://example.com --json
```

Expected behavior:
- `status` and stateful commands call the server, not local workers.
- scrape/crawl responses include server-owned output/artifact handles.
- host-local scrape markdown is not created as the CLI source of truth.
- token-auth failures, dead server failures, and schema mismatches fail clearly.

### Integration suite lane (infra-backed, skip-on-missing)

A separate set of integration tests targets live Qdrant instances and other external services.
These tests do **not** use `#[ignore]` — instead each test calls a resolver (`resolve_test_amqp_url()`, etc.)
that returns `None` and exits cleanly when the corresponding env var is unset.
This means they run in `just test` without error, but only exercise real I/O when infra is available.

Start the required services explicitly, then run the full suite normally:

```bash
just services-up      # Qdrant, TEI, Chrome from docker-compose.yaml
just test
```

Integration suites currently covered:

| File | Env var required | What it tests |
|------|-----------------|---------------|
| `crates/web/execute/tests/ws_protocol_tests.rs` | none | WebSocket protocol frame serialization/deserialization |

## Test Infrastructure Environment Variables

Set these in `~/.axon/.env`:

| Variable | Default (test containers) | Purpose |
|----------|--------------------------|---------|
| `AXON_TEST_QDRANT_URL` | `http://127.0.0.1:53335` | Qdrant integration tests |

The tracked local compose file is `docker-compose.yaml`. It
starts the dev infrastructure stack on loopback-bound host ports:

| Service | Image | Test port |
|---------|-------|-----------|
| `axon-qdrant` | `qdrant/qdrant:v1.13.1` | `53333` (HTTP), `53334` (gRPC) |
| `axon-tei` | `ghcr.io/huggingface/text-embeddings-inference:89-1.9` | `52000` (HTTP) |
| `axon-chrome` | local Chrome image | `6000`, `9222`, `9223` |

CI still provisions Postgres, Redis, and RabbitMQ as GitHub Actions service
containers for legacy ignored worker tests. Those services are not part of the
tracked local compose stack.

## Coverage Areas (v0.11.1+)

### Rust: `crates/services/`

Integration tests under `tests/` cover the LLM backend and services layer end-to-end:

| File | Tests | What is covered |
|------|-------|----------------|
| `tests/services_discovery_services.rs` | 16 | Service discovery contracts |
| `tests/services_lifecycle_services.rs` | 16 | Service lifecycle state machine |
| `tests/services_query_services.rs` | 13 | Query service dispatch |
| `tests/services_system_services.rs` | 8 | System-level service operations |
| `tests/services_compile_services_smoke.rs` | 1 | Services crate compile smoke |

### Rust: `crates/web/`

WebSocket and execute-path tests:

| File | Tests | What is covered |
|------|-------|----------------|
| `tests/web_ws_async_fire_and_forget.rs` | 9 | Async WS fire-and-forget execution paths |
| `tests/web_ws_override_mapping.rs` | 19 | WS mode and flag override mapping |
| `crates/web/execute/tests/ws_protocol_tests.rs` | (inline) | WS protocol frame encode/decode |
| `crates/web/execute/tests/ws_event_v2_tests.rs` | (inline) | WS event v2 serialization |

### Rust: CLI and MCP contracts

| File | Tests | What is covered |
|------|-------|----------------|
| `tests/cli_full_rewire_smoke.rs` | 28 | Full CLI flag rewire smoke (all commands) |
| `tests/cli_system_rewire_regression.rs` | 11 | System command regression after CLI refactor |
| `tests/cli_help_contract.rs` | 3 | `--help` output contracts |
| `tests/mcp_contract_parity.rs` | 24 | MCP tool schema parity with handler implementations |
| `tests/mcp_option_mappers.rs` | 15 | MCP option field mappers |

### Rust: proptest suites

Property-based tests with randomized inputs:

| File | Subject |
|------|---------|
| `crates/core/http/proptest_tests.rs` | HTTP SSRF validator (`validate_url`) — arbitrary host/IP/port inputs |
| `crates/crawl/engine/url_utils_proptest.rs` | `is_junk_discovered_url` — arbitrary URL strings |
| `crates/vector/ops/input_proptest.rs` | Vector input chunking — arbitrary text lengths and overlaps |

### TypeScript: `apps/web/__tests__/`

New TypeScript test files added in v0.11.1:

| File | What is covered |
|------|----------------|
| `api-fetch.test.ts` | `apiFetch` utility — token injection, error handling |
| `api/cortex-routes.test.ts` | `/api/cortex/*` route handlers |
| `api/sessions-routes.test.ts` | `/api/sessions/*` route handlers |
| `api/workspace-route.test.ts` | `/api/workspace` route handler |
| `pulse-chat-api-lib.test.ts` | Pulse chat API library — streaming, message assembly |
| `pulse-session-store.test.ts` | Pulse session store — persistence, hydration, eviction |

## Test-Only Security Escape Hatches

Several tests deliberately use narrow exceptions that must not be copied into
production code:

- `crates/core/http/client.rs` leaks one `reqwest::Client` per test call with
  `Box::leak` so each async test gets a client bound to its own Tokio runtime.
  This is `#[cfg(test)]` only; production uses the process-wide `HTTP_CLIENT`
  singleton.
- `crates/core/http/ssrf.rs` exposes the `ALLOW_LOOPBACK` thread-local only in
  test builds. It lets httpmock-based tests reach `127.0.0.1` while keeping
  `validate_url()` loopback blocking active by default.

These patterns are acceptable only because they are compile-time test scoped.
New tests that need a bypass should keep it behind `#[cfg(test)]` or a dedicated
test-helper feature, and production paths should continue to go through the
normal SSRF and LLM backend validation boundaries.

## Validation Commands

### Compile checks
```bash
just check
just check-tests
```

### Full pre-push gate
```bash
just verify
```

`just verify` runs:
- `./scripts/check_dockerignore_guards.sh`
- `fmt-check`
- `clippy`
- `check`
- `test`

## CI Mapping

- `test` job: standard Rust test lane (`cargo test --all --locked --features test-helpers -- --skip worker_e2e`) plus ignored CLI infra tests. Uses GitHub Actions service containers for Postgres, Redis, and RabbitMQ.
- `test-infra` job: scheduled/manual-only lane, triggered by schedule or `workflow_dispatch` input `run_infra_tests=true`. Runs `just test-infra` against GitHub Actions service containers.
- `live-qdrant` job: scheduled/manual-only lane for ignored live-Qdrant tests.
- `mcp-smoke` job: builds the release binary, starts `docker-compose.yaml` infra plus a CPU TEI container, and runs `scripts/test-mcp-tools-mcporter.sh`.
- `security` job: explicit `cargo audit --deny warnings` and `cargo deny check` with pinned tool versions.
- `msrv` job: validates declared MSRV separately.

## MCP Tooling Tests (mcporter)

Use the existing smoke script to validate MCP tool contract coverage and real mcporter behavior in both runtime modes:

```bash
# wrapper
just mcp-smoke

# equivalent direct script call
bash ./scripts/test-mcp-tools-mcporter.sh
```

Prerequisites:
- `mcporter` installed (`npm install -g mcporter@0.7.3`).
- `jq` installed.
- Debug binary built: `cargo build --bin axon`.
- MCP config available at [`config/mcporter.json`](/home/jmagar/workspace/axon_rust/config/mcporter.json).

Useful direct checks:

```bash
mcporter --config config/mcporter.json list axon --schema
mcporter --config config/mcporter.json call axon.axon action:help response_mode:inline --output json
mcporter --config config/mcporter.json call axon.axon action:crawl subaction:list limit:5 offset:0 --output json
```

Notes:
- Script artifacts/logs are written under `.cache/mcporter-test/`.
- The script generates suite-specific mcporter configs under `.cache/mcporter-test/` and runs with `AXON_LITE=1`.
- The suite requires Qdrant and TEI to be running.
- `screenshot` uses a higher mcporter call timeout than the default because Chrome startup can exceed 60s on some machines.
- CI parity: the `mcp-smoke` workflow job runs this same script in GitHub Actions.
- Canonical MCP runtime/testing reference: `docs/MCP.md`.

## Recommended Local Setup

```bash
just nextest-install
just llvm-cov-install
```

Optional performance helpers already auto-detected by `just` recipes:
- `sccache`
- `mold`

## Coverage (branch-level)

Run once per branch before merge:

```bash
just coverage-branch
```

## Common Failure Modes

### `worker_e2e` tests not running
- Cause: They are intentionally `#[ignore]` in default test lane.
- Fix: Run `just test-infra`.

### Integration tests silently skipping
- Cause: the relevant `AXON_TEST_*` URL for that suite is unset.
- Fix: start the needed service and set the matching env var. For Qdrant, use `just services-up` and `AXON_TEST_QDRANT_URL=http://127.0.0.1:53333`. For legacy worker tests, provide Postgres/Redis/RabbitMQ URLs or run the CI `test-infra` lane.

### Lockfile errors in CI/local commands
- Cause: dependency graph changed but lockfile not updated.
- Fix: run a lockfile-refreshing command locally, then rerun `just verify`.

### DB test connection/auth failures
- Check `AXON_TEST_PG_URL` first.
- If unset, test resolver falls back to `.env` and then defaults.
- Ensure credentials in local `.env` match running Postgres.


## Pull Request Checklist (Testing)
- Ran `just test` after code changes.
- Ran `just test-infra` when changing ignored worker/queue/DB integration paths and the required external services are available.
- Ran `just services-up && just test` when changing Qdrant/TEI/Chrome-backed integration behavior.
- Ran `just verify` before opening/updating PR.
