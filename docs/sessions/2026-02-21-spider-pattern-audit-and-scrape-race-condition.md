# Spider Pattern Audit & scrape_raw() Race Condition Research

**Date:** 2026-02-21
**Branch:** `perf/command-performance-fixes`
**Duration:** Full session

---

## Session Overview

Two-phase session:

1. **Spider pattern audit** — dispatched a 4-agent team to systematically compare all `crates/cli/commands/` implementations against the official Spider library (`~/workspace/spider`). Produced a comprehensive gap analysis report and fully rewrote `scrape.rs` to align with Spider's API.

2. **`scrape_raw()` race condition research** — after the rewritten scrape command failed with "spider page list is empty", investigated the root cause, applied a workaround, then researched the bug's complete history across Spider's GitHub issues, PRs, CHANGELOG, and git log.

---

## Timeline

| Step | Activity |
|------|----------|
| 1 | Dispatched 4-agent team: `spider-mapper`, `core-cmd-auditor`, `crawl-cmd-auditor`, `ingest-cmd-auditor` |
| 2 | Synthesized findings into `docs/reports/spider-pattern-audit.md` |
| 3 | Dispatched agent to rewrite `scrape.rs` using Spider's `Website` API + TDD |
| 4 | Ran `axon scrape https://spider.cloud/docs/core/efficient-scraping` → failed: "spider page list is empty" |
| 5 | Investigated root cause: biased-select race in `scrape_raw()` |
| 6 | Applied workaround: `subscribe(16) + tokio::spawn + crawl_raw()` |
| 7 | Scrape confirmed working |
| 8 | Researched bug history: GitHub issues, PRs, git log, CHANGELOG, version tags |
| 9 | Discovered: bug introduced in v2.44.4 (Feb 3 2026), no upstream issue filed |
| 10 | Discovered: `spider_cli` accidentally bypasses the broken API entirely |
| 11 | Wrote full root cause analysis to `docs/reports/scrape-raw-race-condition.md` |

---

## Key Findings

### Spider Pattern Audit

- **`scrape.rs`** — originally bypassed Spider entirely (manual `reqwest::Client` fetch). Fully rewritten to use `Website` builder API.
- **`sync_crawl.rs`** — **broken (P0)**: references two undefined functions: `run_sitemap_only_crawl` (should be `engine::run_sitemap_only()`) and `append_sitemap_backfill` (should be `append_robots_backfill`).
- **`engine.rs`** — calls `website.persist_links()`, an undocumented Spider internal API (P1).
- **`extract.rs`** — reinvents Spider's native LLM extraction instead of using it (P2).
- Full report: `docs/reports/spider-pattern-audit.md`

### scrape_raw() Race Condition

**Root cause:** Three interacting issues in `website.rs:4930–4971` (Spider v2.44.4+):

1. `tokio::join!(sub, crawl)` — cooperative scheduling on a single Tokio task
2. `biased;` in the `select!` — `done_rx` arm always wins when resolved
3. For fast single-page fetches: `crawl` completes (including `done_tx.send(())`) before `sub` ever drains `rx2.recv()` → page is buffered but never consumed → `get_pages()` returns empty

**Version history:**

| Version | Behavior | Bug |
|---------|----------|-----|
| `≤ v2.44.3` | `while let Ok() = rx2.recv().await` | **Hangs** — missing `w.unsubscribe()` (issues #268, #269) |
| `v2.44.4+` | `tokio::join!` + `biased; done_rx` | **Drops pages** on fast single-page fetches |
| `v2.45.20` (axon locked) | Same as above | **Affected** |
| `v2.45.24` (current latest) | Same as above | **Affected — not fixed** |

**The biased-fix commit:** `441c3712` "chore(website): fix scrape subscription hang", Feb 3 2026 — fixed the hang but introduced the race. No upstream issue tracks Bug 2.

**Why no one reported it:** `spider_cli`'s SCRAPE command never calls `scrape_raw()` at all — it uses `tokio::spawn + crawl_raw() + subscribe()` directly, which is the correct pattern. CLI users are unaffected; only library users calling `scrape_raw()` directly hit the bug.

### Our Workaround

`crates/cli/commands/scrape.rs:115–129`:
```rust
let mut rx = website.subscribe(16).ok_or("failed to subscribe to spider broadcast")?;
let collect: tokio::task::JoinHandle<Option<Page>> =
    tokio::spawn(async move { rx.recv().await.ok() });
match cfg.render_mode {
    RenderMode::Http | RenderMode::AutoSwitch => website.crawl_raw().await,
    RenderMode::Chrome => website.crawl().await,
}
website.unsubscribe();
let page = collect.await.map_err(|e| ...)?.ok_or("spider returned no page")?;
```

This is identical in spirit to what `spider_cli` does. Independent `tokio::spawn` task, 16-slot broadcast buffer, no biased-select race window.

---

## Technical Decisions

- **`subscribe(16)` not `subscribe(0)`** — `subscribe(0)` maps to `DEFAULT_PERMITS` (CPU count), which is fine, but 16 makes the buffer size explicit and sufficient for single-page use. Avoids relying on a CPU-count heuristic.
- **`tokio::spawn` not `tokio::join!`** — independent task eliminates the cooperative scheduling race entirely. This is the correct pattern regardless of Spider version.
- **Workaround, not upstream PR** — the bug has been in the wild 18 days with no reports. We document it fully and work around it rather than waiting on an upstream fix that may not come.

### Possible Upstream Fixes (documented, not submitted)

| Fix | Approach | Verdict |
|-----|----------|---------|
| A | Drain `rx2` after `done_rx` fires with `try_recv()` | Correct — minimal patch |
| B | Remove `biased;` | Partial — still 50% drop rate |
| C | Revert to `while let Ok()` + add `w.unsubscribe()` | Correct — simpler |
| D | Our workaround: `tokio::spawn` | Correct — what we use |

---

## Files Modified

| File | Change |
|------|--------|
| `crates/cli/commands/scrape.rs` | Full rewrite — Spider `Website` API, `subscribe(16)+spawn+crawl_raw()` workaround, 16 tests |
| `docs/reports/spider-pattern-audit.md` | Created — full audit report (4-agent team findings) |
| `docs/reports/scrape-raw-race-condition.md` | Created — complete root cause analysis with version history, all fixes |

---

## Commands Executed

```bash
# Confirmed scrape works after workaround
./scripts/axon scrape https://spider.cloud/docs/core/efficient-scraping
# → full markdown returned

# Spider version checks
cd ~/workspace/spider && git log --oneline -5
# HEAD is v2.45.24

cargo search spider --limit 3
# spider = "2.45.24"

# Confirmed bug NOT fixed in v2.45.21–v2.45.24
cd ~/workspace/spider && git log --oneline v2.45.20..HEAD
# 5 commits, all cache-related

cd ~/workspace/spider && git diff v2.45.20..HEAD -- spider/src/website.rs | grep "biased\|done_tx\|scrape_raw"
# No changes to scrape_raw() — bug still present

# Found the introducing commit
cd ~/workspace/spider && git log --oneline --grep="scrape" -- spider/src/website.rs
# 441c3712 chore(website): fix scrape subscription hang  ← introduced the race
```

---

## Behavior Changes (Before/After)

| Aspect | Before | After |
|--------|--------|-------|
| `axon scrape <url>` | Called `website.scrape_raw().await` — returned empty pages on fast fetches | Uses `subscribe(16)+spawn+crawl_raw()` — page reliably collected |
| `scrape.rs` source of HTML | Manual `reqwest::Client` fetch, bypassed Spider | Spider `Website` builder API |
| Test coverage | 0 tests | 16 tests covering format output, SSRF guards, config mapping |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `axon scrape https://spider.cloud/docs/core/efficient-scraping` | Full markdown content | Full markdown returned | ✅ |
| `cargo test --lib` | All tests pass | 169 passing (16 new) | ✅ |
| `git diff v2.45.20..HEAD -- spider/src/website.rs \| grep biased` | No changes | No output | ✅ Bug confirmed unfixed |
| `cargo search spider --limit 3` | Latest version | `2.45.24` | ✅ |

---

## Source IDs + Collections Touched

No Qdrant/TEI operations performed during this session (pure research + code changes).

---

## Risks and Rollback

- **`scrape.rs` rewrite** — 16 tests pass; the only behavioral change is using Spider's API instead of manual `reqwest`. Rollback: `git checkout crates/cli/commands/scrape.rs`.
- **Workaround vs upstream API** — our `subscribe(16)+spawn` pattern diverges from `scrape_raw()` but is identical to `spider_cli`'s own implementation. Risk: if Spider fixes `scrape_raw()` in a future version, we should migrate back. Not urgent — they haven't noticed in 18 days.

---

## Decisions Not Taken

- **File upstream PR** — bug is 18 days old, unnoticed, and our workaround is stable. Not worth the churn until upstream responds.
- **Pin Spider to pre-v2.44.4** — downgrading would restore Bug 1 (infinite hang). Our workaround handles Bug 2 without needing to downgrade.
- **Remove `biased;` ourselves** — we could patch our local Spider fork, but it's unnecessary since we don't call `scrape_raw()` at all.

---

## Open Questions

- **P0: `sync_crawl.rs` compiler errors** — two undefined function references (`run_sitemap_only_crawl`, `append_sitemap_backfill`). Not fixed this session. Needs resolution before merge.
- **P1: `engine.rs` `persist_links()` internal API** — relies on undocumented Spider internals. Should be replaced with the documented sitemap API pattern.
- **Should we file an upstream issue?** — the biased-select race (Bug 2) has no upstream tracking. Filing would benefit other library users.

---

## Next Steps

1. Fix `sync_crawl.rs` P0: replace `run_sitemap_only_crawl` → `engine::run_sitemap_only()` and `append_sitemap_backfill` → `append_robots_backfill`
2. Fix `engine.rs` P1: replace `persist_links()` with documented sitemap API
3. Consider filing upstream Spider issue for the `scrape_raw()` biased-select race condition (v2.44.4+)
4. When Spider ships a fix for `scrape_raw()`, migrate `scrape.rs` back to `scrape_raw()` or keep the cleaner direct pattern
