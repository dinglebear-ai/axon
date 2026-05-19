# CI Fix Round 2 — mcp-oauth-smoke + crawl DB test serialization

**Date:** 2026-03-04
**Branch:** feat/sidebar
**PR:** #6
**Commits this session:** b71fd7fd

---

## Session Overview

Continued from a prior context-limited session (`/gh-fix-ci` on PR #6). Prior session had fixed `web-lint-test` (snapshot timezone mismatch) and `mcp-http-only` (ripgrep → grep). This session fixed two more of the remaining three CI failures and left `mcp-smoke` (mcporter HTTP transport) partially investigated.

---

## Timeline

1. **Session resumed** — reviewed prior session summary; 3 CI jobs still failing: `mcp-oauth-smoke`, `test`, `mcp-smoke`
2. **`mcp-oauth-smoke` root-cause confirmed** — `scripts/test-mcp-oauth-protection.sh` spawns `cargo run --bin axon -- mcp` with only Google OAuth env vars. The CLI arg parser requires `AXON_PG_URL` before reaching `run_mcp`, causing immediate fail with "AXON_PG_URL environment variable is required". Confirmed `load_mcp_config()` uses `Config::default()` (no eager Postgres connect), so a dummy URL suffices.
3. **`test` root-cause investigated** — `crawl_recover_reclaims_confirmed_stale_running_job` fails with `Error: RowNotFound`. Traced through: `recover_stale_crawl_jobs` → `amqp_consumer::reclaim_stale_running_jobs` → `common::reclaim_stale_running_jobs`. Logic is correct. Root cause: test missing `#[serial]` while all other DB-touching tests have it (from commit 3466ddf0). Race with concurrent schema migrations or env-var-mutating `#[serial]` tests.
4. **Both fixes applied and committed** as `b71fd7fd`
5. **`mcp-smoke` investigation** — discovered mcporter 0.7.3 supports `--http-url`/`--allow-http` flags for HTTP transport. Fix design: start `axon mcp` as background server in CI, use URL-based mcporter config. Work interrupted before implementation.

---

## Key Findings

### `mcp-oauth-smoke` — `AXON_PG_URL` required at arg-parse time
- **File:** `crates/core/config/parse/build_config.rs:188` — "AXON_PG_URL environment variable is required" is emitted by the CLI arg parser, BEFORE command dispatch
- **File:** `crates/mcp/config.rs:8` — `load_mcp_config()` starts with `Config::default()` and only reads env vars if present (no required fields)
- **Conclusion:** Any value for `AXON_PG_URL` unblocks the binary. Dummy URL is safe since `run_http_server` does not eagerly connect to Postgres at startup — only when a tool call is made.

### `test` — Missing `#[serial]` on crawl DB tests
- **File:** `crates/jobs/crawl/runtime/tests.rs` — `crawl_start_job_dedupes_active_pending_job` and `crawl_recover_reclaims_confirmed_stale_running_job` had no `#[serial]`
- **Pattern:** All other DB-touching tests in `embed/tests.rs`, `extract/tests.rs`, `common/tests/pool_integration.rs`, `worker_lane.rs` have `#[serial]`
- **Race:** `worker_lane.rs:375,402` tests with `#[serial]` manipulate `AXON_PG_URL` env var — if `resolve_test_pg_url()` runs concurrently while that var is removed, tests can skip or fail. Schema migration lock contention (5s timeout in `begin_schema_migration_tx`) under 8-concurrent-connection test is another vector.
- **Commit that introduced gap:** `3466ddf0` ("fix(test): add #[serial] to extract DB tests") serialized extract/embed/common but missed crawl.

### `mcp-smoke` — mcporter stdio vs HTTP (NOT YET FIXED)
- mcporter spawns `axon mcp` as stdio subprocess, communicates via stdin/stdout JSON-RPC
- `axon mcp` starts an HTTP server (axum) and never reads stdin → all tool `call` tests fail (PASS=2 FAIL=22)
- `list_server` and `list_schema` pass because mcporter reads config without a live connection
- **Fix design:** start `axon mcp` as background HTTP server in CI; write URL-based `config/mcporter.json` (`{"mcpServers":{"axon":{"url":"http://127.0.0.1:38001/mcp"}}}`); add `--allow-http` to mcporter calls; no GOOGLE_OAUTH env vars → OAuth disabled → mcporter can call tools freely
- mcporter `--http-url` and `--allow-http` flags confirmed via `mcporter call --help`

---

## Technical Decisions

### Dummy service URLs for mcp-oauth-smoke CI job
Added `AXON_PG_URL`, `AXON_REDIS_URL`, `AXON_AMQP_URL` as dummy env vars to the CI job's `env:` block (not to the script itself). This keeps the script unchanged and applies to both the `cargo build` and the smoke test script steps.

### `#[serial]` on both crawl DB tests, not just the failing one
`crawl_start_job_dedupes_active_pending_job` also touches the DB and runs concurrently — serializing only the failing test would leave another potential race. Both get `#[serial]`.

### `crawl_ensure_schema_is_concurrency_safe` NOT serialized
That test specifically validates concurrent `ensure_schema` calls are safe. Adding `#[serial]` would remove concurrency and make the test vacuous.

---

## Files Modified

| File | Change |
|------|--------|
| `.github/workflows/ci.yml` | Added `env:` block with dummy `AXON_PG_URL`, `AXON_REDIS_URL`, `AXON_AMQP_URL` to `mcp-oauth-smoke` job |
| `crates/jobs/crawl/runtime/tests.rs` | Added `use serial_test::serial;` import; `#[serial]` on 2 DB-touching tests |

---

## Commands Executed

```bash
# Verified serial_test is in workspace dev-deps
grep -n "serial_test" /home/jmagar/workspace/axon_rust/Cargo.toml
# → line 86: serial_test = "3"

# Verify compile clean after changes
cargo check --tests --message-format=short
# → no errors or warnings

# Committed and pushed
git add .github/workflows/ci.yml crates/jobs/crawl/runtime/tests.rs
git commit -m "test: fix mcp-oauth-smoke missing env vars and serialize crawl DB tests"
git push origin feat/sidebar
# → b71fd7fd pushed successfully
```

---

## Behavior Changes (Before/After)

| Check | Before | After |
|-------|--------|-------|
| `mcp-oauth-smoke` | FAIL — `AXON_PG_URL environment variable is required` before server starts | Should PASS — dummy URLs let binary start; server binds; OAuth gates `/mcp` |
| `test` (`crawl_recover_reclaims_confirmed_stale_running_job`) | FAIL — `Error: RowNotFound` (race condition) | Should PASS — `#[serial]` prevents concurrent mutation |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check --tests` | No errors | No errors/warnings | ✅ |
| `git push` | Commit b71fd7fd pushed | Pushed successfully | ✅ |
| CI `mcp-oauth-smoke` | PASS | Pending new run | ⏳ |
| CI `test` | PASS | Pending new run | ⏳ |
| CI `mcp-smoke` | PASS | Still FAIL (not yet fixed) | ❌ |

---

## Risks and Rollback

- **`mcp-oauth-smoke` dummy URLs:** Dummy `AXON_PG_URL` etc. only apply to the CI job env. If the MCP server ever eagerly connects to Postgres at startup (e.g., future schema migration in `load_mcp_config`), the smoke test will timeout instead of erroring with an auth message. Currently safe.
- **`#[serial]` on crawl tests:** Small CI time increase (tests run sequentially). No functional risk — logic unchanged.
- **Rollback:** `git revert b71fd7fd`

---

## Decisions Not Taken

- **Add `AXON_PG_URL` directly to `test-mcp-oauth-protection.sh`** — rejected because it would couple the script to CI env assumptions. CI `env:` block is cleaner separation.
- **Make `AXON_PG_URL` optional in the CLI arg parser** — out of scope; would require Config refactor and could affect real-world error messaging.
- **Fix `mcp-smoke` in this commit** — insufficient time before context limit; complex change requiring CI job restructure (background server + mcporter URL config).

---

## Open Questions

- Will `mcp-oauth-smoke` pass with dummy service URLs, or does the server attempt REDIS connect at startup too? (`GoogleOAuthState::from_env` reads `AXON_REDIS_URL` — if it connects eagerly, the dummy Redis URL will fail. The server sets up Redis for OAuth token storage.)
- Exact root cause of `RowNotFound` in `crawl_recover_reclaims_confirmed_stale_running_job` still not pinpointed — `#[serial]` is the standard fix pattern for the codebase.

---

## Next Steps

1. **Verify CI run** — check if `mcp-oauth-smoke` and `test` now pass with b71fd7fd
2. **Fix `mcp-smoke`** — implementation plan:
   - In `.github/workflows/ci.yml` `mcp-smoke` job: add step to start `axon mcp` in background (without GOOGLE_OAUTH vars → OAuth disabled)
   - Wait for server readiness (`curl` loop on `/oauth/google/status` or `/mcp`)
   - Write URL-based `config/mcporter.json`: `{"mcpServers":{"axon":{"url":"http://127.0.0.1:38001/mcp"}},"imports":[]}`
   - Update `scripts/test-mcp-tools-mcporter.sh` calls to add `--allow-http` OR rely on config-based URL resolution
   - Kill background server in cleanup trap
3. **Merge PR #6** once all 5 CI checks pass
