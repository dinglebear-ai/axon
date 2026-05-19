# Session: Dependency Updates + Spider Hedge Wiring
Date: 2026-03-21
Branch: feat/pulse-shell-and-hybrid-search

## Session Overview

Audited all Cargo dependencies for available updates, researched changelogs for key crates (spider, spider_agent, clap, rmcp), cross-referenced new spider fixes against prior session bug reports, then applied all updates including: `cargo update` (63 packages), `tokio-tungstenite` 0.28 → 0.29, spider `hedge` feature flag + `with_hedge()` wiring, and three breaking-change fixes in downstream crates. All 1477 tests pass.

---

## Timeline

| Time | Activity |
|------|----------|
| Start | Ran `cargo update --dry-run` — identified 63 packages with updates |
| Research | Scraped GitHub releases for spider 2.47.x, clap 4.6.0, rmcp 1.2.0 |
| Cross-ref | Grepped `docs/sessions/` for issues matching spider upstream fixes |
| Decision | Identified 3 actionable items: `cargo update`, tungstenite bump, hedge wiring |
| Impl | Applied all three changes + fixed 3 breaking API changes from `agent-client-protocol` 0.10.0 → 0.10.2 and spider `request_timeout` type change |
| Verify | `cargo check` clean, `cargo test --lib` 1477 passed, 0 failed |

---

## Key Findings

### Upstream Fixes Matching Prior Session Incidents

| Spider Fix | Our Session | Severity |
|-----------|-------------|----------|
| `fix: DNS resolve errors (525) skip all retry loops` | `2026-03-16-ingest-worker-dns-fix-job-recovery.md` — 38 jobs failed, manual psql reset needed | High |
| `fix: use final redirect URL as base for relative link resolution (#364)` | Redirect base URL misresolution observed in crawl sessions | Medium |
| `fix: reject empty HTML from all cache and seeded resource paths` | `2026-02-20-chrome-csr-crawl-investigation.md` — 100% thin pages on Chrome crawls | High |
| `fix: Chrome mode honors wait_for config (networkIdle) before HTML extraction` | Same session — we added `with_wait_for_idle_network0(15s)` as workaround | Medium |
| `fix: Fall back to HTTP crawl when Chrome is unavailable (#373)` | `2026-02-21-map-chrome-fallback-bugfix.md` — patched at axon layer | Low |
| `fix: 598/599 status code handling hardened` | Occasional ghost errors in crawl logs across multiple sessions | Low |

### Crate Changelog Highlights

**spider 2.46.0 → 2.47.80 (biggest update):**
- SIMD byte scanning (memchr), zero-copy page passing (`bytes::Bytes`), `Box<[u8]>` html field
- NUMA thread pinning + zerocopy wire parsing (opt-in features)
- io_uring TCP connect + file I/O (opt-in, Linux-only, not enabled by axon)
- Chrome hedged requests with variance-aware adaptive delay (we now enable this for HTTP)
- Spider Browser Cloud integration (not relevant — self-hosted Chrome)
- reqwest 0.13 ecosystem upgrade (we were already on 0.13)
- Anthropic thinking/extended-thinking support in `spider_agent`

**rmcp 1.1.0 → 1.2.0:**
- Added missing constructors for non-exhaustive model types
- Fixed: ping requests before initialize handshake now handled correctly
- Fixed: notifications without `params` field now deserialize correctly
- OAuth: granted scopes included in refresh token request

**clap 4.5.60 → 4.6.0:**
- MSRV bump to Rust 1.85 only — no functional changes

**redis 1.0.4 → 1.1.0, lapin 4.2.0 → 4.3.0:** Minor updates, no API changes.

### Breaking Changes Encountered

1. **`agent-client-protocol` 0.10.0 → 0.10.2**: `From<String>` removed from `SessionConfigOptionValue`; only `From<&str>` and `From<bool>` remain. Affected 3 call sites.

2. **spider 2.47**: `Configuration::request_timeout` field changed from `Option<Box<Duration>>` → `Option<Duration>`. Test called `.as_ref().as_millis()` which broke because `Duration` doesn't impl `AsRef<_>`.

---

## Technical Decisions

- **`with_hedge(HedgeConfig::default())`** wired in `apply_request_and_identity_settings` rather than `apply_browser_settings` — hedge applies to HTTP request pipeline only; Chrome path ignores the config field, so it's safe to always set regardless of `RenderMode`. Default: 3s delay, 1 parallel retry, whichever resolves first wins.
- **tokio-tungstenite bumped to 0.29** to match spider 2.47's new transitive dep, avoiding duplicate versions in the dep tree.
- **Pinned `spider_agent = { version = "2.46" ... }`** not changed — semver `"2.46"` resolves to `>=2.46.0, <3.0.0`, so 2.47.80 resolves automatically without Cargo.toml edit.
- **Did not enable `io_uring`, `numa`, `bloom`, or `zero_copy`** spider features — all opt-in, Linux-specific or experimental. No axon use case currently warrants them.
- **Did not wire `with_hedge()` for Chrome path** — hedging is a no-op under Chrome CDP; spider's control thread manages request lifecycle there.

---

## Files Modified

| File | Change | Purpose |
|------|--------|---------|
| `Cargo.toml` | Added `"hedge"` to spider features; bumped `tokio-tungstenite` 0.28 → 0.29 | Enable hedge feature, avoid duplicate tungstenite dep |
| `Cargo.lock` | 63 packages updated | All dependency updates |
| `crates/crawl/engine/runtime.rs` | Added `use spider::utils::hedge::HedgeConfig;`; added `website.with_hedge(HedgeConfig::default())` in `apply_request_and_identity_settings` | Wire work-stealing for slow HTTP pages |
| `crates/services/acp/persistent_conn/session_options.rs` | Lines 64, 129: `requested.to_string()` → `requested` | Fix `From<String>` removal in ACP 0.10.2 |
| `crates/services/acp/session.rs` | Line 452: `requested_model` → `requested_model.as_str()` | Fix same ACP breaking change |
| `crates/cli/commands/scrape/scrape_migration_tests.rs` | Line 419-423: `.as_ref().map(\|d\| d.as_ref().as_millis())` → `.map(\|d\| d.as_millis())` | Fix spider `request_timeout` type change: `Box<Duration>` → `Duration` |

---

## Commands Executed

```bash
# Audit available updates
cargo update --dry-run

# Apply all updates
cargo update

# Verify compile
cargo check

# Verify tests
cargo test --lib
```

---

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| HTTP crawl resilience | Slow pages block until timeout or failure | After 3s, a parallel duplicate request races; whichever resolves first wins |
| DNS failure retry | Spider retried indefinitely on DNS errors (525) | Exits retry loop immediately — no more infinite retry storms |
| Redirect link resolution | Relative links resolved against original URL, not final redirect target | Resolved against final redirect URL |
| Empty HTML in cache | Empty cached responses could be served as valid pages | Rejected and re-fetched |
| ACP model/mode setting | `String` passed to `SetSessionConfigOptionRequest::new()` | `&str` passed (matches new API contract) |
| Dep tree | Two versions of tungstenite (0.28 + 0.29) after spider update | Single version 0.29 |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo update` | 63 packages updated | 63 packages updated | PASS |
| `cargo check` | Clean compile | `Finished dev profile` in 13.80s | PASS |
| `cargo test --lib` | All tests pass | 1477 passed, 0 failed, 11 ignored | PASS |

---

## Source IDs + Collections Touched

_(No Qdrant embed/retrieve operations performed this session — this was a dependency update session only.)_

---

## Risks and Rollback

- **spider 2.47.80 behavior changes**: Large version jump (34 patch releases). Crawl behavior may differ — especially Chrome hedging, auto-bot detection (Alibaba TMD), and empty HTML rejection. Monitor first crawls after deploy.
- **`with_hedge()` adds parallel HTTP requests**: Default 3s delay with 1 hedge. If a target server rate-limits, this could trigger 429s. Mitigated by the 3s delay (only fires for slow pages) and the `with_delay()` setting already in place.
- **Rollback**: `git revert` the Cargo.toml/Cargo.lock changes + the 5 code file edits. Or pin spider back to `version = "2.46"` and revert ACP/test fixes if needed.

---

## Decisions Not Taken

- **`io_uring` feature flag**: Opt-in Linux-only kernel interface for async I/O. Skipped — no measured latency problem to solve, adds complexity, and requires Linux kernel ≥5.1.
- **`numa` / `bloom` / `zero_copy` spider features**: Exotic performance features. Not enabled — no profiling data justifying them.
- **Wiring `with_hedge()` with custom `HedgeConfig`**: Default (3s, 1 hedge) is appropriate. Custom config would require a new `Config` field and CLI flag — overkill without evidence of specific slow-page problems.
- **Spider Browser Cloud**: Requires `spider_cloud` feature + API key. We use self-hosted Chrome.

---

## Open Questions

- Does `with_hedge()` interact correctly with `with_delay()` (our polite-crawl delay setting)? Both slow the request path but for different reasons — worth verifying with a controlled crawl.
- `agent-client-protocol` 0.11.3 is available (we updated to 0.11.2). Minor version gap — monitor for any fixes in 0.11.3.

---

## Next Steps

- Deploy updated workers and run a test crawl against a known site to validate hedge behavior and confirm no regressions from spider 2.47.
- Monitor for 429 responses on sites that were previously borderline — hedge doubles request count for slow pages.
