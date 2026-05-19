# PR Review + Rust Best Practices Session
**Date:** 2026-02-28
**Branch:** `feat/crawl-download-pack`
**PR:** #5

---

## Session Overview

Multi-phase code quality session on PR #5 (`feat/crawl-download-pack`). Three distinct phases:
1. **PR review** using specialized agents (security, test coverage, code quality) — 16 issues found and fixed across TypeScript and Rust files
2. **Rust best practices audit** using Apollo GraphQL's 9-chapter handbook — 14 issues identified and fixed across 4 Rust files
3. **Residual cleanup** — additional rounds of targeted fixing, snapshot management, and final false-positive triage

End state: 443 Rust tests passing, 243 TS tests passing, 0 clippy warnings, 3 TS snapshots updated, 2 additional correctness fixes applied.

---

## Timeline

### Phase 1 — PR Review (parallel agents)
Three agents ran concurrently: silent-failure-hunter, pr-test-analyzer, code-reviewer. Found 16 issues total.

**TypeScript fixes applied (3 parallel agents):**
- SSRF hardening: IPv4-mapped IPv6 bypass (`::ffff:127.0.0.1`), trailing-dot normalization, IPv6 link-local (`fe80:`)
- MCP env key regex missing `$` anchor (`/^[A-Z_][A-Z0-9_]*/` → `/^[A-Z_][A-Z0-9_]*$/`)
- Dead code fix: `checkStdioServer` guard changed from `includes('/')` to `includes('..')` — `path.isAbsolute` branch was unreachable
- Error logging added to previously silent catch blocks in MCP status route
- `validateStatusUrl` exported and imported in tests (was locally re-implemented, SSRF bypasses went untested)
- `betas`/`toolsRestrict` character allowlist added: `.regex(/^[a-zA-Z0-9,\-.:]*$/)`
- 5 new SSRF bypass tests, hostile `allowedTools` tests with `;rm -rf /`, `$()`, null bytes

**Rust fixes applied (1 agent):**
- `loops.rs`: backoff reset on clean AMQP exit (sleep moved inside `Err` arm only)
- `loops.rs`: `process.rs` `validate_job_dir` switched from blocking `Path::is_dir()` to `tokio::fs::metadata().await`
- `worker_lane.rs`: dead code removed from clean-exit reconnect branch

### Phase 2 — Rust Best Practices Audit
Read all 9 Apollo chapters in parallel, then audited 4 files. 14 issues fixed by 2 parallel agents:

**`worker_lane.rs`:** redundant boolean intermediates inlined, bare `use tokio;` removed, `#[allow]` → `#[expect]`, `Vec::new()` → `Vec::with_capacity(3)`

**`loops.rs`:** bare `use tokio;` removed, Redis client per-sweep comment added, AMQP probe silent `Err(_) => false` now logs warning

**`pack.rs`:** 4 chained `.replace()` → single-pass `chars()` iterator with pre-allocated buffer; `pack_md_basic` 6-assertion test → `insta::assert_snapshot!`

**`download.rs`:** `max_files()`/`max_download_bytes()` → `static LazyLock<T>`; missing `warn!` for skipped unreadable files and malformed manifest lines; test rename and mod organization; `///` → `//!` module doc

### Phase 3 — Residual Issues
- **Snapshot capture:** `INSTA_UPDATE=force cargo test --lib pack` to create `pack_md_basic_snapshot.snap`
- **TS snapshots stale:** `pulse-chat-pane.tsx` renamed "Claude" → "Cortex"; 3 snapshots updated with `pnpm test -u`
- **Function size warnings:** `run_amqp_lane` (86L) → `setup_amqp_consumer()` extracted (72L); `run_polling_lane` (83L) → `sleep_or_drain_one()` extracted (69L)
- **`run_watchdog_sweep` doc comment:** expanded to document `lane == 1` gate explicitly
- **3 new tests in `loops.rs`:** sentinel empty-ids guard, cancel key format, writer/reader round-trip
- **`process.rs:59`:** `err.contains("canceled")` → `err.contains(CANCEL_SENTINEL)` where `CANCEL_SENTINEL = "AXON_JOB_CANCELED"` — prevents false cancellation detection from network errors
- **`sitemap.rs:69`:** `&lastmod[..10]` → `lastmod.get(..10).unwrap_or(lastmod)` — panic-safe UTF-8 slice

---

## Key Findings

- **SSRF bypass via IPv4-mapped IPv6:** `::ffff:127.0.0.1` and `::ffff:7f00:1` both resolve to localhost but bypassed the MCP URL validator. Fixed by adding hex-quad and dotted-decimal IPv4-mapped patterns.
- **`validateStatusUrl` not imported in tests:** `mcp/route.test.ts` re-implemented the function locally — SSRF fixes weren't tested against the real validator. Fixed by exporting and importing.
- **AMQP probe failure was silent:** `loops.rs:443` had `Err(_) => false` — any AMQP probe error silently fell back to polling mode with no log. Fixed with `log_warn!`.
- **Cancel detection by substring:** `process.rs:59` used `err.contains("canceled")` — network library errors (`"connection canceled by peer"`) could falsely mark failed jobs as Canceled. Fixed with `CANCEL_SENTINEL`.
- **Insta snapshot workflow:** `cargo insta` not installed as a cargo subcommand; must use `INSTA_UPDATE=force cargo test` to auto-accept new snapshots.
- **`run_amqp_lane` backoff bug (pre-existing fix):** sleep was unconditional — on clean exit the delay was applied before reconnect. Fixed so sleep only runs in `Err` arm.

---

## Files Modified

| File | Purpose |
|------|---------|
| `apps/web/app/api/mcp/status/route.ts` | IPv4-mapped IPv6 SSRF, trailing-dot, dead-code fix, error logging |
| `apps/web/app/api/mcp/route.ts` | Env key regex anchor, SSRF before save, config validation on read |
| `apps/web/app/api/pulse/chat/route.ts` | Error logging on Claude CLI exit, fallback metadata |
| `apps/web/app/api/pulse/chat/claude-stream-types.ts` | Character allowlist on `betas`/`toolsRestrict` |
| `apps/web/lib/pulse/types.ts` | `fallback_source` metadata field |
| `apps/web/__tests__/mcp/route.test.ts` | Import real `validateStatusUrl`, 5 new SSRF tests |
| `apps/web/__tests__/pulse/build-claude-args.test.ts` | `validateAddDir` edge cases, hostile `allowedTools` tests |
| `apps/web/__tests__/__snapshots__/pulse-chat-pane-layout.test.ts.snap` | Updated: "Claude" → "Cortex" display name |
| `apps/web/__tests__/__snapshots__/omnibox-snapshot.test.tsx.snap` | Updated: "Cortex" display name |
| `crates/jobs/worker_lane.rs` | Boolean intermediates, bare import, `#[expect]`, capacity, function extraction |
| `crates/jobs/crawl/runtime/worker/loops.rs` | Bare import, Redis comment, AMQP probe warning, watchdog doc, 3 new tests, function extraction |
| `crates/jobs/crawl/runtime/worker/process.rs` | `CANCEL_SENTINEL` constant, cancel detection via sentinel |
| `crates/web/download.rs` | `LazyLock`, missing `warn!`, test rename/org, `//!` module doc |
| `crates/web/pack.rs` | Single-pass XML escape, `insta::assert_snapshot!` |
| `crates/web/snapshots/axon__crates__web__pack__tests__pack_md_basic_snapshot.snap` | New: insta snapshot baseline |
| `crates/crawl/engine/sitemap.rs` | UTF-8-safe `lastmod` slice |
| `Cargo.toml` | `insta = "1"` dev-dependency |

---

## Commands Executed

```bash
# Verification gates
cargo check                          # clean
cargo clippy --all-targets           # 0 warnings
cargo test --lib                     # 443 passing
INSTA_UPDATE=force cargo test --lib pack  # generated snapshot
pnpm test -u                         # 243 passing, 3 snapshots updated
python3 scripts/enforce_monoliths.py --file crates/jobs/worker_lane.rs  # pass

# Monolith function sizes (after extraction)
# run_amqp_lane: 86 → 72 effective lines
# run_polling_lane: 83 → 69 effective lines
```

---

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| AMQP probe failure | Silent fallback to polling | Logs `log_warn` with queue name and error |
| Job cancel detection | `err.contains("canceled")` — matches any error with that word | `err.contains("AXON_JOB_CANCELED")` — matches only sentinel |
| MCP URL validation | IPv4-mapped IPv6 bypassed validator | All IPv4-mapped forms blocked |
| SSRF test coverage | Tests used local re-impl of validator | Tests import and exercise real `validateStatusUrl` |
| XML escaping | 4 chained `.replace()` = 3 intermediate allocs | Single-pass `chars()` loop, 0 intermediate allocs |
| Env var reads | `max_files()`/`max_download_bytes()` re-read env on every request | Read once at startup via `LazyLock` |
| Snapshot tests | `pack_md_basic` had 6 inline assertions | `insta::assert_snapshot!` baseline committed |
| Sitemap date parsing | `&lastmod[..10]` — could panic on multi-byte chars | `lastmod.get(..10).unwrap_or(lastmod)` — panic-safe |
| TS snapshots | Referenced "Claude" as assistant name | Updated to "Cortex" |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check` | clean | `Finished dev profile` | ✅ |
| `cargo clippy --all-targets` | 0 warnings | 0 warnings | ✅ |
| `cargo test --lib` | all pass | 443 passed, 0 failed | ✅ |
| `pnpm test` | all pass | 243 passed, 3 snapshots updated | ✅ |
| `enforce_monoliths.py --file worker_lane.rs` | pass | `Monolith policy check passed` | ✅ |
| `enforce_monoliths.py --staged` | pass | `Monolith policy check passed` | ✅ |

---

## False Positives Triaged (Did Not Fix)

| Finding | Verdict |
|---------|---------|
| `parse.rs` `let _` on `set_host`/`set_port` | Intentional — hardcoded HOST_MAP enum values, not user input |
| `watchdog.rs` SQL injection | Not injection — table/status from Rust enums, user value parameterized with `$1` |
| `content.rs` `find_between` UTF-8 panic | Safe — `String::find()` always returns char-boundary byte indices |
| `loops.rs` "missing timers scope" | False positive — Rust borrow checker enforces safety; code compiles clean |
| `download.rs` blocking `is_dir()` | Pre-fixed — all I/O is `tokio::fs::*` (confirmed by grep) |
| React key `${tool.name}-${j}` | Acceptable — tools within a message block are stable and small |
| `mcp/server.rs` / `execute/mod.rs` oversized | Pre-existing, in `.monolith-allowlist` |

---

## Decisions Not Taken

- **Typed error for cancel detection** — Could have used a custom `CancelError` struct instead of `CANCEL_SENTINEL` string. Rejected: would require changing `Box<dyn Error>` return types throughout the call stack. String sentinel is simpler and sufficient.
- **Shared Redis client in `signal_reclaimed_cancel_keys`** — Could pass a long-lived client. Rejected per CLAUDE.md: fresh client per sweep avoids holding a connection across idle periods; documented with comment.
- **Split `worker_lane.rs` (725L)** — File is over 500 lines but passes monolith enforcer (test blocks excluded from count). Deferred; would require significant restructure without immediate correctness benefit.
- **`insta` snapshot for all multi-assertion tests** — Only applied to `pack_md_basic`. Extending to all tests would require `cargo-insta` install across CI — deferred.

---

## Risks and Rollback

- **`CANCEL_SENTINEL` change**: If any in-flight job was mid-cancel when this deployed, its error string won't match the new sentinel and would be recorded as `Failed` instead of `Canceled`. One-time migration acceptable; no data loss.
- **`LazyLock` for env vars**: Values are now fixed at first read. If `AXON_DOWNLOAD_MAX_FILES`/`AXON_DOWNLOAD_MAX_BYTES` are changed at runtime (hot-reload scenario), the new value won't be picked up without a restart. This is the correct behavior for a service binary.
- **Rollback**: All changes are on `feat/crawl-download-pack`. Rolling back is `git revert` or branch reset; no schema changes, no migrations, no external state modified.

---

## Open Questions

- `process.rs` error type is `Box<dyn Error>` throughout — the `CANCEL_SENTINEL` approach works but a proper typed error hierarchy would be cleaner. Tracked as future refactor.
- `signal_reclaimed_cancel_keys` has no integration tests (unit tests for format/guard added; Redis path untested without live Redis).
- `ingest errors <uuid>` subcommand is silently unhandled in `maybe_handle_ingest_subcommand` — pre-existing gap, not addressed this session.

---

## Next Steps

- `/quick-push` to commit and push all changes
- `crates/web/snapshots/` directory (untracked) needs to be included in the commit
- Consider adding `cargo-insta` to CI for snapshot review workflow
- Future: typed error hierarchy to replace `CANCEL_SENTINEL` string matching
