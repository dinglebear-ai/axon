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

### Integration suite lane (infra-backed, skip-on-missing)

A separate set of integration tests targets live Qdrant instances and other external services.
These tests do **not** use `#[ignore]` — instead each test calls a resolver (`resolve_test_amqp_url()`, etc.)
that returns `None` and exits cleanly when the corresponding env var is unset.
This means they run in `just test` without error, but only exercise real I/O when infra is available.

Start the required services explicitly, then run the full suite normally:

```bash
just services-up      # Qdrant, TEI, Chrome from config/docker-compose.services.yaml
just test
```

Integration suites currently covered:

| File | Env var required | What it tests |
|------|-----------------|---------------|
| `crates/web/execute/tests/ws_protocol_tests.rs` | none | WebSocket protocol frame serialization/deserialization |

## Test Infrastructure Environment Variables

Set these in `.env`:

| Variable | Default (test containers) | Purpose |
|----------|--------------------------|---------|
| `AXON_TEST_QDRANT_URL` | `http://127.0.0.1:53335` | Qdrant integration tests |

The tracked local compose file is `config/docker-compose.services.yaml`. It
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

Integration tests under `tests/` cover the ACP and services layer end-to-end:

| File | Tests | What is covered |
|------|-------|----------------|
| `tests/services_acp_spawn_env.rs` | 4 | `spawn_adapter()` env-stripping regression (see below) |
| `tests/services_acp_lifecycle.rs` | 11 | ACP session lifecycle (start, query, cancel, shutdown) |
| `tests/services_acp_event_mapping.rs` | 12 | ACP event type mapping and serialization |
| `tests/services_acp_security.rs` | 12 | ACP security model (SEC-7 session-scoped permission routing) |
| `tests/services_acp_smoke.rs` | 3 | ACP compile-time smoke tests |
| `tests/services_acp_bridge_event_serialize.rs` | 7 | Bridge event serialization round-trips |
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
| `crates/web/execute/tests/acp_ws_event_tests.rs` | (inline) | ACP WS event types |
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
| `use-axon-acp-editor.test.ts` | `useAxonAcpEditor` hook — `<axon:editor>` XML block wiring to PlateJS |

## ACP Regression Tests (`spawn_adapter` env stripping)

`tests/services_acp_spawn_env.rs` covers a critical regression: `spawn_adapter()` must not leak
specific environment variables to the child `claude-agent-acp` process.

**Background:** When axon runs inside a Claude Code session, `CLAUDECODE=1` is set in the environment.
If inherited by the ACP adapter, the inner `claude` CLI detects a nested session and exits 1
("Claude Code cannot be launched inside another Claude Code session"), causing
"Query closed before response received" in Pulse Chat.
`OPENAI_BASE_URL`, `OPENAI_API_KEY`, and `OPENAI_MODEL` point at Axon's local LLM proxy — if
inherited, the claude/codex adapters would use the wrong endpoint and authentication scheme.

**Tests:**

| Test | What it asserts |
|------|----------------|
| `spawn_adapter_strips_claudecode_nested_session_guard` | `CLAUDECODE` is not present in child env |
| `spawn_adapter_strips_llm_proxy_vars` | `OPENAI_BASE_URL`, `OPENAI_API_KEY`, `OPENAI_MODEL` absent from child |
| `spawn_adapter_passes_through_gemini_auth_vars` | Gemini auth vars (`GOOGLE_*`) are NOT stripped |
| `spawn_adapter_strips_all_isolation_vars_together` | All isolation vars stripped in combination |

These tests use a process-level `Mutex` to serialize `std::env::set_var` / `remove_var` calls
(required in Rust 1.81+ where those functions are `unsafe`). The file-level `#![allow(unsafe_code)]`
annotation is intentional — these are the only tests in the codebase that require it.

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
- `crates/services/acp/session_cache.rs` has dummy ACP handles and responder
  maps inside its test module so cache eviction and replay-buffer behavior can
  be tested without spawning real adapters.

These patterns are acceptable only because they are compile-time test scoped.
New tests that need a bypass should keep it behind `#[cfg(test)]` or a dedicated
test-helper feature, and production paths should continue to go through the
normal SSRF and ACP validation boundaries.

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
- `mcp-smoke` job: builds the release binary, starts `config/docker-compose.services.yaml` infra plus a CPU TEI container, and runs `scripts/test-mcp-tools-mcporter.sh`.
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

### `spawn_adapter` ACP tests failing on "unsafe_code"
- Cause: Whole-crate `deny(unsafe_code)` applies before the file-level `allow`.
- Fix: ensure `#![allow(unsafe_code)]` is present at the top of `tests/services_acp_spawn_env.rs` (not inside a module). This is intentional — do not remove it.

## Pull Request Checklist (Testing)
- Ran `just test` after code changes.
- Ran `just test-infra` when changing ignored worker/queue/DB integration paths and the required external services are available.
- Ran `just services-up && just test` when changing Qdrant/TEI/Chrome-backed integration behavior.
- Ran `just verify` before opening/updating PR.
