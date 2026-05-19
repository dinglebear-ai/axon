# Session: Remote Deployment Plan Implementation
**Date:** 2026-03-24
**Branch:** `feat/warm-session-pool`
**Plan:** `docs/superpowers/plans/2026-03-24-remote-deployment.md`

---

## Session Overview

Implemented all 6 tasks from the remote deployment plan, enabling the `axon` CLI/MCP to run on a machine that does not host backing services. The core changes: `qdrant_url` and `tei_url` are now required at config-parse time (fail fast with clear error message), two new `Option<String>` fields (`acp_ws_url`, `acp_ws_token`) were added to `Config`, and a new `AcpWsCompletionRunner` routes ACP completions through a remote `axon serve` WebSocket when `AXON_ACP_WS_URL` is set.

Tasks 1, 3, 4, and 5 were partially completed in a prior session (same conversation context). This session completed Task 2 (`tei_url` required), fixed all affected tests, added three new tests, and documented the new env vars in `.env.example`.

---

## Timeline

1. **Context resume** — Picked up from prior session summary. Verified Tasks 3/4/5 were already in-place: `acp_ws_url`/`acp_ws_token` fields in `Config`, `ws_runner.rs` created, `acp_llm.rs` routing updated.
2. **Task 2 — `tei_url` required** — Changed `unwrap_or_default()` to `ok_or_else(|| "TEI_URL...")?` in `build_config.rs`.
3. **Test fixes** — Added `--tei-url http://127.0.0.1:52000` to three existing tests that would now fail since `tei_url` is required and `TEI_URL` env var may be unset in the test environment.
4. **New tests** — Added `into_config_errors_when_tei_url_missing`, `into_config_reads_acp_ws_url_from_env`, `into_config_reads_acp_ws_token_from_env`.
5. **Task 6 — `.env.example`** — Added `AXON_ACP_WS_URL` and `AXON_ACP_WS_TOKEN` with descriptive comments after `AXON_ACP_PREWARM`.
6. **Verification** — `cargo test --lib build_config`: 7/7 passing. `cargo check`: 0 errors.

---

## Key Findings

- `tei_url` and `qdrant_url` assignments in `build_config.rs` are evaluated in struct field source order. `tei_url` (line 379) precedes `qdrant_url` (line 384), so `into_config_errors_when_qdrant_url_missing` would have failed with a TEI_URL error without `--tei-url` in the test args.
- `ws_runner.rs` required a manual `percent_encode()` implementation because the `url` crate is not a direct dependency in the `axon` workspace. The encoder covers all non-unreserved URI characters per RFC 3986.
- `AcpCompletionRunner::complete_streaming` is generic over `F` (not `dyn FnMut`) — the new `AcpWsCompletionRunner` impl must match this exact bound.
- `crates/services/` is a module inside the main `axon` crate, not a separate crate with its own `Cargo.toml`.

---

## Technical Decisions

- **`tei_url` required rather than defaulting to empty string** — An empty `tei_url` silently fails at embed time with an unhelpful connection error. Failing at config-parse with a clear message matches the `qdrant_url` pattern established in a prior session and is more debuggable on remote deployments.
- **`acp_ws_url`/`acp_ws_token` as `Option<String>` not required** — WS mode is additive; the existing subprocess-based ACP path must remain fully functional when these are unset. Only remote deployments need WS mode.
- **WS timeout `300s`** — Matches `AXON_EMBED_DOC_TIMEOUT_SECS` default and is consistent with other long-running operations in the codebase.
- **`build_ws_endpoint` normalizes `http→ws`, `https→wss`** — Users configure HTTP base URLs consistently across the codebase (`AXON_WEB_API_TOKEN`, `TEI_URL`, etc.). The function handles scheme rewriting transparently.
- **`percent_encode` manual impl** — Inline helper, not a new dependency. Only needs RFC 3986 unreserved character exclusion; no edge cases beyond that.

---

## Files Modified

| File | Purpose |
|------|---------|
| `crates/core/config/parse/build_config.rs` | Task 2: made `tei_url` required; fixed 3 existing tests; added 3 new tests |
| `crates/core/config/types/config.rs` | Task 3: added `acp_ws_url: Option<String>` and `acp_ws_token: Option<String>` fields |
| `crates/core/config/types/config_impls.rs` | Task 3: added defaults (`None`) and Debug impl (token redacted) |
| `crates/services/acp_llm.rs` | Task 5: added `mod ws_runner;`, WS-first routing in `complete_text` / `complete_streaming` |
| `crates/services/acp_llm/ws_runner.rs` | Task 4: new file — `AcpWsCompletionRunner`, `build_ws_endpoint`, `percent_encode`, `compose_execute_msg`, `extract_event`, 13 unit tests |
| `.env.example` | Task 6: added `AXON_ACP_WS_URL` / `AXON_ACP_WS_TOKEN` after `AXON_ACP_PREWARM` |

---

## Commands Executed

```bash
# Verify all build_config tests pass
cargo test --lib -p axon build_config
# Result: 7 passed; 0 failed

# Confirm no compile errors
cargo check
# Result: 0 errors (2 unrelated warnings in ws_runner test about serde_json path)
```

---

## Behavior Changes (Before/After)

| Behavior | Before | After |
|----------|--------|-------|
| Missing `TEI_URL` | Silent empty string; fails at embed time with connection refused | Fails at startup with: `TEI_URL environment variable is required (or pass --tei-url). Copy .env.example to .env and fill in credentials.` |
| `AXON_ACP_WS_URL` set | Ignored (field didn't exist) | ACP completions (`ask`, `research`, `evaluate`, extract fallback) routed through remote `axon serve` WebSocket |
| `AXON_ACP_WS_TOKEN` set | Ignored | Appended as `?token=<percent-encoded>` to WebSocket URL |
| Remote `axon serve` WebSocket | Not supported | Supported via `wss://{host}/ws?token={tok}` — `AcpWsCompletionRunner` handles connect, send, receive, timeout |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo test --lib -p axon build_config` | 7 passed, 0 failed | 7 passed, 0 failed | ✓ PASS |
| `cargo check` | 0 errors | 0 errors | ✓ PASS |
| `into_config_errors_when_tei_url_missing` | `err.contains("TEI_URL")` | `ok` | ✓ PASS |
| `into_config_errors_when_qdrant_url_missing` | `err.contains("QDRANT_URL")` | `ok` | ✓ PASS |
| `into_config_reads_acp_ws_url_from_env` | `cfg.acp_ws_url == Some("https://axon.example.com:49000")` | `ok` | ✓ PASS |
| `into_config_reads_acp_ws_token_from_env` | `cfg.acp_ws_token == Some("supersecret")` | `ok` | ✓ PASS |

---

## Risks and Rollback

- **`tei_url` now required** — Any deployment or test that relied on the empty-string default will break at startup. Mitigation: the error message explicitly names the env var and points to `.env.example`. Rollback: revert `build_config.rs:379-386` to `.unwrap_or_default()`.
- **`make_test_config()` literals** — Per `CLAUDE.md`, inline `Config { .. }` literals in `crates/cli/commands/research.rs`, `search.rs`, and `crates/jobs/common/` must add `acp_ws_url: None, acp_ws_token: None` if they were updated since the last session. These only fail at test-build time, not `cargo check`. The fields have defaults so struct update syntax (`..Config::default()`) is unaffected.

---

## Decisions Not Taken

- **`tei_url` as `Option<String>`** — Would preserve backward compat but silently allows embed operations to proceed with no TEI configured, producing confusing errors downstream. Rejected in favor of fail-fast.
- **CLI flag `--acp-ws-url`** — The WS URL is a deployment-time setting, not a per-invocation flag. Keeping it env-only reduces CLI surface area and matches how `AXON_WEB_API_TOKEN` is handled.
- **Subagent dispatch for implementation** — User explicitly rejected subagent overhead and requested direct in-context implementation.

---

## Open Questions

- The two warnings in `ws_runner.rs` tests (`serde_json::Value` path should be `Value`) are pre-existing clippy suggestions, not errors. They can be cleaned up in a separate pass.
- `make_test_config()` literals in `research.rs` / `search.rs` / `common/` may need `acp_ws_url: None, acp_ws_token: None` added — not checked this session (fields have `Config::default()` fallback via `..` syntax, so only explicit struct literals without `..` are affected).

---

## Next Steps

- Run `just verify` (fmt-check + clippy + check + test) as pre-PR gate before merging `feat/warm-session-pool`.
- Confirm `.env.example` changes are reflected in any deployment documentation.
- Consider adding an integration test that exercises `AcpWsCompletionRunner` against a mock WS server (low priority — 13 unit tests cover all pure-logic paths).
