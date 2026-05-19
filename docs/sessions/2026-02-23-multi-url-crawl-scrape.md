# Session: Multi-URL Support for crawl + scrape (batch elimination)

**Date:** 02/23/2026
**Branch:** `fix-crawl`
**Duration:** Single focused implementation session + smoke-test round

---

## Session Overview

Implemented multi-URL support for `axon crawl` and `axon scrape`, eliminating the need for `axon batch` to handle multiple URLs. Previously both commands accepted exactly one URL; now they accept any number via positional args or `--urls` CSV. The async crawl path uses the pre-existing `start_crawl_jobs_batch` function (single PgPool + AMQP connection for N jobs). `axon batch` is preserved pending deletion in a separate commit.

**Bonus fix found during smoke testing:** clap argument ID clash between positional field `urls: Vec<String>` in `CrawlArgs`/`ScrapeArgs` and the global `--urls` flag in `GlobalArgs` — both had ID "urls", causing `--urls` to show as "unexpected argument" for the `crawl` subcommand. Fixed by renaming the positional field to `positional_urls`. Also patched a pre-existing `reused_pages` missing-field compile error in `crates/crawl/engine/tests.rs`.

---

## Timeline

1. **Read plan** — ingested the full implementation plan from the session transcript
2. **Read all source files** — `cli.rs`, `parse/mod.rs`, `crawl.rs`, `scrape.rs`, `mod.rs`, `common.rs`, `crawl_jobs/mod.rs`, `audit/mod.rs`
3. **Verified `start_crawl_jobs_batch` already exists** — `crates/jobs/crawl_jobs/mod.rs:25`
4. **Applied 5-step changes** in order: config structs → parse arms → command handlers → dispatch
5. **`cargo check` + `cargo test --lib -q`** — clean compile, 337 tests passed
6. **Ran smoke tests** — multi-URL scrape and crawl worked; `--urls` CSV failed
7. **Diagnosed clap ID clash** — `CrawlArgs.urls` shadowed global `--urls` flag; confirmed same pre-existing bug in `BatchArgs`
8. **Fixed**: renamed positional field to `positional_urls` in `CrawlArgs` + `ScrapeArgs`; fixed `engine/tests.rs` missing `reused_pages`
9. **Reran smoke tests** — all passed

---

## Key Findings

- `start_crawl_jobs_batch` at `crates/jobs/crawl_jobs/mod.rs:25` already existed and matched the plan exactly — no new job infrastructure needed
- **Clap ID clash (bug found in testing):** When a positional field in a subcommand struct has the same name as a global `--flag`, clap registers both with the same argument ID. The subcommand's local arg wins, silently hiding the global flag for that subcommand. Affected: `CrawlArgs.urls` (new), and pre-existing in `BatchArgs.urls` + `ExtractArgs.urls`.
- `audit::run_crawl_audit(cfg, start_url)` still takes `start_url`; with multi-URL positionals, the audit URL is now derived from `cfg.positional.get(1)` in the match arm — not passed down from `run_crawl`
- The old `run_async_enqueue` used single-URL `start_crawl_job`; replaced entirely with `run_async_enqueue_multi` using `start_crawl_jobs_batch`
- `crates/crawl/engine/tests.rs:4` had a pre-existing missing-field error (`reused_pages: u32` added to `CrawlSummary` in a prior session but not propagated to the test helper)

---

## Technical Decisions

- **`scrape` is sequential**: looping `scrape_one` per URL. Single-page, cheap SSRF guard + spider setup — no semaphore needed.
- **`crawl` async uses `start_crawl_jobs_batch`**: single PgPool + AMQP connection for N jobs, per-URL dedup baked in.
- **`crawl` sync loops `run_sync_crawl`**: sequential, no parallel concern.
- **Renamed positional to `positional_urls`**: avoids shadowing the global `--urls` flag. `BatchArgs`/`ExtractArgs` have the same pre-existing issue but are out of scope for this plan.
- **Audit URL from `positional.get(1)`**: kept `run_crawl_audit` signature unchanged; derive URL in the match arm — minimal blast radius.
- **`axon batch` not deleted**: kept pending separate clean-up commit.

---

## Files Modified

| File | Change |
|------|--------|
| `crates/core/config/cli.rs` | Added `ScrapeArgs { positional_urls: Vec<String> }`; `CliCommand::Scrape(UrlArg)` → `CliCommand::Scrape(ScrapeArgs)`; `CrawlArgs.url: Option<String>` → `CrawlArgs.positional_urls: Vec<String>` |
| `crates/core/config/parse/mod.rs` | `Scrape` arm: `args.value.into_iter()...` → `args.positional_urls`; `Crawl` arm: `args.url.into_iter()...` → `args.positional_urls` |
| `crates/cli/commands/crawl.rs` | Dropped `start_url` param from `run_crawl`/`maybe_handle_subcommand`; added `parse_urls` + `start_crawl_jobs_batch` imports; replaced `run_async_enqueue` with `run_async_enqueue_multi`; audit URL from `cfg.positional.get(1)` |
| `crates/cli/commands/scrape.rs` | Added `parse_urls` import; extracted `scrape_one(cfg, url)`; `run_scrape(cfg)` loops over `parse_urls(cfg)` |
| `mod.rs` | `run_scrape(cfg, start_url)` → `run_scrape(cfg)`; `run_crawl(cfg, start_url)` → `run_crawl(cfg)` |
| `crates/crawl/engine/tests.rs` | Added missing `reused_pages: 0` to `CrawlSummary` struct literal in `summary()` test helper (line 4) |

---

## Commands Executed

```bash
cargo check --bin axon 2>&1 | grep -E "^error"
# → (no output — clean)

cargo test --lib -q
# → test result: ok. 337 passed; 0 failed; 0 ignored

./scripts/axon scrape https://example.com https://httpbin.org/get
# → scraped both pages sequentially, embedded 7 chunks each into cortex

./scripts/axon crawl https://example.com https://httpbin.org
# → Crawl Job f60ed714 → https://example.com/ | Crawl Job b864938c → https://httpbin.org/

./scripts/axon crawl --urls "https://example.com,https://docs.rs"
# → (first attempt) error: unexpected argument '--urls' found  ← clap ID clash
# → (after fix)     Crawl Job e6b520ca → https://example.com/ | Crawl Job add77491 → https://docs.rs/

./scripts/axon crawl https://example.com
# → Job ID: fa35ca49...  (single URL, no "+N more")

./scripts/axon crawl list
# → listed all prior crawl jobs including the two new multi-URL ones
```

---

## Behavior Changes (Before/After)

| Command | Before | After |
|---------|--------|-------|
| `axon scrape <url>` | Single URL | Works (backward compat) |
| `axon scrape <url1> <url2>` | Error / ignored | Both scraped + embedded sequentially |
| `axon crawl <url>` | Single URL | Works (backward compat), no `(+N more)` |
| `axon crawl <url1> <url2>` | Error / ignored | 2 jobs enqueued, `(+1 more)` in header |
| `axon crawl --urls "u1,u2"` | Broken (only first URL) | Both URLs processed ✓ (after clap fix) |
| `axon crawl audit <url>` | Broken (passed `"audit"` as the URL) | Fixed: URL from `cfg.positional.get(1)` |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check --bin axon` | 0 errors | 0 errors | PASS |
| `cargo test --lib -q` | 337 pass | 337 passed, 0 failed | PASS |
| `axon scrape https://example.com` | 1 page, embedded | 7 chunks into cortex | PASS |
| `axon scrape https://example.com https://httpbin.org/get` | 2 pages scraped + embedded | Both scraped, 7 chunks each | PASS |
| `axon crawl https://example.com https://httpbin.org` | 2 job IDs printed | `f60ed714` + `b864938c` both running | PASS |
| `axon crawl --urls "https://example.com,https://docs.rs"` | 2 job IDs | `e6b520ca` + `add77491` | PASS (after fix) |
| `axon crawl https://example.com` | 1 job, no `(+N more)` | `fa35ca49`, clean output | PASS |
| `axon crawl list` | All jobs visible | All 7+ jobs listed | PASS |

---

## Source IDs + Collections Touched

| Source | Collection | Outcome |
|--------|-----------|---------|
| `https://example.com` | `cortex` | 7 chunks embedded (scrape smoke test) |
| `https://httpbin.org/get` | `cortex` | 7 chunks embedded (scrape smoke test) |
| `docs/sessions/2026-02-23-multi-url-crawl-scrape.md` | `cortex` | 1 chunk embedded (session doc) |

---

## Risks and Rollback

- **Risk**: `axon crawl audit <url>` URL derivation changed. Old behavior would have passed `"audit"` as the URL string (immediately failing `validate_url`), so this is a fix, not a regression.
- **Clap ID clash pre-existing in `BatchArgs` + `ExtractArgs`**: `--urls` CSV is still broken for `axon batch` and `axon extract`. Not introduced by this session — pre-existing. Fix is the same rename to `positional_urls` when those commands are touched.
- **Rollback**: `git revert HEAD` or `git diff HEAD~1` covers all 6 changed files cleanly.

---

## Decisions Not Taken

- **Parallel scrape with semaphore**: Rejected — single-page scrape is fast; no throughput gain worth the complexity.
- **Modify `run_crawl_audit` signature**: Would chain changes through `audit/mod.rs`, `manifest_audit.rs`, `sitemap.rs`. Derive URL in the match arm instead — smaller blast radius.
- **Fix `BatchArgs`/`ExtractArgs` ID clash now**: Out of scope for this plan; noted as known issue.
- **Delete `axon batch` now**: Kept until smoke tests confirmed working — deletion is a clean separate commit.

---

## Open Questions

- Should `axon crawl --wait true <url1> <url2>` run sync crawls in parallel? Currently sequential. Parallelism via `FuturesUnordered` would be possible but adds complexity — wait for a user request.
- `axon batch --urls` and `axon extract --urls` are still broken (pre-existing clap ID clash). Worth fixing in a follow-up if users hit this.

---

## Next Steps

1. ~~Smoke test multi-URL scrape~~ ✓
2. ~~Smoke test multi-URL crawl~~ ✓
3. ~~Smoke test `--urls` CSV~~ ✓
4. Delete `axon batch` in a separate commit (smoke tests passed)
5. Fix `BatchArgs.urls` / `ExtractArgs.urls` → `positional_urls` to unblock `--urls` CSV for those commands
6. Update `CLAUDE.md` / README to document multi-URL crawl/scrape behavior
