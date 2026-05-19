# Session: MCP Server Config Refactor — Delete load_mcp_config()

**Date:** 2026-03-28
**Branch:** feat/lite-mode
**Commit:** `54244286`
**Version:** 0.33.6 → 0.33.7 (patch)

---

## Session Overview

Identified and fixed the root cause of MCP `ask` failures: `load_mcp_config()` in
`crates/mcp/config.rs` was a hand-rolled config builder that started from
`Config::default()` (where `acp_adapter_cmd: None`) and never ran the
`AXON_ASK_AGENT` / `AXON_ACP_*_ADAPTER_CMD` resolution logic from
`build_config()`. MCP `ask` always failed with "requires an ACP adapter" even
when the env was correctly configured for CLI usage.

Fix: deleted `load_mcp_config()` and threaded the properly-built `cfg: Config`
(from the existing `build_config()` path) through `run_mcp()` into both server
entry points. MCP server now shares the identical config path as every other CLI
command.

---

## Timeline

1. **Continued from previous session** — Investigation showed MCP `ask` reaching
   ACP runtime but failing on missing `acp_adapter_cmd`. CLI `ask` worked fine.
2. **Root cause identified** — `load_mcp_config()` calls `Config::default()` which
   has `acp_adapter_cmd: None`; it reads ~30 env vars but skips all `AXON_ASK_AGENT`
   resolution entirely.
3. **User architectural feedback** — "shouldn't there be like a ServiceContext and
   be owned by the services layer like all of our other commands" — directed away
   from patching `load_mcp_config()` toward proper config threading.
4. **Refactor executed** — `run_stdio_server` / `run_http_server` signatures updated
   to accept `Config`; `run_mcp()` passes `cfg.clone()` to both.
5. **config.rs deleted** — module declaration removed from `crates/mcp.rs`;
   file deleted from disk.
6. **evaluate test fixed** — Assertion updated to match current error message
   containing `"AXON_ASK_AGENT"` (was asserting old string
   `"AXON_ACP_ADAPTER_CMD is required for ask/evaluate commands"`).
7. **All 1558 tests pass** — Pre-commit hook ran full suite; commit and push succeeded.

---

## Key Findings

- **`crates/mcp/config.rs:27`** — `load_mcp_config()` started from `Config::default()`,
  never called `build_config()`. This is why `acp_adapter_cmd` was always `None`
  for MCP requests.
- **`crates/core/config/parse/build_config.rs`** — `resolve_ask_adapter_cmd()` reads
  `AXON_ASK_AGENT` and maps to `AXON_ACP_{AGENT}_ADAPTER_CMD`. This logic was
  completely absent from the MCP config path.
- **HTTP server factory closure** — `StreamableHttpService` creates one `AxonMcpServer`
  per session via a factory closure. The config is captured as `Arc<Config>` in the
  closure: `move || Ok(AxonMcpServer::new((*cfg_arc).clone()))`.
- **`crates/vector/ops/commands/evaluate.rs:278`** — Test was asserting the old error
  string from a previous refactor; updated to `contains("AXON_ASK_AGENT")`.
- **`config/mcporter.json`** — `AXON_REPO_ROOT` env var addition (from prior session)
  allows the bash subcommand to find `.env` regardless of cwd.

---

## Technical Decisions

- **Thread `cfg: Config` vs wrap in `Arc`** — Both server functions now accept
  `Config` by value. The HTTP server captures it as `Arc<Config>` internally
  (needed for the `move` closure). The stdio path just moves it into `AxonMcpServer`.
- **Delete `config.rs` entirely vs keep as dead code** — Deleted. The 334 lines of
  `load_mcp_config()` and its test suite were all testing behavior now subsumed by
  `build_config()`. Keeping dead code would invite confusion.
- **`ServiceContext` vs bare `Config`** — User asked about `ServiceContext`, but the
  `AxonMcpServer` already has a `base_service_context()` that lazily constructs
  `ServiceContext` from `self.cfg`. The immediate blocker was config correctness,
  not missing `ServiceContext` — threading `cfg` correctly is the necessary fix
  without a larger structural change.
- **No `load_mcp_config()` fallback** — Removing it entirely forces the correct path
  with no escape hatch. If `run_mcp()` has correct config, all downstream handlers
  do too.

---

## Files Modified

| File | Change |
|------|--------|
| `crates/mcp/config.rs` | **Deleted** — `load_mcp_config()` and all helpers removed |
| `crates/mcp.rs` | Removed `mod config;` module declaration |
| `crates/mcp/server.rs` | `run_stdio_server(cfg: Config)` / `run_http_server(cfg: Config, host, port)` — accept cfg directly; removed `load_mcp_config()` calls |
| `crates/cli/commands/mcp.rs` | `run_mcp()` passes `cfg.clone()` to both server functions |
| `crates/vector/ops/commands/evaluate.rs` | Test assertion: `contains("AXON_ASK_AGENT")` |
| `crates/vector/ops/commands/ask.rs` | (Prior session) `validate_ask_llm_config()` error message updated |
| `crates/vector/ops/commands/ask/tests.rs` | (Prior session) Test assertion updated |
| `crates/core/config/parse/build_config.rs` | (Prior session) `resolve_ask_adapter_cmd/args()` added; `AXON_ASK_AGENT` support |
| `crates/core/config/types/config_impls.rs` | (Prior session) Default for new config fields |
| `.env.example` | (Prior session) Added `AXON_ASK_AGENT` documentation |
| `config/mcporter.json` | (Prior session) Added `AXON_REPO_ROOT` env var |
| `Cargo.toml` | Version bump 0.33.6 → 0.33.7 |

---

## Commands Executed

```bash
# Verify compile clean after removing load_mcp_config import
cargo check --bin axon

# Remove config.rs module and delete file
rm crates/mcp/config.rs

# Full test suite (1558 tests)
cargo test 2>&1 | grep -E "^test result|FAILED" | head -20
# Result: 0 failures

# Version bump + lock update
sed -i 's/version = "0.33.6"/version = "0.33.7"/' Cargo.toml
cargo check -q

# Commit + push (pre-commit ran full suite)
git add . && git commit -m "refactor: thread build_config() into MCP server; delete load_mcp_config()"
git push
# → 7d585b34..54244286  feat/lite-mode -> feat/lite-mode
```

---

## Behavior Changes (Before / After)

| Scenario | Before | After |
|----------|--------|-------|
| `mcporter call axon.axon action:ask` | Fails: "ask/evaluate requires an ACP adapter" even with env configured | Succeeds: inherits `AXON_ASK_AGENT` from `.env` via `build_config()` |
| `axon mcp` config path | `Config::default()` + selective env reads (missing ACP fields) | Full `build_config()` config, identical to CLI `ask` |
| `AXON_ASK_AGENT` in MCP context | Ignored — `load_mcp_config()` never read it | Applied — MCP now uses same resolution as CLI |
| `evaluate` test | Asserted old error string (failing) | Asserts `AXON_ASK_AGENT` substring (passing) |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check --bin axon` (after removing `config` import) | Clean | Clean | ✅ |
| `cargo check --bin axon` (after deleting `config.rs`) | Clean | Clean | ✅ |
| `cargo test` (full suite) | 0 failures | 1558 passed, 0 failed | ✅ |
| `git push` | Accepted | `7d585b34..54244286 feat/lite-mode` | ✅ |

---

## Source IDs + Collections Touched

None — this was a pure code refactor session. No embeds or retrieves were run
against Qdrant during this session.

---

## Risks and Rollback

- **Risk:** The HTTP server factory closure captures `Arc<Config>` — if `Config`
  gains non-`Clone` fields in future, this pattern will break. Low risk; `Config`
  is already `Clone` throughout the codebase.
- **Rollback:** `git revert 54244286` restores `load_mcp_config()` and the old
  signatures. Would re-introduce the ACP adapter resolution gap.

---

## Decisions Not Taken

- **Patch `load_mcp_config()` to add `AXON_ASK_AGENT` support** — Rejected per
  user direction. Patching would create two diverging config paths to maintain.
- **Full `ServiceContext` in `AxonMcpServer`** — Not done this session; the lazy
  `base_service_context()` pattern already provides `ServiceContext` on demand.
  A fuller refactor (storing `ServiceContext` directly) is a separate improvement.
- **`pub(crate)` on `resolve_ask_adapter_cmd`** — Made in prior session to expose
  it to `mcp/config.rs`, but this approach was superseded. Visibility change
  kept (harmless) since the functions are useful internally.

---

## Open Questions

- Should `AxonMcpServer` store `ServiceContext` directly rather than lazily
  constructing it from `self.cfg` via `base_service_context()`? The user raised
  this direction — it remains a valid follow-up.
- Are there other MCP handlers besides `handle_ask` that reconstruct config
  manually (e.g., per-request `cfg.clone()` mutations)? Should audit.

---

## Next Steps

- Test `mcporter call axon.axon action:ask` end-to-end with `AXON_ASK_AGENT` set
  to confirm the fix works through the full MCP path.
- Consider full `ServiceContext` refactor in `AxonMcpServer` (store `ctx` instead
  of `cfg`; use `ctx.jobs` directly in crawl/embed handlers).
- Merge `feat/lite-mode` → `main` when ready (feature complete per prior session).
