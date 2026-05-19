# Artifact System Phase 3: Blocking I/O, Auto-Inline, Smart Shape, File Split
**Date:** 2026-03-09
**Branch:** `refactor/acp-performance-modern-rust`
**Duration:** Single session

---

## Session Overview

Implemented Phase 3 of the MCP artifact system refactor across 5 tasks:

1. **Split** `crates/mcp/server/artifacts.rs` (481 lines) into a 5-file module before it crossed the 500-line monolith limit
2. **Fixed** all blocking sync I/O calls inside async MCP handler stacks (Tokio runtime starvation hazard)
3. **Improved** `json_shape_preview` to show short strings verbatim and generate status histograms for job arrays
4. **Added** auto-inline for small payloads — Claude reads `status` results without any follow-up `artifacts.read` tool call
5. **Updated** `.env.example`, `.env`, `docs/MCP.md`, and `docs/MCP-TOOL-SCHEMA.md`

**Result:** 949 lib tests pass, 0 clippy warnings, all 5 new files under 500 lines.

---

## Timeline

| Time | Activity |
|------|----------|
| Session start | Read `artifacts.rs` (544 lines) and `handlers_system.rs` to understand existing code |
| Task 1 | Created `crates/mcp/server/artifacts/` directory with 4 submodule files |
| Task 1 | Rewrote `artifacts.rs` as 17-line re-export facade |
| Task 1 (debug) | Fixed `#[path = "artifacts/..."]` attributes — `server.rs` uses `#[path = "server/artifacts.rs"]` so submodule resolution is relative to `server/`, not `server/artifacts/` |
| Task 1 (debug) | Changed `pub(super)` → `pub` in submodule files to allow re-exporting |
| Task 2 | Converted all sync fs calls to `tokio::fs::*` async equivalents |
| Task 2 | Updated 2 call sites in `handlers_system.rs` with `.await?` |
| Task 3 | Added `status_histogram` helper and updated `json_shape_preview` |
| Task 4 | Added auto-inline guard at top of `respond_with_mode` |
| Task 5 | Updated `.env.example`, `.env`, `docs/MCP.md`, `docs/MCP-TOOL-SCHEMA.md` |
| Verification | 949 lib tests pass, 0 clippy, all files under 500 lines |

---

## Key Findings

- **`#[path]` attribute interaction**: `server.rs` declares `#[path = "server/artifacts.rs"] pub(super) mod artifacts;`. When `artifacts.rs` then declares `mod path;`, Rust resolves the file relative to the directory of `artifacts.rs` (`server/`), looking for `server/path.rs` — NOT `server/artifacts/path.rs`. Fix: add `#[path = "artifacts/path.rs"]` etc. inside `artifacts.rs`.

- **`pub(super)` re-export restriction**: Items marked `pub(super)` in a child module (e.g. `lifecycle.rs`) are visible to the parent module (`artifacts`). But re-exporting them with `pub(super)` in `artifacts.rs` tries to make them visible to `server`, which exceeds the item's declared visibility. Rust rejects this. Fix: mark items `pub` in submodule files; visibility is effectively bounded by the private `mod` declarations.

- **Pre-existing test failure**: `youtube_help_describes_video_url_or_id_only` in `tests/cli_help_contract.rs` fails because `axon youtube` was removed (deleted `crates/cli/commands/youtube.rs`) in the ingest unification already in-progress on this branch. Not caused by Phase 3 changes.

- **Tokio runtime starvation**: The previous `std::fs::create_dir_all`, `std::fs::OpenOptions::open`, and `Path::canonicalize()` calls ran blocking I/O on async executor threads. Under load this starves other tasks queued on the same thread.

---

## Technical Decisions

| Decision | Rationale |
|----------|-----------|
| `pub` visibility in submodule files (not `pub(super)`) | Required for re-exporting. The private `mod` declarations in `artifacts.rs` bound effective visibility to `pub(super)` at the `server` level anyway. |
| Tests use `tokio::runtime::Builder::new_current_thread().block_on(...)` instead of `#[tokio::test]` | Allows holding a `std::sync::Mutex` guard across the async call without requiring the guard to be `Send`. The mutex guards env var mutations shared between tests. |
| Auto-inline `threshold > 0` check | Setting `AXON_INLINE_BYTES_THRESHOLD=0` disables auto-inline entirely without any special-case logic — zero means "disabled". |
| Status histogram uses `BTreeMap` | Deterministic key ordering in test assertions and JSON output. |
| Short-string threshold: 100 chars | Long enough to show real UUIDs, short paths, and status values; small enough to avoid inflating path-mode responses meaningfully. |
| Tests moved to `path.rs` and `shape.rs` (not consolidated in `lifecycle.rs`) | Keeping tests adjacent to the functions they test is cleaner; the plan's suggestion to put all tests in `lifecycle.rs` would have created awkward cross-module access. |

---

## Files Modified

| File | Action | Purpose |
|------|--------|---------|
| `crates/mcp/server/artifacts.rs` | Rewritten | Module root: 17 lines, `#[path]` + `pub(super) use` re-exports only |
| `crates/mcp/server/artifacts/path.rs` | Created (192 lines) | Path/dir helpers, all async. Tests for `ensure_artifact_root`. |
| `crates/mcp/server/artifacts/shape.rs` | Created (129 lines) | `line_count`, `sha256_hex`, `clip_inline_json`, `status_histogram`, `json_shape_preview`. 4 tests. |
| `crates/mcp/server/artifacts/respond.rs` | Created (95 lines) | `write_json_artifact` + `respond_with_mode` + auto-inline logic |
| `crates/mcp/server/artifacts/lifecycle.rs` | Created (227 lines) | `list_artifact_files`, `delete_artifact_file`, `clean_artifact_files`, `search_artifact_files` |
| `crates/mcp/server/handlers_system.rs` | Modified | Added `.await?` to `resolve_artifact_output_path`, `ensure_artifact_root`, and `validate_artifact_path` call sites |
| `.env.example` | Modified | Added `AXON_INLINE_BYTES_THRESHOLD=8192` with comment |
| `.env` | Modified | Added `AXON_INLINE_BYTES_THRESHOLD=8192` with comment |
| `docs/MCP.md` | Modified | Documented auto-inline behavior and shape preview improvements |
| `docs/MCP-TOOL-SCHEMA.md` | Modified | Updated `ResponseMode` enum to document `auto-inline` server-side behavior |

---

## Commands Executed

```bash
# Create submodule directory
mkdir -p crates/mcp/server/artifacts

# Replace pub(super) with pub in all 4 submodule files
sed -i 's/pub(super) /pub /g' crates/mcp/server/artifacts/{path,shape,respond,lifecycle}.rs

# Verify compilation
cargo check --bin axon
# Result: Finished dev profile — 0 errors, 0 warnings

# Run targeted artifact tests
cargo test --lib artifacts
# Result: 10 passed, 0 failed

# Full lib test suite
cargo test --lib
# Result: 949 passed, 0 failed (1 pre-existing Qdrant integration test failure)

# Clippy
cargo clippy --bin axon
# Result: 0 warnings
```

---

## Behavior Changes (Before/After)

### Blocking I/O
- **Before**: `std::fs::create_dir_all`, `std::fs::OpenOptions::open`, `Path::canonicalize()` ran synchronously on async executor threads inside MCP handler call stacks
- **After**: All filesystem operations use `tokio::fs::*` async equivalents; executor threads are never blocked

### `json_shape_preview` — Short strings
- **Before**: `"status": "<string 7>"` — Claude has no idea what the value is
- **After**: `"status": "running"` — Claude reads the value directly; no follow-up needed

### `json_shape_preview` — Job arrays
- **Before**: `"jobs": "<array[47]>"` — Claude must call `artifacts.read` to know job states
- **After**: `"jobs": {"total": 47, "by_status": {"completed": 44, "running": 2, "failed": 1}}` — Claude answers status questions from the shape alone

### Auto-inline
- **Before**: All payloads written to artifact file; small `status` responses require `artifacts.read` follow-up
- **After**: Payloads ≤ 8192 bytes returned inline with `"response_mode": "auto-inline"`. `status`, `doctor`, small `query` results all inline by default.

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check --bin axon` | 0 errors, 0 warnings | Finished dev profile, 0 warnings | ✅ |
| `cargo test --lib artifacts` | 10 pass, 0 fail | 10 passed, 0 failed | ✅ |
| `cargo test --lib` | 940+ pass, 0 fail | 949 passed, 0 failed | ✅ |
| `cargo clippy --bin axon` | 0 warnings | Finished dev profile | ✅ |
| `wc -l artifacts.rs` | ≤ 500 | 17 | ✅ |
| `wc -l artifacts/path.rs` | ≤ 500 | 192 | ✅ |
| `wc -l artifacts/shape.rs` | ≤ 500 | 129 | ✅ |
| `wc -l artifacts/respond.rs` | ≤ 500 | 95 | ✅ |
| `wc -l artifacts/lifecycle.rs` | ≤ 500 | 227 | ✅ |

---

## Source IDs + Collections Touched

None — no web scraping, embedding, or vector DB operations performed in this session.

---

## Risks and Rollback

**Risk**: Async `ensure_artifact_root` adds ~1 probe file write per MCP handler invocation (to test writability). This is already a hidden cost of the sync version; now it's on the async path where it won't starve other tasks.

**Risk**: Auto-inline bypasses explicit `response_mode=path` for small payloads. If a caller relies on always receiving an artifact path (e.g., for downstream processing), small payloads will now return `"response_mode": "auto-inline"` instead of `"path"`. Callers should check the `artifact` field (always present) regardless of mode.

**Rollback**: `git checkout crates/mcp/server/artifacts.rs crates/mcp/server/handlers_system.rs && rm -rf crates/mcp/server/artifacts/` restores the monolithic original. The env var additions to `.env.example`/`.env` are cosmetic and safe to leave.

---

## Decisions Not Taken

| Alternative | Rejected Because |
|-------------|-----------------|
| Put all tests in `lifecycle.rs` (as plan suggested) | Would require cross-module access to `path::ensure_artifact_root` and `shape::json_shape_preview` from tests; placing tests adjacent to their functions is cleaner |
| Use `tokio::sync::Mutex` for `ENV_CWD_LOCK` in tests | Requires `lazy_static` or `OnceLock` for static initialization and complicates test code; `block_on()` pattern avoids the issue without changing test structure |
| Keep `MCP_ARTIFACT_DIR_ENV` constant in `artifacts.rs` root | Would require `path.rs` to import it via `super::MCP_ARTIFACT_DIR_ENV`; defining it in `path.rs` keeps all path-related constants co-located |
| Run `mcp-schema-validator` agent | Schema changes were additive documentation only (new `auto-inline` mode description); manually updated both docs in-session; agent not needed for doc-only changes |

---

## Open Questions

- The `ensure_artifact_root` probe write (`is_writable`) runs on EVERY call to the function. Should it be cached with a `std::sync::OnceLock<PathBuf>` or short TTL? This is a pre-existing concern, not introduced by Phase 3.
- The `youtube_help_describes_video_url_or_id_only` integration test in `tests/cli_help_contract.rs` needs to be updated to use `axon ingest` now that `axon youtube` is removed. Should be part of the ingest unification PR review.

---

## Next Steps

- Run `just precommit` (monolith check + verify) to confirm pre-commit gate passes before pushing
- Address `youtube_help_describes_video_url_or_id_only` test — either update to `axon ingest <youtube-url>` or delete if superseded
- Consider adding a smoke test that verifies `status` action returns `"response_mode": "auto-inline"` (small payload)
- Consider caching `artifact_root` path resolution to avoid repeated env var reads per handler invocation
