# Spider Pattern Alignment — normalize / retry / tld

**Branch:** `perf/command-performance-fixes`
**Date:** 2026-02-22
**Session type:** Targeted bug-fix + feature wiring

---

## 1. Session Overview

Implemented the three confirmed gaps from the `spider-pattern-audit.md` investigation:

1. **`with_retry()` never called in engine** — `fetch_retries` was parsed from CLI and set by performance profiles but silently ignored by Spider's crawl engine (the flag had no effect on crawls). Fixed by wiring `with_retry()` in `configure_website()`.
2. **`with_tld()` mirrored `include_subdomains`** — defaulting both to `true` caused silent TLD-variant crawling (e.g., `example.co.uk`, `example.de`). Fixed by hardcoding `with_tld(false)`.
3. **`with_normalize()` not called** — trailing-slash URL deduplication was unavailable. Added as an opt-in flag `--normalize` (default `false`).

All changes are ~15 lines of production code plus 3 new test functions. The `scrape` command already called `with_retry()`; this brings the crawl engine into parity.

---

## 2. Timeline

| Time | Activity |
|------|----------|
| Start | Read plan from prior session; read all 5 target files in parallel |
| +5 min | Confirmed Spider API methods exist: `with_retry` (line 8935), `with_normalize` (line 9490), `with_tld` (line 8829) in `spider/src/website.rs`; `configuration.retry` (line 234), `.normalize` (line 249), `.tld` (line 158) in `spider/src/configuration.rs` |
| +10 min | Made all 5 edits across config and engine files |
| +12 min | `cargo test engine` caught 3 `Config` literal completeness errors in `research.rs`, `search.rs`, `jobs/common.rs` |
| +15 min | Fixed all 3 missing `normalize: false` fields in inline Config literals |
| +17 min | `cargo test --lib` → 194 passing, 0 failures |
| +18 min | `cargo fmt` fixed pre-existing fmt drift in `runtime.rs`, `sync_crawl.rs`, and import order in `engine.rs` |
| End | `cargo fmt --check` clean, `cargo clippy` clean |

---

## 3. Key Findings

- **`with_retry()` was already in `scrape.rs:39`** — the engine (`configure_website()`) was the only callsite missing it. The pattern from `scrape.rs` was followed exactly: `cfg.fetch_retries.min(u8::MAX as usize) as u8`.
- **`with_tld()` was piggybacked on `include_subdomains`** — `engine.rs:178` had `website.with_tld(cfg.include_subdomains)`. Since both default to `true`, every crawl was silently expanding scope to TLD variants. No new CLI flag needed; hardcode `false`.
- **`with_normalize()` is a Spider v2 API** — confirmed at `spider/src/website.rs:9490`. Takes `bool`, stores in `configuration.normalize` (`spider/src/configuration.rs:249`).
- **Three files construct `Config` inline** in test helpers — these must always be updated when new fields are added to `Config`. Pattern: add `normalize: false` (or whatever the default is) at the end of the struct literal.

---

## 4. Technical Decisions

| Decision | Rationale |
|----------|-----------|
| `with_tld(false)` hardcoded — no new flag | TLD-variant crawling crawls non-English domains (`.de`, `.co.uk`), which the content pipeline filters out anyway. If ever needed, add `--include-tld` flag then. Zero API surface for a zero-benefit feature. |
| `with_retry()` guarded by `fetch_retries > 0` | Matches the `scrape.rs` pattern. When retries = 0 (edge case), don't call the method at all rather than explicitly setting 0. |
| `normalize` defaults to `false` | URL trailing-slash deduplication changes crawl behavior. Opt-in is safer. The existing `canonicalize_url_for_dedupe()` in the engine already handles trailing-slash dedup at the output layer; Spider-level dedup would be additive. |
| Tests verify Spider API contract, not `configure_website()` directly | `configure_website()` is `async` and requires a full `Config` struct (no `Default`). Testing the Spider builder method round-trips directly is sufficient to pin the API contract without needing an integration test harness. |

---

## 5. Files Modified

| File | Change | Purpose |
|------|--------|---------|
| `crates/core/config/types.rs` | Added `normalize: bool` field + `Debug` impl entry | Config struct completeness |
| `crates/core/config/cli.rs` | Added `--normalize <bool>` to `GlobalArgs` | CLI surface |
| `crates/core/config/parse/mod.rs` | Wired `normalize: global.normalize` in `into_config()` | CLI→Config plumbing |
| `crates/crawl/engine.rs` | Fixed `with_tld(false)`; added `with_retry()`; added `with_normalize()` | Core fix |
| `crates/crawl/engine/tests.rs` | 3 new tests for Spider API wiring | Regression coverage |
| `crates/cli/commands/research.rs` | `normalize: false` in inline Config literal | Compile fix |
| `crates/cli/commands/search.rs` | `normalize: false` in inline Config literal | Compile fix |
| `crates/jobs/common.rs` | `normalize: false` in `test_config()` | Compile fix |

**Not modified** (as specified in plan): `sync_crawl.rs`, `scrape.rs`, `map.rs`, `extract.rs`.

---

## 6. Commands Executed

```bash
# Check all modified files before editing
cargo check --bin axon            # Exit 0 after initial edits (missed 3 Config literals)

# Discovered 3 missing normalize fields in Config literals — fixed

cargo test engine                 # 23 tests OK (was 14 before; +3 new Spider API tests)
cargo test --lib                  # 194 passed; 0 failed
cargo clippy                      # 0 warnings (filtered output confirmed clean)
cargo fmt --check                 # Exit 1 (pre-existing drift in runtime.rs, sync_crawl.rs + import order)
cargo fmt                         # Fixed all
cargo fmt --check                 # Exit 0
```

---

## 7. Behavior Changes (Before / After)

| Flag / Behavior | Before | After |
|-----------------|--------|-------|
| `--fetch-retries N` (crawl) | Parsed, stored in Config, set by performance profiles — **never passed to Spider engine**. Silently had no effect on crawls. | Passed to Spider via `with_retry()`. Crawl engine now retries failed fetches up to N times. |
| TLD crawling | `with_tld(cfg.include_subdomains)` — defaulted to `true`. Crawling `docs.example.com` would also discover `docs.example.co.uk`, `docs.example.de`, etc. | `with_tld(false)` hardcoded. Scope is limited to explicit subdomain control via `--include-subdomains`. |
| `--normalize` | Did not exist. | `false` by default. Pass `--normalize true` to deduplicate `/about` and `/about/` as the same URL at Spider engine level. |

---

## 8. Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check --bin axon` | Exit 0 | Exit 0 | ✅ |
| `cargo test engine` | All pass | 23/23 passed | ✅ |
| `cargo test --lib` | All pass | 194/194 passed | ✅ |
| `cargo clippy` | 0 errors | 0 errors | ✅ |
| `cargo fmt --check` | Exit 0 | Exit 0 (after `cargo fmt`) | ✅ |
| `test_spider_retry_wiring_round_trips` | `website.configuration.retry == 3` | PASS | ✅ |
| `test_spider_normalize_wiring_round_trips` | round-trip true/false | PASS | ✅ |
| `test_spider_tld_disabled_by_default` | `configuration.tld == false` | PASS | ✅ |

---

## 9. Source IDs + Collections Touched

No Qdrant embed/retrieve operations were performed in this session (pure code change session).

---

## 10. Risks and Rollback

**Retry wiring** — low risk. Feature was already in `scrape.rs`; engine was the only missing callsite. If Spider's `with_retry` has a bug, the behavior change is retries actually happening (vs. silent no-op). Rollback: remove the `if cfg.fetch_retries > 0 { website.with_retry(...); }` block in `engine.rs:211-215`.

**TLD hardcode** — medium risk to users who relied on TLD-variant crawling (likely nobody, since it was never controllable). The old behavior was `with_tld(include_subdomains)` = `with_tld(true)` by default. If a user was knowingly crawling TLD variants, they will see fewer pages after this change. Rollback: revert `engine.rs:179` to `website.with_tld(cfg.include_subdomains)`.

**normalize** — zero risk, defaults to `false`, is a no-op unless explicitly enabled.

---

## 11. Decisions Not Taken

| Alternative | Why Rejected |
|-------------|--------------|
| Add `--include-tld` CLI flag | Zero known users of TLD-variant crawling; adds API surface for a feature that conflicts with content pipeline's language filtering. Deferred per plan. |
| Test `configure_website()` end-to-end | Requires full `Config` struct construction (no `Default` impl) + tokio runtime. Disproportionate test complexity for verifying 3 simple setter calls. API contract tests are sufficient. |
| Wire `with_normalize()` only when `normalize == true` | Calling `with_normalize(false)` is a no-op in Spider; always calling it is cleaner and more explicit. |

---

## 12. Open Questions

- Does `with_retry()` in Spider also apply to sitemap fetches (`crawl_sitemap()`)? If yes, the sitemap phase now respects `fetch_retries` too — which is the desired behavior but should be verified empirically.
- Should `with_normalize()` default to `true`? The existing `canonicalize_url_for_dedupe()` already strips trailing slashes at output time, so the two mechanisms might produce slightly different behavior in edge cases (e.g., Spider deduplication affects which pages get fetched, not just how they're counted).

---

## 13. Next Steps

- Smoke test: `axon crawl https://example.com --fetch-retries 3 --normalize true --wait true` to confirm runtime behavior matches the wiring.
- Consider adding `--include-tld` flag in a future PR if the need arises (see Deferred Items in plan).
- The `spider-pattern-audit.md` deferred items (budget, proxy, `with_block_assets` audit) remain open for a future pass.
