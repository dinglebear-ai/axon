# Session: Lite Mode PR Review Fixes — Batch 2
Date: 2026-03-27
Branch: feat/lite-mode
Version: 0.33.4 → 0.33.5

## Session Overview

Resumed from a previous context-compacted session mid-commit. Fixed remaining compilation errors and clippy warnings blocking clean commits, then landed per-bead commits for all P0/P1 review findings from PR #60 (rs8.1–rs8.9). Closed all 9 beads. Also fixed MCP start handlers to route through `ServiceContext` (so lite mode is respected end-to-end) and corrected missing early-return paths in `crawl_start_with_context` and `embed_start_with_context`.

## Timeline

1. **Unblocked compilation** — Three issues inherited from previous session: `pub(self)` clippy error on `lift_ss`, 7× `explicit_auto_deref` on `&**self.backend.pool()`, and a `?` coercion failure in `crawl.rs:542` test function. All fixed.
2. **rs8.1 committed** — `pool()` getter on `LiteBackend`; `resolve_runtime` no longer opens a redundant second pool.
3. **rs8.6 committed** — `list_service_jobs` in `lite/query.rs` now uses passed `limit`/`offset` instead of hardcoded `LIMIT 500`.
4. **rs8.7 committed** — Lite mode guards added to graph and refresh MCP handlers.
5. **rs8.8 committed** — `downgrade()` helper added to `services/jobs.rs`; all test mock `ServiceJobRuntime` impls updated to `Box<dyn Error + Send + Sync>`.
6. **rs8.4 committed** — `crates/services/runtime.rs` (616→214 lines) split into `runtime/full.rs` + `runtime/mapping.rs`; `crates/jobs/lite/workers.rs` (524→246 lines) split into `workers/runners.rs`.
7. **All 9 P0/P1 beads closed** via `bd close axon_rust-rs8.1` through `rs8.9`.
8. **MCP handler context wiring** — crawl, extract, embed, ingest MCP start handlers switched from raw service functions to `_with_context` variants.
9. **Early-return fixes** — `crawl_start_with_context` and `embed_start_with_context` now return immediately when `cfg.wait = false` instead of falling through to the blocking wait loop.
10. **Pushed v0.33.5** — all commits landed with pre-commit hooks passing (clippy, check, 1574 tests).

## Key Findings

- `pub(self)` is a clippy lint (`needless_pub_self`) — just remove it; private fields are accessible from child modules in Rust.
- `&**self.backend.pool()` is clippy `explicit_auto_deref` for function-call sites (`&SqlitePool` param), but NOT for sqlx's generic `Executor<'_>` bound. The deref is necessary for `fetch_one()`. Solution: use `.as_ref()` for the sqlx call, plain `self.backend.pool()` for function call sites.
- `Box<dyn Error>` and `Box<dyn Error + Send + Sync>` have no automatic coercion via `?`. Explicit `.map_err(|e| e.to_string().into())` is required at every boundary.
- `crawl_start_with_context` had no early-return after building `result` when `cfg.wait = false` — it fell through into the `for job in &result.jobs` wait loop unconditionally.
- MCP handlers were calling `crawl_svc::crawl_start()` (config-only, bypasses `ServiceContext`) instead of `crawl_start_with_context()`. Same for extract, embed, ingest. Lite mode was therefore never enforced at the MCP layer.

## Technical Decisions

- **`pool().as_ref()`** for the single sqlx `Executor` call rather than `&**pool()` (which clippy flags) or changing the `pool()` return type. Minimal change, clear semantics.
- **`downgrade()` helper in `services/jobs.rs`** — converts `Box<dyn Error + Send + Sync>` → `Box<dyn Error>` at the wrapper function boundary. Keeps the trait surface clean while letting CLI callers use the simpler error type.
- **Per-bead commits** — each rs8.x fix is its own commit so `git log --grep="rs8.6"` isolates exactly what changed for pagination, etc.
- **`lift_ss` visibility** — keeping it module-private (`fn`, not `pub`) since it's only used inside `runtime.rs` and its child modules (`full.rs` accesses it via `use super::lift_ss`).

## Files Modified

| File | Change |
|------|--------|
| `crates/jobs/lite.rs` | Added `pub fn pool(&self) -> &Arc<SqlitePool>` getter |
| `crates/jobs/lite/query.rs` | Removed hardcoded `LIMIT 500`; `list_service_jobs` now uses `limit`/`offset` params |
| `crates/services/runtime.rs` | Split to 214 lines; fixed `pub(self)`, auto-deref, `fetch_one(.as_ref())` |
| `crates/services/runtime/full.rs` | New — `FullServiceRuntime` impl of `ServiceJobRuntime` (305 lines) |
| `crates/services/runtime/mapping.rs` | New — job type conversion helpers `*_to_service_job` (119 lines) |
| `crates/jobs/lite/workers.rs` | Split to 246 lines; runner bodies moved to submodule |
| `crates/jobs/lite/workers/runners.rs` | New — `run_*_job_lite` runner bodies (312 lines) |
| `crates/mcp/server/handlers_graph.rs` | Added lite mode guard |
| `crates/mcp/server/handlers_refresh_status.rs` | Added lite mode guard |
| `crates/services/jobs.rs` | Added `downgrade()` helper; fixed test mock Send+Sync return types |
| `crates/services/crawl.rs` | Fixed `?` coercion in test; added early-return when `cfg.wait=false`; `cfg.wait=true` in tests; new enqueue-without-blocking test |
| `crates/services/embed.rs` | Added early-return when `cfg.wait=false` |
| `crates/mcp/server/handlers_crawl_extract.rs` | Switched crawl/extract to `_with_context` variants |
| `crates/mcp/server/handlers_embed_ingest.rs` | Switched embed/ingest to `_with_context` variants |
| `Cargo.toml` + `Cargo.lock` | Version 0.33.4 → 0.33.5 |
| `CHANGELOG.md` | Added v0.33.5 section |

## Commands Executed

```bash
cargo check          # verified clean after each fix pass
cargo clippy         # verified zero warnings
git add <files> && git commit -m "fix(rs8.N): ..."   # per-bead commits x5
bd close axon_rust-rs8.1 ... axon_rust-rs8.9          # closed 9 beads
git push             # pushed to origin/feat/lite-mode
```

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| MCP crawl/extract/embed/ingest start | Called config-only functions, ignored lite mode | Routes through `ServiceContext`, lite mode respected |
| `crawl_start_with_context` with `wait=false` | Fell through into blocking wait loop | Returns immediately after enqueueing |
| `embed_start_with_context` with `wait=false` | Called `wait_for_embed_completion` unconditionally | Returns immediately after enqueueing |
| `list_jobs` in lite mode | Silently returned max 500 results, ignored offset | Respects `limit` and `offset` parameters |
| `resolve_runtime` lite path | Opened two SQLite pool connections to same file | Single pool via `LiteBackend::new()` |
| `runtime.rs` / `workers.rs` file sizes | 616 and 524 lines (monolith violations) | 214 and 246 lines (compliant) |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check` | Clean | 0 errors | ✅ |
| `cargo clippy` | No warnings | 0 warnings | ✅ |
| Pre-commit hook (1574 tests) | All pass | 1574 ok | ✅ |
| `bd list` P0/P1 beads | All closed | rs8.1–rs8.9 ✓ | ✅ |
| `git push` | Accepted | `5a21aba3..45bc76e9` | ✅ |

## Source IDs + Collections Touched

None — no Qdrant embed/retrieve operations were performed in this session (all work was code changes and commits).

## Risks and Rollback

- **MCP handler change** — switching from `crawl_svc::crawl_start()` to `crawl_start_with_context()` adds a `ServiceContext` construction step per MCP call. If `service_context_for()` is slow or fails, MCP calls will surface a new `logged_internal_error("crawl.start.context", ...)` error. Rollback: revert `handlers_crawl_extract.rs` to the config-only call.
- **Migration 0003** (rs8.3, prior session) — unknown-status rows are now mapped to `'failed'` with the original status preserved in `error_text`. Irreversible once migration runs. Low risk since unknown statuses indicate already-broken rows.
- **File splits** (rs8.4) — pure refactor with no behavior change. Rollback by reverting to single-file versions if needed.

## Decisions Not Taken

- **Change `pool()` return type to `&SqlitePool`** — would fix the `fetch_one` issue without `.as_ref()`, but requires changing all callers; the current approach minimizes diff.
- **Make `lift_ss` `pub(crate)`** — not needed since child modules can access private items from their parent; `pub(crate)` would expand visibility unnecessarily.
- **Single omnibus commit** — opted for per-bead commits so each fix is independently revertable and `git log --grep` works.

## Open Questions

- The 3 GitHub Dependabot vulnerability alerts (1 high, 2 moderate) on the default branch appear on every push. These are unrelated to this branch's changes and need triage on `main`.
- P2/P3 beads (rs8.10–rs8.17) remain open: SQLite pool size vs worker count, `wait_for_job` timeout, dead code in `poll_sqlite_for_cancels`, watch def FK enforcement, and simplification opportunities.

## Next Steps

- Open PR from `feat/lite-mode` → `main` (the branch has been feature-complete since the shared runtime cutover; this session addressed all P0/P1 review blockers)
- Triage Dependabot alerts on main branch
- Address P2/P3 beads (rs8.10–rs8.17) — optional before merge but worth tracking
