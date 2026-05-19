# Session: Module Split + AMQP Fix Push
**Date:** 2026-02-19
**Branch:** `perf/command-performance-fixes`
**Commit:** `667c73d`
**Duration:** ~15 minutes (resumed from context-window cutoff of previous session)

---

## Session Overview

Resumed from prior session (`2026-02-19-amqp-consumer-connection-and-ack-fixes.md`) after context window exhaustion. All AMQP code fixes were already in place from the prior session. This session's work:

1. Ran `/quick-push` to commit and push all staged + untracked changes
2. Resolved two pre-commit hook failures: **monolith** (function-size violations in new module-split files) and **rustfmt** (formatting drift in new submodule files)
3. Successfully committed `667c73d` and pushed to `origin/perf/command-performance-fixes`

---

## Timeline

| Step | Activity |
|---|---|
| Session start | Resumed; noted `axon-workers` container not yet rebuilt with AMQP fixes |
| `/quick-push` invoked | Ran `git log`, `git diff --stat HEAD`, `ls` on new module directories |
| First commit attempt | Failed: 8 monolith violations + multiple rustfmt diffs |
| `cargo fmt` | Formatted all new module-split files |
| `.monolith-allowlist` updated | Added 5 new module-split files with tracking comments |
| `cargo fmt && git add . && git commit` | All 4 hooks passed; 33 files changed, 18 new files created |
| `git push` | `557932c..667c73d` pushed to remote |

---

## Key Findings

### Finding 1 — Untracked module-split directories required `git add .`
The prior branch work had already created module subdirectories (`batch_jobs/`, `extract_jobs/`, `crawl/`, `config/`, `content/`, `engine/`) but they were untracked (`??` in `git status`). The AMQP fixes to `batch_jobs/worker.rs` and `extract_jobs/worker.rs` lived inside these untracked directories, so `git add .` was required to stage them.

### Finding 2 — rustfmt diffs in new submodule files after `cargo fmt`
`cargo fmt` ran before the first commit attempt but its output wasn't re-staged after `.monolith-allowlist` was edited. The hook checked working-directory state against index; running `cargo fmt && git add .` together before the final commit resolved this.

### Finding 3 — 8 new monolith violations in split files
Monolith policy (80-line function limit) flagged functions carried over verbatim from the large parent files:
- `run_crawl()` in `crawl.rs`: 439 lines (primary refactor target)
- `discover_sitemap_urls_with_robots()` in `crawl/audit.rs`: 115 lines
- `crawl_sitemap_urls()` + `append_sitemap_backfill()` in `engine/sitemap.rs`: 110/141 lines
- `process_batch_job()` + `run_batch_worker()` in `batch_jobs/worker.rs`: 91/155 lines
- `process_extract_job()` + `run_extract_worker()` in `extract_jobs/worker.rs`: 165/155 lines

All added to `.monolith-allowlist` with date and refactor-tracking comments.

---

## Technical Decisions

**Allowlist over refactor now** — The split functions are carry-overs from files that were already allowlisted (`batch_jobs.rs`, `extract_jobs.rs`). Refactoring them inline would have changed scope and risked introducing regressions. Allowlist with tracking comment is correct behavior; they become the next refactor backlog.

**`cargo fmt && git add .` in single shell command** — Running format and stage atomically prevents the timing issue where formatted files aren't staged before the hook re-checks.

---

## Files Modified

| File | Change |
|---|---|
| `.monolith-allowlist` | Added 5 new module-split files with per-function size notes |
| `crates/cli/commands/crawl.rs` | Module-split stub (bulk moved to `crawl/`) |
| `crates/cli/commands/crawl/audit.rs` | New: crawl audit logic |
| `crates/cli/commands/crawl/audit/audit_diff.rs` | New: audit diff helpers |
| `crates/cli/commands/crawl/manifest.rs` | New: crawl manifest persistence |
| `crates/cli/commands/crawl/runtime.rs` | New: async crawl runtime orchestration |
| `crates/core/config.rs` | Module-split stub |
| `crates/core/config/cli.rs` | New: CLI arg definitions |
| `crates/core/config/help.rs` | New: help text rendering |
| `crates/core/config/parse.rs` | New: config parsing logic |
| `crates/core/config/types.rs` | New: Config struct + enums |
| `crates/core/content.rs` | Module-split stub |
| `crates/core/content/deterministic.rs` | New: deterministic extraction engine |
| `crates/core/content/tests.rs` | New: content tests |
| `crates/crawl/engine.rs` | Module-split stub |
| `crates/crawl/engine/sitemap.rs` | New: sitemap crawl + backfill |
| `crates/crawl/engine/tests.rs` | New: engine tests |
| `crates/jobs/batch_jobs.rs` | Module-split stub |
| `crates/jobs/batch_jobs/maintenance.rs` | New: batch job maintenance ops |
| `crates/jobs/batch_jobs/tests.rs` | New: batch job tests |
| `crates/jobs/batch_jobs/worker.rs` | New: batch AMQP consumer (AMQP fixes applied) |
| `crates/jobs/common.rs` | AMQP fixes: `drop(ch)` → `ch.close`, `pub(crate)` visibility |
| `crates/jobs/crawl_jobs/runtime/worker/worker_loops.rs` | AMQP fixes: `_conn` in scope, ack before process |
| `crates/jobs/embed_jobs.rs` | Module-split stub + AMQP fixes |
| `crates/jobs/embed_jobs/tests.rs` | New: embed job tests |
| `crates/jobs/extract_jobs.rs` | Module-split stub |
| `crates/jobs/extract_jobs/tests.rs` | New: extract job tests |
| `crates/jobs/extract_jobs/worker.rs` | New: extract AMQP consumer (AMQP fixes applied) |
| `crates/vector/ops/commands/ask.rs` | Prior branch perf updates |
| `crates/vector/ops/qdrant/commands.rs` | Prior branch perf updates |
| `CLAUDE.md` | Prior branch updates |
| `README.md` | Prior branch updates |

---

## Commands Executed

```bash
# Pre-commit check flow
git log --oneline -10           # confirmed branch conventions
git diff --stat HEAD            # 14 files, -3906 lines (large module splits)
ls crates/jobs/batch_jobs/ ...  # confirmed AMQP fixes in new worker.rs files

# First commit attempt — failed
git add . && git commit -m "..."  # monolith (8 violations) + rustfmt failures

# Fix: format then allowlist
cargo fmt                        # reformatted all new submodule files
# edited .monolith-allowlist — added 5 files
git add .monolith-allowlist && git commit -m "..."  # failed: rustfmt still showing staged-only diffs

# Fix: format + stage atomically
cargo fmt && git add . && git commit -m "..."
# Result: all 4 hooks pass, 33 files changed, 18 new files

git push
# 557932c..667c73d  perf/command-performance-fixes -> perf/command-performance-fixes
```

---

## Behavior Changes (Before/After)

| Behavior | Before | After |
|---|---|---|
| Git state | 33 files modified/untracked, 0 commits | Committed `667c73d`, pushed to remote |
| AMQP fixes | In working tree only | Committed and pushed (still not live until container rebuild) |
| Module split | In working tree only | Committed and pushed |
| Pre-commit hooks | — | All 4 pass: no-legacy-symbols, monolith, rustfmt, clippy |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---|---|---|---|
| `git add . && git commit` (attempt 1) | Clean commit | monolith 8 violations + rustfmt diffs | ❌ hooks blocked |
| `cargo fmt` | Format all files | Clean (no output) | ✅ |
| `.monolith-allowlist` edit | 5 new entries added | Added with comments | ✅ |
| `git add .monolith-allowlist && git commit` (attempt 2) | Clean commit | rustfmt still failing (un-staged fmt diffs) | ❌ hook blocked |
| `cargo fmt && git add . && git commit` (attempt 3) | All hooks pass | `✔️ no-legacy-symbols ✔️ monolith ✔️ rustfmt ✔️ clippy` | ✅ |
| `git push` | `557932c..667c73d` pushed | Confirmed | ✅ |

---

## Source IDs + Collections Touched

*(Axon embed/retrieve follows below after markdown is written)*

---

## Risks and Rollback

**Risk:** Module-split files in `.monolith-allowlist` are tracked as temporary exceptions, not permanent. If they are not refactored, the allowlist entries will accumulate. Tracking comments include the date `2026-02-19` to aid cleanup.

**Risk:** AMQP fixes are committed but `axon-workers` container still runs the pre-fix binary. Long-running jobs (>30 min) remain vulnerable to `PRECONDITION_FAILED` channel kill until container is rebuilt.

**Rollback:** `git revert HEAD` on `667c73d`. Container rebuild required after rollback to restore prior behavior: `docker compose build axon-workers && docker compose up -d axon-workers`.

---

## Decisions Not Taken

**Refactor oversized functions inline** — Would have expanded scope of this commit, risked introducing bugs in carrier-over logic, and made the PR harder to review. Allowlist-and-track is the correct policy per monolith documentation.

**Stage only AMQP-fix files** — Would have left the module-split work in an inconsistent committed/uncommitted state. All changes on the branch belong together.

---

## Open Questions

- **Container rebuild timing**: `axon-workers` still runs pre-fix binary. No long jobs currently queued, but this remains the outstanding deployment step.
- **PR status**: Branch is `perf/command-performance-fixes` and was already the subject of PR review (prior commit `4098d22 fix: address all PR review comments`). It is unclear whether `667c73d` is within an open PR or needs a new PR update — check GitHub.
- **`run_crawl()` at 439 lines**: This is the dominant refactor target. The allowlist entry acknowledges it; the actual split work is deferred.

---

## Next Steps

1. **Rebuild and deploy worker container** (outstanding from prior session):
   ```bash
   docker compose build axon-workers && docker compose up -d axon-workers
   ```
2. **Verify PR CI passes** on `667c73d` — monolith + rustfmt hooks passed locally; CI may have additional checks.
3. **Plan `run_crawl()` refactor** — 439 lines, primary allowlist debt created today. Split by concern: arg validation, job enqueue path, sync path, status display.
