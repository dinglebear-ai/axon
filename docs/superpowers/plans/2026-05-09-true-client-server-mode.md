# True Client/Server Mode Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `axon serve` the authoritative execution and persistence boundary for stateful CLI operations, with the host CLI acting as an authenticated HTTP client when server mode is configured.

**Architecture:** Add a generic CLI server-mode config and client module, expose first-party JSON routes under `axon serve`, and migrate stateful commands to dispatch through those routes. Business logic remains in `src/services/**`; CLI, MCP, and web routes stay transport/presentation layers.

**Tech Stack:** Rust, Tokio, axum, reqwest, serde, clap/env config, SQLite-backed Lite workers, MCP action schema, Beads issue tracker.

---

## Planning Inputs

Epic: `axon_rust-jxs` (`Implement server-owned CLI client mode`)

Child beads:
- `axon_rust-jxs.1` - Add generic CLI server-mode config and HTTP client
- `axon_rust-jxs.2` - Add first-party server action API under serve
- `axon_rust-jxs.4` - Standardize server-owned artifact references
- `axon_rust-jxs.3` - Migrate stateful CLI commands to server dispatch
- `axon_rust-jxs.5` - Document Docker/systemd server-client operation
- `axon_rust-jxs.6` - Add end-to-end server-client verification

Locked decisions:
- Do not solve this by sharing one mounted host/container data root as the client contract.
- Server/container owns stateful execution, job state, outputs, screenshots, and artifacts.
- Server mode must fail clearly on connection/auth/schema errors. No silent local fallback.
- Prefer first-party HTTP routes over implementing an MCP streamable-HTTP client in the CLI.
- Preserve the services-layer contract: `src/services/**` owns business logic.
- Client-facing responses use job/artifact handles and root-relative identifiers; absolute paths are display/debug only.
- Runtime env files such as `~/.axon/.env` and `~/.axon/config.toml` are not edited by this plan.

## Research And Review Feedback Incorporated

Lavra research:
- Existing server mode is ask-only: `Config.server_url`, `--server-url`, and `AXON_ASK_SERVER_URL` currently target `/v1/ask`.
- `src/web/server.rs` already proves `axon serve` can host JSON routes, but `/v1/ask` is too narrow and uses an ask-specific auth helper.
- MCP already has typed `AxonRequest` action routing in `src/mcp/schema.rs` and `src/mcp/server.rs`; reuse its schema/dispatch shape where practical, but do not put MCP JSON-RPC/SSE in the CLI.
- `ServiceContext::new_with_workers()` is the long-lived worker-bearing server boundary; server-mode CLI must not spawn local workers for `--wait true`.
- Artifact code already validates roots and traversal in `src/mcp/server/artifacts/path.rs`, but current payloads can expose absolute paths. The client contract must be handle based.
- Use the existing shared HTTP client discipline from `src/core/http/client.rs`; do not create fresh `reqwest::Client::new()` calls per request.

Design pass:
- Add explicit config first: `AXON_SERVER_URL`, generic `--server-url`, and explicit local override. Keep `AXON_ASK_SERVER_URL` only as a compatibility alias for `ask`.
- Start with explicit URL mode. Do not add localhost auto-discovery in this epic.
- Add a `/v1/actions`-style first-party route with typed request/response envelopes and a `/v1/capabilities` or equivalent schema/version handshake before broad command migration.
- Centralize token attachment, cleartext bearer refusal, status decoding, JSON decoding, timeout handling, and user hints in one CLI client module.
- Keep local filesystem inputs honest. If a command requires reading a host-local path that the server cannot see, either design upload/import explicitly or fail with a clear message in server mode.

CEO review:
- Keep scope focused on the real user outcome: one authoritative state store for CLI, MCP, and Docker/systemd operations.
- Do not expand this into a full public REST API. Build a private first-party control API with narrow action coverage and documented compatibility guarantees.
- Add operator-grade failure visibility: version mismatch, auth failure, dead server, unsupported action, and local-mode override must all be obvious in CLI output and logs.
- Add diagrams and rollout docs so future operators understand why host-local markdown no longer appears when server mode is active.

Engineering review:
- Do not copy `/v1/ask` auth as-is for `/v1/actions`; route auth must share the MCP HTTP auth/scope policy where possible.
- Add schema/capability negotiation to catch stale server/binary drift before dispatching stateful actions.
- Split implementation by file-conflict boundaries: config/client, server API, handles, command migration, docs, E2E.
- Test ownership, not only success: prove server-mode scrape does not create host-local output as source of truth.
- Be explicit that `--wait true` waits on server job state rather than local in-process workers.

## File Structure

Create:
- `src/cli/client.rs` - generic server-mode HTTP client, request envelope, error type, token handling, cleartext guard.
- `src/cli/client/tests.rs` - unit tests for auth headers, cleartext guard, error classification, version/capability handling.
- `src/web/actions.rs` - first-party `/v1/actions` and capability route handlers, using shared service dispatch.
- `src/web/actions/tests.rs` - route auth, request validation, error envelope, capability tests.
- `src/services/action_api.rs` - transport-neutral first-party action dispatcher if sharing MCP handler logic directly would duplicate business rules.
- `src/services/types/client_server.rs` - typed action envelope, response envelope, error envelope, artifact handle types.
- `tests/client_server_mode.rs` - integration tests with mock server and optional live-server helpers.
- `scripts/test-client-server-mode.sh` - opt-in smoke script if shell smoke is kept outside Rust tests.

Modify:
- `src/core/config/cli/global_args.rs` - generic `--server-url`, explicit local flag/env.
- `src/core/config/types/config.rs` - generic server URL/client-mode fields and debug redaction.
- `src/core/config/types/config_impls.rs` - defaults and debug formatting.
- `src/core/config/parse/build_config/config_literal.rs` - config precedence and compatibility alias handling.
- `src/core/config/parse/build_config/tests/priority_chain/ask.rs` or new client-mode tests - precedence, alias, invalid URL.
- `src/lib.rs` - intercept stateful commands before local execution when server mode is active.
- `src/web/server.rs` - mount action routes and shared state.
- `src/mcp/auth.rs`, `src/mcp/server/http.rs`, or a new shared auth helper - reusable auth/scope check for first-party routes.
- `src/mcp/schema.rs` - reuse or wrap `AxonRequest` for first-party route schema.
- `src/mcp/server/artifacts/**` - produce stable handles and root-relative IDs.
- `src/services/{crawl,scrape,screenshot,system,embed,extract,ingest}.rs` - ensure stateful results expose handles needed by clients.
- `src/cli/commands/{status,crawl,scrape,embed,extract,ingest,sessions,screenshot}.rs` - server-mode dispatch/rendering.
- `.env.example`, `README.md`, `docs/CONFIG.md`, `docs/OPERATIONS.md`, `docs/SETUP.md`, `docs/TESTING.md`, `docs/commands/*.md`, `docs/mcp/*.md` - docs and examples.
- `Justfile` - optional smoke target.

Do not modify:
- `~/.axon/.env`
- `~/.axon/config.toml`
- Any unrelated runtime/service files outside the plan scope.

## Task 1: Generic Server-Mode Config And CLI Client

**Bead:** `axon_rust-jxs.1`

**Files:**
- Create: `src/cli/client.rs`
- Create: `src/cli/client/tests.rs`
- Modify: `src/cli.rs` or `src/cli/mod.rs` to expose the module
- Modify: `src/core/config/cli/global_args.rs`
- Modify: `src/core/config/types/config.rs`
- Modify: `src/core/config/types/config_impls.rs`
- Modify: `src/core/config/parse/build_config/config_literal.rs`
- Modify/Test: `src/core/config/parse/build_config/tests/priority_chain/ask.rs`
- Modify: `src/cli/commands/ask.rs`
- Modify/Test: `src/cli/commands/ask/ask_via_server_tests.rs`

- [ ] **Step 1: Write config precedence tests**

Add tests covering:
- `AXON_SERVER_URL` populates generic server URL.
- CLI `--server-url` overrides `AXON_SERVER_URL`.
- `AXON_ASK_SERVER_URL` remains a compatibility alias for `ask` only when generic URL is unset.
- Explicit local mode bypasses server URL for stateful commands.
- Malformed generic URL reports `invalid --server-url / AXON_SERVER_URL`.

Run: `cargo test server_url --lib`

Expected: tests fail because generic config does not exist yet.

- [ ] **Step 2: Add generic config fields**

Add fields to `Config`:
- `server_url: Option<reqwest::Url>` becomes generic, with docs updated away from ask-only wording.
- `client_mode: ClientMode` or equivalent with `Local`, `Server`, and `Auto` only if `Auto` is implemented as explicit config behavior. Recommended for this epic: support `Local` and `Server`, keep `Auto` out.
- `local_mode: bool` or an enum variant driven by `--local`.

Update all `Config { .. }` literals and `Config::test_default()`. Remember this repo’s gotcha: new non-`Option` fields only fail at test compile time, not `cargo check`.

Run: `cargo test config --lib`

Expected: config tests pass.

- [ ] **Step 3: Create the generic client module**

Implement `src/cli/client.rs` with:
- `ServerClient::new(base_url: reqwest::Url)`.
- `ServerClient::post_action<T, R>(&self, request: &T) -> Result<R, ServerClientError>`.
- token attachment from `AXON_MCP_HTTP_TOKEN`.
- cleartext bearer refusal for non-loopback `http://` unless a generic explicit override is set.
- shared timeout constant suitable for long jobs plus separate polling timeout.
- schema/version mismatch error classification.
- no per-call `reqwest::Client::new()`; use `crate::core::http::http_client()` where possible or the existing `build_client()` pattern only if timeout requirements force a dedicated client.

Run: `cargo test cli::client --lib`

Expected: token, cleartext, and error tests pass.

- [ ] **Step 4: Move ask to the generic client**

Keep `/v1/ask` behavior working, but remove duplicated token/error code from `src/cli/commands/ask.rs` where it can use the new client helper.

Run: `cargo test ask_via_server --lib`

Expected: existing ask server tests still pass, including cleartext-token guard.

- [ ] **Step 5: Commit**

```bash
git add src/cli src/core/config
git commit -m "feat(cli): add generic server client config"
```

## Task 2: First-Party Server Action API

**Bead:** `axon_rust-jxs.2`

**Files:**
- Create: `src/web/actions.rs`
- Create: `src/web/actions/tests.rs`
- Create: `src/services/action_api.rs`
- Create: `src/services/types/client_server.rs`
- Modify: `src/web.rs`
- Modify: `src/web/server.rs`
- Modify: `src/mcp/auth.rs`
- Modify: `src/mcp/server.rs`
- Modify: `src/mcp/schema.rs`
- Modify: `tests/mcp_contract_parity.rs`

- [ ] **Step 1: Write failing route tests**

Test:
- `GET /v1/capabilities` returns binary version, schema version, supported actions, and minimum client fields.
- `POST /v1/actions` rejects missing/invalid auth when token auth is enabled.
- `POST /v1/actions` rejects unknown action with JSON error, not HTML.
- `POST /v1/actions` can dispatch `status` using a test service context.

Run: `cargo test web::actions --lib`

Expected: tests fail because routes do not exist.

- [ ] **Step 2: Add typed envelopes**

Create envelope types:
- `ClientActionRequest { request_id, action }` where `action` wraps/reuses `AxonRequest`.
- `ClientActionResponse { request_id, ok, result, error, server }`.
- `ClientActionError { kind, message, retryable, hint }`.
- `ServerInfo { version, schema_version, supported_actions }`.

Do not return MCP `CallToolResult` strings as the first-party API contract.

Run: `cargo test client_server --lib`

Expected: serialization tests pass.

- [ ] **Step 3: Implement dispatch without business-logic duplication**

Preferred shape:
- `src/services/action_api.rs` maps `AxonRequest` to existing service functions for first-party routes.
- MCP handlers may continue to render MCP response modes, but shared validation and service calls should be extracted where duplication would otherwise grow.
- Preserve `ServiceContext::new_with_workers()` through server state so queued work wakes server workers.

Run: `cargo test services_action_api --lib`

Expected: status and one job lifecycle action dispatch through services.

- [ ] **Step 4: Mount routes under `axon serve`**

In `src/web/server.rs`, mount:
- `GET /v1/capabilities`
- `POST /v1/actions`

Use route-specific body limits. Keep `/v1/ask` for compatibility.

Run: `cargo test web::server --lib`

Expected: existing `/v1/ask` tests and new action tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/web src/services src/mcp tests
git commit -m "feat(server): add first-party action API"
```

## Task 3: Stable Server-Owned Artifact Handles

**Bead:** `axon_rust-jxs.4`

**Files:**
- Modify: `src/mcp/server/artifacts.rs`
- Modify: `src/mcp/server/artifacts/path.rs`
- Modify: `src/mcp/server/artifacts/lifecycle.rs`
- Modify: `src/mcp/server/artifacts/respond.rs`
- Modify: `src/services/types/service.rs`
- Modify/Create: `src/services/types/client_server.rs`
- Modify: `src/services/crawl.rs`
- Modify: `src/services/scrape.rs`
- Modify: `src/services/screenshot.rs`
- Modify: `src/cli/commands/{crawl,scrape,screenshot}.rs`

- [ ] **Step 1: Write handle contract tests**

Test:
- A server response includes `artifact_handle.relative_path` or equivalent.
- Absolute `display_path` is present only as display/debug metadata.
- Artifact read/head/grep reject traversal and symlink escapes.
- Container-style absolute paths are not required for client follow-up calls.

Run: `cargo test artifacts --lib`

Expected: tests fail until handles are added.

- [ ] **Step 2: Define artifact handle types**

Add a type with fields equivalent to:
- `kind`
- `relative_path`
- `display_path`
- `bytes`
- `line_count`
- `job_id`
- `url`

Keep absolute paths out of machine-readable follow-up identifiers.

Run: `cargo test client_server --lib`

Expected: handle serialization tests pass.

- [ ] **Step 3: Update artifact/list/read responses**

Change artifact listing/search/read payloads to include root-relative handles. Keep existing display fields only where compatibility requires them.

Run: `cargo test artifacts --lib`

Expected: traversal and listing tests pass.

- [ ] **Step 4: Update scrape/crawl/screenshot service results**

Ensure stateful output-producing services return handles or enough metadata for the first-party route to produce handles.

Run: `cargo test scrape crawl screenshot --lib`

Expected: output metadata tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/mcp/server/artifacts src/services src/cli/commands
git commit -m "feat(server): return portable artifact handles"
```

## Task 4: Migrate Stateful CLI Commands To Server Dispatch

**Bead:** `axon_rust-jxs.3`

**Files:**
- Modify: `src/lib.rs`
- Modify: `src/cli/commands/status.rs`
- Modify: `src/cli/commands/crawl.rs`
- Modify: `src/cli/commands/crawl/subcommands.rs`
- Modify: `src/cli/commands/scrape.rs`
- Modify: `src/cli/commands/embed.rs`
- Modify: `src/cli/commands/extract.rs`
- Modify: `src/cli/commands/ingest.rs`
- Modify: `src/cli/commands/sessions.rs`
- Modify: `src/cli/commands/screenshot.rs`
- Modify/Create tests beside each changed command.

- [ ] **Step 1: Add dispatch decision tests**

Test:
- With `AXON_SERVER_URL`, stateful commands use `ServerClient`.
- With explicit local mode, stateful commands use existing local paths.
- Dead server fails before any local scrape/crawl/write begins.
- Query-only commands remain local unless explicitly migrated.

Run: `cargo test client_server_dispatch --lib`

Expected: tests fail until dispatch is implemented.

- [ ] **Step 2: Intercept stateful commands before local execution**

In `src/lib.rs`, route these commands through server mode before creating local worker-bearing behavior:
- `status`
- `scrape`
- `crawl` and lifecycle subcommands
- `extract`
- `embed`
- `ingest`
- `sessions`
- `screenshot`
- artifact commands if exposed through CLI

Do not let server-mode `--wait true` spawn local workers. It must submit to the server and poll server job state.

Run: `cargo test client_server_dispatch --lib`

Expected: dispatch tests pass.

- [ ] **Step 3: Implement command rendering from server responses**

Preserve existing human/JSON output where practical:
- `--json` prints response result JSON, not the full internal envelope unless explicitly requested.
- human output keeps current command language but indicates server mode on errors.
- auth failure hint says token mismatch.
- dead server hint says start `axon serve` or use explicit local mode.
- schema mismatch hint says rebuild/restart the canonical server.

Run: `cargo test cli --lib`

Expected: command rendering tests pass.

- [ ] **Step 4: Handle host-local inputs explicitly**

For `embed <path>` and any command that needs a local file:
- If the server can read the same path only by accident, do not assume it.
- Either add a first-party upload/import step in this task or fail clearly with a message that server mode does not accept host-local paths yet.
- Keep URL/text inputs working through server mode.

Run: `cargo test embed_server_mode --lib`

Expected: local-path server-mode behavior is explicit and tested.

- [ ] **Step 5: Commit**

```bash
git add src/lib.rs src/cli/commands
git commit -m "feat(cli): route stateful commands through server mode"
```

## Task 5: Operational Docs And Config Examples

**Bead:** `axon_rust-jxs.5`

**Files:**
- Modify: `.env.example`
- Modify: `README.md`
- Modify: `docs/CONFIG.md`
- Modify: `docs/OPERATIONS.md`
- Modify: `docs/SETUP.md`
- Modify: `docs/TESTING.md`
- Modify: `docs/commands/*.md`
- Modify: `docs/mcp/CONNECT.md`
- Modify: `docs/mcp/ENV.md`
- Modify: `docker-compose.yaml` only if comments/examples need updates
- Modify: `scripts/plugin-setup.sh` only if generated service env docs need generic server URL comments

- [ ] **Step 1: Write docs grep checks or manual checklist**

Check:
- `AXON_SERVER_URL` appears as primary server-mode setting.
- `AXON_ASK_SERVER_URL` appears only as compatibility/deprecated ask wording.
- Docs explain explicit local mode.
- Docs explain why server-mode scrape does not create host-local markdown.
- Docs warn that bearer tokens over non-loopback plaintext HTTP are refused by default.

Run: `rg "AXON_SERVER_URL|AXON_ASK_SERVER_URL|server mode|local mode" docs README.md .env.example`

Expected: current docs show ask-only language and missing generic mode.

- [ ] **Step 2: Update config examples**

Add blank/commented generic examples:
```dotenv
AXON_SERVER_URL=
# AXON_SERVER_URL=http://127.0.0.1:8001
```

Do not edit runtime env files in `~/.axon`.

- [ ] **Step 3: Update Docker/systemd operation docs**

Document:
- Docker/server owns stateful operations.
- Host CLI uses `AXON_SERVER_URL`.
- `AXON_MCP_HTTP_TOKEN` is required for non-loopback published topologies.
- `~/.axon` remains the canonical server appdata root, but client APIs use handles, not absolute paths.
- To recover from stale runtime drift, rebuild, restart the canonical server on port `8001`, and verify `which -a axon`, `axon --version`, and the live listener owner.

- [ ] **Step 4: Commit**

```bash
git add .env.example README.md docs docker-compose.yaml scripts/plugin-setup.sh
git commit -m "docs: document axon client server mode"
```

## Task 6: End-To-End Verification

**Bead:** `axon_rust-jxs.6`

**Files:**
- Create: `tests/client_server_mode.rs`
- Create: `scripts/test-client-server-mode.sh` if using shell smoke
- Modify: `Justfile`
- Modify: `docs/TESTING.md`

- [ ] **Step 1: Add mock-server integration tests**

Test:
- `axon status` in server mode calls the server and renders the response.
- `axon scrape --json` sends first-party action JSON with auth header.
- dead server returns a clear error and does not run local scrape.
- explicit local mode bypasses the mock server.

Run: `cargo test client_server_mode`

Expected: tests pass.

- [ ] **Step 2: Add ownership proof test**

Use a temp host output directory and a mock/live server:
- run server-mode scrape
- assert no host-local scrape markdown becomes source of truth
- assert response includes server-owned handle
- assert follow-up artifact read uses handle

Run: `cargo test client_server_mode::server_mode_scrape_uses_server_owned_output`

Expected: test passes.

- [ ] **Step 3: Add optional live smoke**

Add `just client-server-smoke` only if it can run without corrupting local runtime config. The script must use temporary env overrides and must not edit `~/.axon/.env` or `~/.axon/config.toml`.

Smoke commands:
```bash
AXON_SERVER_URL=http://127.0.0.1:8001 axon status --json
AXON_SERVER_URL=http://127.0.0.1:8001 axon scrape https://example.com --json --max-pages 1
AXON_SERVER_URL=http://127.0.0.1:8001 axon crawl status <job_id> --json
```

Run: `just client-server-smoke`

Expected: server-mode calls hit the canonical server and outputs are server-owned.

- [ ] **Step 4: Full gate**

Run:
```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add tests scripts Justfile docs/TESTING.md
git commit -m "test: verify axon client server mode"
```

## Rollout Order

1. `axon_rust-jxs.1` config/client foundation.
2. `axon_rust-jxs.2` first-party server action API.
3. `axon_rust-jxs.4` artifact/output handles.
4. `axon_rust-jxs.3` stateful command migration.
5. `axon_rust-jxs.5` docs/config examples.
6. `axon_rust-jxs.6` E2E verification and smoke target.

Do not parallelize tasks that touch the same files unless the implementer first splits the file scopes further. The highest-conflict files are `src/lib.rs`, `src/web/server.rs`, `src/mcp/schema.rs`, `src/core/config/types/config.rs`, and `src/cli/commands/*`.

## Failure Modes To Preserve In Tests

| Codepath | Failure | Required Behavior |
| --- | --- | --- |
| server-mode dispatch | server unreachable | fail clearly; do not run local command |
| auth | missing/wrong token | fail closed with token hint |
| cleartext bearer | remote `http://` URL | refuse unless explicit insecure override |
| schema handshake | stale server/client | fail with rebuild/restart/version hint |
| `--wait true` | server job failure | show server job error; do not spawn local worker |
| artifact read | traversal/symlink escape | reject before read |
| host-local embed path | server cannot see path | fail clearly or upload explicitly |

## Final Verification Checklist

- [ ] `bd show axon_rust-jxs` points to this plan.
- [ ] `cargo test client_server_mode` passes.
- [ ] `cargo test ask_via_server` passes.
- [ ] `cargo test artifacts` passes.
- [ ] `cargo test config` passes.
- [ ] Server-mode scrape does not create host-local markdown as source of truth.
- [ ] Explicit local mode still works.
- [ ] Runtime env files under `~/.axon` remain untouched by implementation.
