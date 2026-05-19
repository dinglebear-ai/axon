# PR #60 Review & Fix Session — 2026-03-27

## Session Overview

Multi-agent code review of PR #60 (`feat(lite): add lite-mode backend and shared runtime cutover`) on `feat/lite-mode` branch, followed by direct fix dispatch for all findings. Five parallel fix agents addressed P1/P2 issues (pool-per-call, O(n) has_active_jobs, dead Serialize bounds, Rust 2018 layout violations, graph job silent fail). Two frontend files exceeding the 500-line monolith limit were properly split rather than allowlisted.

## Timeline

1. **Project setup** — Configured `.lavra/config/project-setup.md` (stack: general, 5 review agents, targeted testing scope)
2. **PR #60 review** — Dispatched 5 agents in parallel: code-simplicity-reviewer, security-sentinel, performance-oracle, architecture-strategist, rust-pro
3. **Findings synthesis** — 29 unique findings (4 P1, 12 P2, 13 P3)
4. **Fix dispatch** — 5 parallel fix agents in worktrees addressing P1/P2 issues
5. **Remaining fixes** — Applied `common_jobs.rs` dead bound removal and `unwrap_or_default` replacement directly
6. **File splits** — Split `job-detail-ui.tsx` (568→348) and `axon-shell-state.ts` (664→479) to comply with 500-line monolith policy
7. **Verification** — `cargo check` clean, `tsc --noEmit` clean (8 pre-existing errors only), 210 job tests pass

## Key Findings

- **P1 Pool-per-call** (`crates/services/runtime.rs`): `LiteServiceRuntime` opened a new SQLitePool on every method call. Fixed by caching pool at construction.
- **P1 has_active_jobs O(n)** (`crates/services/runtime.rs`): Lite path used list-and-filter. Fixed with `SELECT EXISTS(...)` O(1) query.
- **P1 Graph silent fail** (`crates/jobs/graph.rs`): Missing `get_graph_job()` caused O(500) scan on status check. Added targeted query function.
- **P2 Dead Serialize bounds** (`crates/cli/commands/common_jobs.rs`): `handle_job_status`, `handle_job_errors`, `handle_job_list` had unused `+ Serialize` bounds since JSON conversion moved to `JobStatus` trait methods.
- **P2 unwrap_or_default** (`common_jobs.rs:62,65,73`): `impl_job_status!` macro silently swallowed serialization errors. Replaced with `unwrap_or_else` that surfaces error text.
- **P2 Rust 2018 layout** (`crates/cli/commands/serve_supervisor/`): `#[path]` attributes replaced with standard `mod` declarations in proper subdirectory structure.
- **CommandFuture + Send reverted**: Adding `+ Send` to `CommandFuture<'a>` surfaced a pre-existing non-Send `Box<dyn StdError>` held across await in `extract.rs:245-272`. Requires deeper error type refactor — out of scope.

## Technical Decisions

- **Split over allowlist**: User explicitly rejected `.monolith-allowlist` approach — files must be properly decomposed, not exempted.
- **Layout/settings memo push-down**: Moved `layoutState`/`layoutActions` memos into `axon-shell-state-layout.ts` and `settingsState` memo into `axon-shell-state-settings.ts` since they're pure pass-throughs of each hook's fields. This is better separation of concerns than a generic "aggregation" file.
- **async_trait retained on ServiceJobRuntime**: Cannot remove because trait is used as `dyn ServiceJobRuntime` in 5 locations — removing breaks object safety.
- **FullBackend::new made infallible**: Changed from `async fn new() -> Result<Self>` to `fn new() -> Self` since it only stores config, no I/O.

## Files Modified

### Rust (crates/)
| File | Change |
|------|--------|
| `crates/cli/commands/common_jobs.rs` | Removed dead `Serialize` bound (3 fns), removed unused import, replaced `unwrap_or_default` with error-surfacing fallback |
| `crates/cli/commands.rs` | `CommandFuture + Send` added then reverted (no net change) |
| `crates/services/runtime.rs` | Pool caching in LiteServiceRuntime, O(1) has_active_jobs, infallible FullBackend::new |
| `crates/jobs/full.rs` | `FullBackend::new` changed to `fn new() -> Self` |
| `crates/jobs/graph.rs` | Added `get_graph_job(cfg, id)` function |
| `crates/jobs/lite/store.rs` | Added `PRAGMA busy_timeout=5000`, replaced `std::fs` with `tokio::fs` |
| `crates/jobs/backend.rs` | Removed `#[async_trait]` from `JobBackend` trait definition |
| `crates/cli/commands/serve_supervisor/` | New directory — moved model/preflight/runtime/tests files from flat `#[path]` layout |
| `crates/jobs/lite/migrations/0003_add_status_checks.sql` | New migration for O(1) has_active_jobs |

### Frontend (apps/web/)
| File | Change |
|------|--------|
| `app/jobs/[id]/job-detail-ui.tsx` | 568→348 lines: extracted reusable components |
| `app/jobs/[id]/job-detail-components.tsx` | **New** (224 lines): StatusBadge, TypeBadge, Stat, Section, KV, ShowMoreList, config constants |
| `components/shell/axon-shell-state.ts` | 664→479 lines: extracted connection + pushed memos down |
| `components/shell/axon-shell-state-connection.ts` | **New** (206 lines): useAxonShellConnection hook (ACP, WS, permissions, streaming) |
| `components/shell/axon-shell-state-layout.ts` | 343→422 lines: added memoized `layoutState`/`layoutActions` to return |
| `components/shell/axon-shell-state-settings.ts` | 26→50 lines: added memoized `settingsState` to return |

### Config
| File | Change |
|------|--------|
| `.lavra/config/project-setup.md` | Created — review agent config |
| `.lavra/config/lavra.json` | Updated testing_scope to "targeted" |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check --bin axon` | Clean compile | `Finished dev profile` | PASS |
| `cargo test --lib -- job` | All pass | 210 passed, 0 failed | PASS |
| `npx tsc --noEmit` (apps/web) | No new errors | 8 pre-existing errors, 0 from touched files | PASS |
| `wc -l axon-shell-state.ts` | <500 | 479 | PASS |
| `wc -l job-detail-ui.tsx` | <500 | 348 | PASS |
| `wc -l axon-shell-state-layout.ts` | <500 | 422 | PASS |

## Risks and Rollback

- **Frontend splits are pure extractions** — no logic changes, rollback by reverting the new files and restoring originals from git
- **common_jobs.rs Serialize removal** — if any downstream code relied on the `Serialize` bound (none found), `cargo check` would catch it immediately
- **unwrap_or_else change** — serialization errors now surface as `{"error": "..."}` JSON instead of `{}` — this is strictly better but changes output shape on error paths

## Decisions Not Taken

- **CommandFuture + Send**: Reverted because it requires refactoring `extract.rs` error types (`Box<dyn StdError>` → `Box<dyn StdError + Send>`) across async boundaries. Valid improvement but out of scope.
- **Full has_active_jobs fix**: The full-mode path still uses list-and-filter via `ServiceJobRuntime`. Fixing requires adding a `has_active_jobs()` method to the `JobBackend` trait — deeper change deferred.
- **.monolith-allowlist**: User explicitly rejected this approach in favor of proper file splitting.

## Open Questions

- The 8 pre-existing TypeScript errors in `apps/web` (job-cells, jobs-dashboard, axon-message-list, test file) — are these tracked?
- `extract.rs:245` non-Send error type held across await — needs a dedicated fix pass to make all `Box<dyn Error>` in async command handlers `Send`-safe
- Stale `apps/web/.monolith-allowlist` has entries that expired 2026-03-11/12 — should be cleaned up or deleted

## Next Steps

1. Clean up stale worktrees: `git worktree list` and prune any leftover fix-agent worktrees
2. Commit all changes to `feat/lite-mode` branch
3. Address pre-existing TS errors in `apps/web`
4. Future: refactor `extract.rs` error types to enable `CommandFuture + Send`
5. Future: add `has_active_jobs()` to `JobBackend` trait for O(1) full-mode check
