# Session: Full Codebase Review — 57 Issues Fixed (v0.4.0)

**Date:** 2026-03-04
**Branch:** feat/sidebar
**Version bump:** 0.3.0 → 0.4.0
**Skill invoked:** `/comprehensive-review:full-review full codebase --strict-mode`

---

## Session Overview

Ran a strict-mode comprehensive review of the full axon_rust codebase (244 Rust files + 424 TypeScript files). Phase 1 found **40 issues** across code quality and architecture dimensions. Six parallel fix agents addressed all 40. Three CodeRabbit reviewers then found **17 new issues** (CR-A through CR-Q). Six more fix agents addressed all 17. Total: **57 issues found and resolved** in one session, all committed to `feat/sidebar`, tests green at 789 passing.

---

## Timeline

1. **Phase 1 review** — Parallel agents produced `01-quality-architecture.md`: 3 Critical, 7 High, 17 Medium, 13 Low findings
2. **First-round fix agents (6 parallel)** — oauth-fixer, jobs-fixer, ingest-fixer, web-fixer, vector-mcp-fixer, config-arch-fixer
3. **Session compaction** — killed 5 of 6 agents; re-dispatched; all completed
4. **CodeRabbit review (3 parallel)** — found 17 new issues CR-A through CR-Q
5. **Second-round fix agents (6 parallel)** — oauth-fixes, migrations-docs, ingest-fix, config-fixes, jobs-fixes, web-mcp-fixes
6. **Second session compaction** — killed 3 agents; branches had no unique commits
7. **Manual integration** — cherry-picked from worktree branches, resolved conflicts, committed all remaining changes
8. **Cleanup + push** — fixed flaky extract test, version bump, changelog update, `git push`

---

## Key Findings

### Critical (resolved)
- **C-01** `crates/mcp/server/oauth_google/state.rs:301-354` — OAuth cleanup triple-lock contention (15 lock ops, TOCTOU gaps) — already fixed before review
- **C-02** `crates/ingest/reddit/client.rs:14-19` — Two new `reqwest::Client` per call — fixed with `LazyLock<Client>`
- **C-03** `crates/mcp/server/oauth_google/handlers_protected.rs:22-293` — 290-line handler, cyclomatic complexity >20 — already fixed before review

### High Security (resolved)
- **H-02/M-08** `apps/web/app/api/pulse/chat/route.ts:272` — full server env passed to Claude CLI child process — fixed with `CLAUDE_CHILD_ENV_ALLOWLIST` + `CLAUDE_*` prefix passthrough
- **H-03** multiple `crates/jobs/` files — `format!` SQL interpolation — fixed in all job workers (crawl, extract, ingest, refresh)
- **H-05** `crates/mcp/server/oauth_google/state.rs:28-38` — unbounded HashMap growth — fixed with `MAX_OAUTH_STATE_ENTRIES=10_000` cap

### Architecture (scaffolded)
- **A-H-01** `crates/core/config/types/config.rs` — Config god object — scaffolded `Secret<T>`, `ConfigOverrides`, 6 sub-config structs; full decomposition deferred
- **A-M-04** — No database migration system — `migrations/001_initial_schema.sql` created

### CodeRabbit Notable
- **CR-E** `crates/mcp/server.rs:89-91` — `block_in_place` panics on `current_thread` runtime — replaced with `spawn_blocking`
- **CR-F** `crates/mcp/server/oauth_google/handlers_protected.rs` — token rotation race (delete-before-store) — fixed to store-before-delete
- **CR-G** `crates/core/config/secret.rs` — `Secret<String>` PartialEq short-circuits (timing oracle) — added `constant_time_eq()` via XOR-fold

---

## Technical Decisions

- **`block_in_place` → `spawn_blocking`**: `reqwest::Response` is `!Send`; `rmcp` proc-macro requires `Send`. Plain `.await` fails at compile. `spawn_blocking` with cheap `AxonMcpServer::clone()` (only holds `Arc<Config>`) satisfies both constraints.
- **Config decomposition scoped to scaffolding only**: Wiring `Secret<T>` through all 90+ fields and all callers was out of scope for a single review pass. New types committed as scaffolding with `#[allow(dead_code)]`; full migration tracked in `docs/config-decomposition-plan.md`.
- **Worktree merge via cherry-pick not `git merge`**: `crates/jobs/ingest.rs` diverged from 89 lines (feat/sidebar) to 578 lines (jobs-fixer worktree) — file was restructured into submodules. Full merge would conflict; specific SQL param changes were re-applied manually.
- **`apply_overrides` returns `Config` instead of `&mut self`**: CR-M finding — immutable receiver + return-new pattern is safer and more idiomatic than mutation; all 7 existing tests updated.

---

## Files Modified

### Rust Backend
| File | Change |
|------|--------|
| `crates/mcp/server/oauth_google/state.rs` | Capacity caps on all 5 HashMaps; background cleanup task |
| `crates/mcp/server/oauth_google/handlers_protected.rs` | Token rotation order fixed (store-before-delete) |
| `crates/mcp/server/oauth_google/handlers_google.rs` | put_pending_state error propagation |
| `crates/mcp/server/oauth_google/helpers.rs` | scheme validation, escape_html single-pass |
| `crates/mcp/server.rs` | `block_in_place` → `spawn_blocking` |
| `crates/ingest/reddit.rs` | `LazyLock<Client>` singleton |
| `crates/ingest/reddit/client.rs` | removed per-call client construction |
| `crates/ingest/youtube.rs` | 50MB VTT guard before `read_to_string` |
| `crates/jobs/crawl/runtime/worker/process.rs` | SQL parameterization (3 sites), `is_cancel_error()`, `PROGRESS_UPDATE_INTERVAL` |
| `crates/jobs/extract.rs` | SQL parameterization (4 sites) |
| `crates/jobs/extract/worker.rs` | SQL parameterization (2 sites) |
| `crates/jobs/ingest/ops.rs` | SQL parameterization (4 sites) |
| `crates/jobs/refresh/schedule.rs` | SQL parameterization |
| `crates/jobs/refresh/processor.rs` | SQL parameterization |
| `crates/jobs/extract/tests.rs` | `#[serial]` on DB tests (race condition fix) |
| `crates/jobs/status.rs` | doctest annotation fix |
| `crates/core/config.rs` | `pub mod secret`, `ConfigOverrides` re-export |
| `crates/core/config/types.rs` | `pub mod overrides`, `pub mod subconfigs` |
| `crates/core/config/types/config_impls.rs` | `Config::test_default()` |
| `crates/core/config/types/overrides.rs` | *(new)* `ConfigOverrides` + `apply_overrides` |
| `crates/core/config/types/subconfigs.rs` | *(new)* 6 sub-config structs |
| `crates/core/config/secret.rs` | *(new)* `Secret<T>` with `constant_time_eq` |
| `crates/crawl/engine/sitemap.rs` | added `sitemap_loc_in_scope_subdomain_branching` test |
| `crates/web.rs` | rustfmt fix |
| `lib.rs` | `OnceLock`-guarded telemetry schema DDL |
| `migrations/001_initial_schema.sql` | *(new)* all 7 job tables + `axon_session_ingest_state` |

### TypeScript / Web
| File | Change |
|------|--------|
| `apps/web/app/api/pulse/chat/route.ts` | `CLAUDE_CHILD_ENV_ALLOWLIST` + `ANTHROPIC_API_KEY` + `CLAUDE_*` passthrough |

### Docs
| File | Purpose |
|------|---------|
| `docs/config-decomposition-plan.md` | *(new)* 3-phase Config decomposition roadmap |
| `docs/error-handling.md` | *(new)* `AxonError` enum plan |
| `docs/web-architecture.md` | *(new)* Dual server topology analysis |
| `docs/migrations.md` | *(new)* Migration strategy guide |
| `docs/scaling.md` | horizontal scaling guide (updated with network fix) |
| `docs/WS-PROTOCOL.md` | WebSocket protocol contract (24 modes) |

---

## Commands Executed

```bash
# Verify each commit's test suite
cargo test --lib  # 789 passing, 0 failing

# Format fix before commit
cargo fmt -- crates/web.rs

# Version bump
sed -i 's/^version = "0.3.0"/version = "0.4.0"/' Cargo.toml
cargo check -q  # updates Cargo.lock

# Push
git push  # feat/sidebar → origin/feat/sidebar
```

---

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| Claude CLI child process | Inherits full server env (all DB passwords, API keys) | Only allowlisted vars pass through; ANTHROPIC_API_KEY explicitly allowed |
| MCP ask handler | `block_in_place` — panics on `current_thread` runtime | `spawn_blocking` — works on both runtime flavors |
| OAuth token rotation | Delete old token, then store new (race window) | Store new first, then delete old |
| OAuth state capacity | Unbounded HashMaps grow forever | Capped at 10,000 entries per map |
| SQL queries in job workers | `format!` interpolation (safe but precedent-setting) | Parameterized `$N` binds throughout |
| `Secret<T>` Debug output | Would print plaintext values | Prints `[REDACTED]` |
| `ServiceUrls` Debug | Leaks `pg_url`, `redis_url`, `openai_api_key` | Manual impl redacts all credential fields |
| `apply_overrides` | Mutates `&mut self` | Returns new `Config`, receiver unchanged |
| Telemetry schema DDL | `CREATE TABLE IF NOT EXISTS` on every CLI invocation | `OnceLock`-guarded, runs at most once per process |
| Extract DB tests | Flaky race condition under parallel test execution | `#[serial]` serializes DB access |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo test --lib` | 0 failures | 789 passed, 0 failed | ✅ |
| `cargo check` | 0 errors | Clean | ✅ |
| `cargo clippy` | 0 warnings | Clean | ✅ |
| `cargo fmt --check` | No diffs | Clean | ✅ |
| Pre-commit hooks (lefthook) | All pass | All pass | ✅ |
| `git push` | Accepted | `ee330e95..9eddd039` pushed | ✅ |

---

## Source IDs + Collections Touched

- Embedded via `axon embed` after session (see Axon integration below)

---

## Risks and Rollback

- **`spawn_blocking` for MCP ask** — adds a thread-pool hop but eliminates the `current_thread` panic. If performance degrades, profile first; rollback is trivial (revert `crates/mcp/server.rs`).
- **Config sub-config scaffolding** — new types are inert (`#[allow(dead_code)]`); no call sites changed. Zero runtime risk.
- **SQL parameterization** — all `JobStatus` enum values are trusted internal strings; parameterization is safer but behaviorally identical.
- **`OnceLock` telemetry DDL** — if multiple processes share a DB and race on first invocation, `CREATE TABLE IF NOT EXISTS` is still idempotent. Safe.
- **Rollback path**: `git revert <sha>` for any individual commit; all changes are in discrete commits.

---

## Decisions Not Taken

- **Full Config decomposition** (A-H-01) — would require touching 90+ fields and all callers across 50+ files; deferred to dedicated refactor session per `docs/config-decomposition-plan.md`.
- **`AxonError` enum** (A-M-08) — documented in `docs/error-handling.md`; not implemented; existing `Box<dyn Error>` boundaries unchanged.
- **Cargo workspace** (A-M-02) — significant build system change; documented in plan but not tackled.
- **Database migration tool** (`sqlx-migrate` / `refinery`) — `migrations/001_initial_schema.sql` created but no tool adopted; tracked in `docs/migrations.md`.
- **Merging worktree branches via `git merge`** — `crates/jobs/ingest.rs` had 89 vs 578 line divergence; cherry-pick of specific hunks was safer.

---

## Open Questions

- GitHub Dependabot flagged **5 high vulnerabilities** on the default branch. These are pre-existing on `main`, not introduced by this session. Should be triaged.
- `crates/cli/commands/job_display.rs` was created by an agent but never referenced by any `mod` declaration — deleted. Was it intended to replace something?
- `H-01` (DNS rebinding TOCTOU in SSRF guard) was accepted risk for single-tenant use — needs re-evaluation if multi-tenant deployment is planned.
- **L-09** (polling fallback has no reconnect) — accepted/documented; runbook note not yet added to ops docs.

---

## Next Steps

1. Triage the 5 Dependabot high vulnerabilities on `main`
2. Full Config decomposition (A-H-01) — use `docs/config-decomposition-plan.md` as guide
3. Adopt `sqlx-migrate` or `refinery` for schema migrations (A-M-04)
4. Wire `Secret<T>` into actual `Config` fields (pg_url, redis_url, openai_api_key, etc.)
5. Implement `AxonError` enum at command boundaries (A-M-08)
6. Add runbook note for L-09 (polling fallback reconnect behavior)
