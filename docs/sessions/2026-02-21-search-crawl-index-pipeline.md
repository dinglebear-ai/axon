# Session: Search → Crawl → Index Pipeline

**Date:** 2026-02-21
**Branch:** `perf/command-performance-fixes`
**Base commit:** `1b54d7e` (feat: research command, Tavily search, monolith splits)

---

## Session Overview

Implemented automatic knowledge-base growth on every `axon search` invocation. Previously search printed Tavily results and returned. Now it automatically queues async crawl jobs for each unique origin domain in the results — fire-and-forget, no new workers, pure glue between two existing systems. A code review caught and fixed a SSRF guard bypass before merge. Three follow-up tightening changes were applied post-review.

---

## Timeline

| Time | Activity |
|------|----------|
| Start | Read plan, explored all relevant source files |
| Phase 1 | Implemented 6-part plan: Config field, CLI flag, parse wiring, 3 test fixture fixes, `extract_crawl_seed` helper, `run_search` fan-out loop |
| Phase 2 | Code review via `superpowers:code-reviewer` — identified 1 Critical (SSRF bypass), 1 Important (bundled change), 3 Minor |
| Phase 3 | Fixed Critical: added `validate_url(seed)` guard before `start_crawl_job` in `search.rs` |
| Phase 4 | Tightened 3 remaining minor items: scheme validation, dedup test, log verbosity |
| End | 179 tests passing, 0 clippy warnings, fmt clean |

---

## Key Findings

- **SSRF bypass**: `start_crawl_job()` in `crates/jobs/crawl_jobs/runtime/mod.rs` has no `validate_url()` call. `run_crawl()` applies the guard but `search.rs` bypassed it by calling the job function directly. Fixed with an explicit `validate_url(seed)` check before enqueue (`search.rs:80`).
- **`chrome_remote_url` normalization gap**: `parse/mod.rs` was the only service URL missing `.map(normalize_local_service_url)`. Fixed in the same PR as an incidental bug fix (`parse/mod.rs:229`).
- **`extract_crawl_seed` is `pub`**: Because it's exported, scheme validation must happen inside the function itself — not just downstream at `validate_url`. Both `ftp://` and `file://` seeds now return `None` before any caller gets them.
- **Log verbosity**: Default `search_limit=10` produced 10+ per-seed log lines per search. Collapsed to a single summary line after all jobs are queued.

---

## Technical Decisions

- **Origin crawl by default** (`crawl_from_result = false`): Three results from `docs.rust-lang.org` → one crawl job, not three. Users wanting targeted crawls pass `--crawl-from-result true`.
- **`HashSet` deduplication**: Simplest correct approach. `start_crawl_job` already has Postgres dedup for active/pending jobs, so double-queuing the same origin is safe but wasteful; the `HashSet` prevents it in-process.
- **Tolerant fan-out**: Failed AMQP enqueues warn and continue; search command always succeeds if the Tavily query succeeds. Crawl jobs are best-effort.
- **Scheme check inside `extract_crawl_seed`**: Placed at parse time (before the `from_result` branch split) so both code paths are covered and callers get a clean `Option<String>` contract.
- **Summary log line**: `"Queued N crawl job(s): id1, id2, ..."` — compact, still shows job IDs for debugging without flooding the terminal.

---

## Files Modified

| File | Purpose |
|------|---------|
| `crates/core/config/types.rs` | Added `pub crawl_from_result: bool` field to `Config`; added to `Debug` impl |
| `crates/core/config/cli.rs` | Added `--crawl-from-result` global arg to `GlobalArgs` |
| `crates/core/config/parse/mod.rs` | Wired `crawl_from_result` into `into_config()`; fixed `chrome_remote_url` normalization |
| `crates/cli/commands/search.rs` | Primary implementation: `extract_crawl_seed`, fan-out loop, SSRF guard, 8 unit tests |
| `crates/cli/commands/research.rs` | Test fixture: added `crawl_from_result: false` to `make_cfg` |
| `crates/jobs/common.rs` | Test fixture: added `crawl_from_result: false` to `test_config` |
| `crates/crawl/engine.rs` | Pre-existing branch changes; `cargo fmt` fixed import ordering |

---

## Commands Executed

```bash
# Build verification
cargo test --lib        # 176 → 177 → 179 passing across phases
cargo clippy            # 0 warnings
cargo fmt --check       # clean (after fmt fix on engine.rs import ordering)

# Diff inspection
git diff HEAD           # confirmed only intended files changed
git diff HEAD crates/crawl/engine.rs  # confirmed pre-existing branch changes
```

---

## Behavior Changes (Before/After)

| Behavior | Before | After |
|----------|--------|-------|
| `axon search "query"` | Prints Tavily results, returns | Prints results, then queues one crawl job per unique origin domain, logs summary |
| Crawl seeds from private IPs | N/A | Blocked by `validate_url()`, warning logged, seed skipped |
| `ftp://` or `file://` result URLs | N/A | `extract_crawl_seed` returns `None`, silently skipped |
| Same domain appearing 3× in results | N/A | Deduped to 1 crawl job via `HashSet` |
| `--crawl-from-result true` | Flag did not exist | Crawls from exact result URL instead of origin root |
| Log output during fan-out | N/A | Single summary: `"Queued N crawl job(s): id1, id2, ..."` |
| `chrome_remote_url` via env var | Not normalized (Docker hostnames not rewritten outside Docker) | Normalized via `normalize_local_service_url` |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo test --lib` | 179 passing, 0 failed | 179 passed; 0 failed | ✅ |
| `cargo clippy` | 0 errors, 0 warnings | No output (clean) | ✅ |
| `cargo fmt --check` | No diff | No diff | ✅ |
| `grep -n "validate_url" crates/cli/commands/search.rs` | Line present | `2: use crate::crates::core::http::validate_url;` and `80: if let Err(e) = validate_url(seed)` | ✅ |
| New tests in `search.rs` | 8 unit tests for `extract_crawl_seed` | 8 tests: strips_to_origin, preserves_non_default_port, strips_deep_path, from_result_exact, unparseable_none, private_ip_guard, rejects_non_http_scheme, deduplicates_same_domain | ✅ |

---

## Source IDs + Collections Touched

No Qdrant embed/retrieve operations performed during this session. All work was source code changes.

---

## Risks and Rollback

- **Risk**: `validate_url()` is a synchronous SSRF guard; it blocks private IPs and localhost but cannot prevent DNS rebinding after the URL is enqueued. Defense-in-depth (spider's `ssrf_blacklist_patterns()`) still applies during the crawl itself.
- **Risk**: `crawl_from_result = false` (default) crawls full origin domains — a single search could queue crawls for 10 domains. Postgres dedup in `start_crawl_job` prevents duplicate active/pending jobs, but first-time searches against popular domains will queue large crawls.
- **Rollback**: Revert `crates/cli/commands/search.rs` to remove the fan-out block (lines 67–89). The `crawl_from_result` Config field is backward-compatible (defaults false) — no other rollback needed for the config changes.

---

## Decisions Not Taken

- **Move `validate_url` into `start_crawl_job`**: Would enforce the SSRF guarantee at the infrastructure boundary for all callers. Deferred — broader refactor, wrong PR. In-place guard in `search.rs` is the minimum correct fix.
- **Concurrent fan-out with `tokio::spawn`**: Crawl jobs are async enqueues (fast AMQP publish), not long-running operations. Sequential loop is simpler and the latency difference is negligible.
- **`--no-crawl` flag to opt out**: User can already achieve no-crawl via `--embed false` (crawl jobs auto-embed; without embed there's less value). Decided not to add a new flag without a concrete user request.
- **Scheme whitelist in `extract_crawl_seed` via `validate_url`**: Caller-side guard was present, but `pub` function contract needed its own guarantee independent of caller discipline.

---

## Open Questions

- Should `start_crawl_job` itself call `validate_url`? Currently all callers are internal and either call it upstream (`run_crawl`) or inline (`search.rs`), but a future caller could miss it.
- At `search_limit=10`, a typical search queues 5–10 crawls. Under high search usage this could saturate the crawl worker queue. Rate-limiting or a `--max-crawl-seeds` cap may be worth adding if usage patterns show runaway queuing.

---

## Next Steps

- Commit and push the working tree (`git add` + commit + push to `perf/command-performance-fixes`)
- Add note to PR description: `chrome_remote_url` normalization fix is a separate bug fix bundled in this PR
- Consider adding `validate_url` inside `start_crawl_job` as a follow-up hardening ticket
