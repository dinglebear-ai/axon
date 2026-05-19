# Session: Spider Alignment Investigation + Dead Config Fixes

**Date:** 2026-02-19
**Branch:** `chore/housekeeping`
**Working directory:** `/home/jmagar/workspace/axon_rust`

---

## Session Overview

Two major workstreams completed:

1. **Bench directory relocation** — moved `benches/` to `docs/benches/`, updated `Cargo.toml` with explicit `path =`, added `docs/benches/` to `.gitignore`.
2. **Spider alignment audit via 4-agent team** — dispatched parallel Explore agents against all 85+ examples in `~/workspace/spider/examples/` and the full `spider_agent` library. Agents produced four gap-analysis reports totalling ~125 KB. Acted on all dead-config findings immediately.

### Net result
- 7 config fields that were parsed, stored, and displayed but never forwarded to spider's `configure_website()` are now wired.
- `webdriver` spider feature flag added — Selenium WebDriver container (already running at port 4444) is now actually usable by the crawl engine.
- Docker Compose project name and bridge network renamed `cortex` → `axon`.
- `docs/reports/` contains four detailed alignment reports as a roadmap for future work.

---

## Timeline

| Time | Activity |
|------|----------|
| Session start | Bench file relocation (`benches/` → `docs/benches/`) |
| +5 min | Confirmed spider examples origin: 73 upstream, 12 locally authored |
| +10 min | Created `spider-alignment` team, dispatched 4 parallel Explore agents |
| +30 min | `core-api-agent` completed — 3 functional bugs + 7 high-priority gaps |
| +32 min | `ai-agent-investigator` completed — spider_agent architecture + AI integration plan |
| +33 min | `poc-pipeline-analyst` completed — spider_to_axon_poc.rs full walkthrough |
| +34 min | `chrome-perf-analyst` completed — Chrome/cache/anti-bot gap matrix |
| +35 min | Team shut down, stale ghost agents pruned from config.json manually |
| +40 min | Dead-config fields identified by cross-referencing config.rs vs engine.rs directly |
| +45 min | All 7 dead-config wires implemented in `configure_website()` |
| +50 min | `webdriver` feature added to Cargo.toml; linter regression caught and fixed |
| +55 min | `cargo check` clean — 0 errors, 11 pre-existing warnings |
| +60 min | Docker Compose + README `cortex` → `axon` rename |

---

## Key Findings

### Confirmed Bugs Fixed This Session

| # | Field | Bug | Fix Location |
|---|-------|-----|-------------|
| 1 | `chrome_remote_url` | `.with_chrome_connection()` never called — remote Chrome non-functional | `engine.rs:185` |
| 2 | `chrome_proxy` | `.with_proxies()` never called — proxy setting ignored entirely | `engine.rs:170` |
| 3 | `chrome_user_agent` | `.with_user_agent()` never called — UA always spider default | `engine.rs:173` |
| 4 | `chrome_stealth` | `with_stealth(true)` hardcoded — `--chrome-stealth false` had no effect | `engine.rs:183` |
| 5 | `chrome_anti_bot` | Same hardcoded path as stealth — field value never read | `engine.rs:183` |
| 6 | `chrome_intercept` | `RequestInterceptConfiguration::new(false)` hardcoded — `--chrome-intercept true` had no effect | `engine.rs:182` |
| 7 | `webdriver_url` | `.with_webdriver()` never called — Selenium container at port 4444 was unusable by engine | `engine.rs:177` |

### Fields Confirmed NOT Dead (agent report was wrong)
- `cache` / `cache_skip_browser` — power custom URL dedup + render mode logic in `crawl.rs` and `crawl_jobs.rs`; not spider's native cache but fully active
- `chrome_bootstrap` / `_timeout_ms` / `_retries` — fully used by `bootstrap_chrome_runtime()` in `crawl.rs`
- `webdriver_url` in `doctor.rs` / `status.rs` — used for health checks and diagnostics (separate from engine wiring)

### Spider Examples Provenance
- **73 examples** are upstream `spider-rs/spider` (official)
- **12 examples** are locally authored (not in upstream `origin/main`):
  - `spider_to_axon_poc.rs` (1938 lines) — axon integration design doc
  - `axon_cli_rust.rs`, `change_detection.rs`, `competitive_analysis.rs`, `concurrent_ai_extraction.rs`, `content_pipeline.rs`, `crawl_extract.rs`, `not_a_robot_haiku.rs`, `sitemap_quality_audit.rs`, `thc_intel.rs`

### High-Value Gaps Not Fixed This Session (deferred)
- **Dual-pass transform fallback** — POC retries thin pages with `readability=false`; axon silently drops them (`engine.rs` content pipeline)
- **Concurrent LLM extraction** — `remote_extract.rs` awaits serially; `spider_agent` Arc<Agent> with semaphore gives N× throughput
- **HTML diff optimization** — `with_remote_multimodal()` 50–70% token reduction; axon sends full HTML
- **JSONL manifest** — no per-crawl audit trail; POC writes one alongside markdown
- **i18n path exclusions** — zero default blacklist; POC has 28 language prefix defaults
- **URL filename hash suffix** — `url_to_filename()` has no hash → silent overwrites on URL collisions
- **`with_ignore_sitemap(true)`** — moot since `sitemap` feature not in Cargo.toml (not a current bug)
- **Double sitemap fetch claim** — not reproducible; `sitemap` feature not compiled in, so spider doesn't fetch internally

---

## Technical Decisions

### Why `chrome_stealth || chrome_anti_bot` for stealth
Spider has no separate `with_anti_bot()` method. `chrome_anti_bot` is a UX-level flag without a direct spider API mapping. Combining it with `chrome_stealth` via OR preserves backward-compatible behavior (both default true → stealth always on) while allowing either flag alone to enable stealth.

### Why `webdriver_url` takes priority over CDP Chrome in `configure_website()`
WebDriver (Selenium) and CDP Chrome are mutually exclusive protocols. When `webdriver_url` is set, the crawl engine should use the Selenium grid, not attempt CDP. The existing `ChromeRuntimeMode::WebDriverFallback` logic in `crawl.rs` was already setting this intent but never forwarding it to the engine.

### Why `webdriver` feature added to `Cargo.toml` rather than feature-gating the call
The `axon-webdriver` Selenium container is already in docker-compose and the `webdriver_url` config field has been in the CLI since at least the Feb 17 security audit. Keeping it as a stub no-op would be misleading. Full feature enables real `WebDriverConfig` struct construction.

### Why docker network renamed `cortex` → `axon` but Qdrant collection default stays `cortex`
User explicitly stated: "make sure the only cortex in the codebase is our qdrant collection." The collection default (`config.rs:347`) is intentional product naming. The Docker network was an unconverted leftover from the original `cortex` binary era.

### Agent report discrepancy on dead fields
The chrome-perf agent flagged `cache`/`cache_skip_browser` as dead. Direct code inspection showed they power an axon-specific URL dedup cache mechanism in `crawl.rs` and `crawl_jobs.rs` — separate from spider's native caching but fully active. Agent reports require ground-truth verification before acting.

---

## Files Modified

| File | Change | Purpose |
|------|--------|---------|
| `benches/ask_query_retrieve.rs` | Moved to `docs/benches/ask_query_retrieve.rs` | Relocate bench source out of root `benches/` |
| `Cargo.toml` | Added `path = "docs/benches/ask_query_retrieve.rs"` to `[[bench]]`; added `"webdriver"` to spider features | Explicit bench path; enable real WebDriverConfig |
| `.gitignore` | Added `docs/benches/` | Ignore relocated bench dir |
| `crates/crawl/engine.rs` | `configure_website()` — wired 7 dead config fields | Fix silent proxy/UA/stealth/intercept/remote-url/webdriver ignoring |
| `docker-compose.yaml` | `name: cortex→axon`, network `cortex→axon` (3 occurrences) | Rename Docker project + bridge network |
| `README.md` | "cortex bridge network" → "axon bridge network" | Sync doc with compose rename |

### Files Created
| File | Purpose |
|------|---------|
| `docs/benches/ask_query_retrieve.rs` | Bench source at new canonical location |
| `docs/reports/spider-alignment-core-api.md` (28 KB) | Core API gap analysis |
| `docs/reports/spider-alignment-ai-agent.md` (33 KB) | AI/agent capabilities report |
| `docs/reports/spider-alignment-poc-pipeline.md` (29 KB) | POC + content pipeline analysis |
| `docs/reports/spider-alignment-chrome-perf.md` (34 KB) | Chrome/cache/anti-bot/perf report |

---

## Commands Executed

```bash
# Bench relocation
mkdir -p docs/benches && mv benches/ask_query_retrieve.rs docs/benches/ && rmdir benches

# Spider examples provenance check
cd ~/workspace/spider && git diff origin/main HEAD --name-only -- examples/
# → 12 locally-authored files identified

# Verify engine compile after dead-config wires
cargo check
# → Finished dev profile, 0 errors, 11 pre-existing warnings (1m 06s)
```

---

## Behavior Changes (Before → After)

| Setting | Before | After |
|---------|--------|-------|
| `--chrome-proxy http://...` | Stored in Config, printed in output, silently ignored by spider | Forwarded via `with_proxies()` — spider actually uses it |
| `--chrome-user-agent "..."` | Same — ignored by spider | Forwarded via `with_user_agent()` |
| `AXON_CHROME_REMOTE_URL=http://...` | Parsed, stored, never passed to spider — remote Chrome always auto-discovered | `with_chrome_connection(url)` called — spider connects to configured endpoint |
| `--chrome-stealth false` | Hardcoded `with_stealth(true)` — flag had zero effect | Respected — `with_stealth(false)` passed when both flags false |
| `--chrome-intercept true` | Hardcoded `new(false)` — flag had zero effect | Respected — `new(true)` enables request interception |
| `AXON_WEBDRIVER_URL=http://localhost:4444` | Selenium container running but engine never connected to it | `WebDriverConfig { server_url, headless, proxy, user_agent }` wired to `with_webdriver()` |
| `docker compose up` | Created `cortex` project + `cortex` network | Creates `axon` project + `axon` network |
| `cargo bench` | Looked for `benches/ask_query_retrieve.rs` | Reads from `docs/benches/ask_query_retrieve.rs` via explicit `path =` |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check --bench ask_query_retrieve` | Clean compile from new bench path | `Finished dev profile` — 0 errors | ✅ |
| `cargo check` after engine.rs changes | 0 errors | `Finished dev profile` — 0 errors, 11 pre-existing warnings | ✅ |
| `git diff origin/main HEAD --name-only -- examples/` | Identify locally-authored files | 12 files listed | ✅ |
| `grep -n "cortex" docker-compose.yaml` | Only collection references remain | 0 results (all renamed) | ✅ |
| `grep -n "cortex" crates/core/config.rs` | Only Qdrant collection default | Lines 347, 926, 940 — all collection refs | ✅ |

---

## Source IDs + Collections Touched

*(To be filled post-embed)*

---

## Risks and Rollback

### Docker Compose network rename (`cortex` → `axon`)
- **Risk:** Any running containers on the old `cortex` network will not auto-reconnect. `docker compose down && docker compose up -d` required after apply.
- **Rollback:** Revert the 3 lines in `docker-compose.yaml`.

### WebDriver wiring + `webdriver` feature flag
- **Risk:** `thirtyfour` crate (Selenium WebDriver client) pulled as new transitive dependency — increases binary size and compile time. If Selenium container is down and `webdriver_url` is set, crawl engine gets a connection error at runtime instead of silently ignoring it.
- **Rollback:** Remove `"webdriver"` from spider features in `Cargo.toml`, revert the `cfg.webdriver_url` block in `engine.rs`.

### `chrome_stealth || chrome_anti_bot` change
- **Risk:** Previously stealth was always `true` regardless of config. Users who explicitly set `--chrome-stealth false --chrome-anti-bot false` will now get `with_stealth(false)` — which is the correct behavior, but is a behavior change from the hardcoded path.
- **Rollback:** Change `cfg.chrome_stealth || cfg.chrome_anti_bot` back to `true` in `engine.rs:183`.

---

## Decisions Not Taken

| Option | Rejected Because |
|--------|-----------------|
| Wire spider's native `cache` / `with_caching()` | axon has its own URL dedup cache logic using `previous_urls`; wiring spider's disk cache would be additive work requiring `cache` feature flag + behavioral design — deferred |
| Add `with_ignore_sitemap(true)` to prevent double-fetch | `sitemap` feature not in Cargo.toml — spider doesn't fetch sitemaps internally; the double-fetch claim was a false alarm |
| Remove `webdriver_url` config field entirely | Container already exists in docker-compose; field has been advertised in CLI help; wiring is the correct path |
| Implement `cortex agent` command with spider_agent | Significant new feature — deferred to its own session; AI agent report has full implementation sketch |
| Fix thin-page dual-pass fallback | Correct but requires content pipeline redesign — deferred |

---

## Open Questions

1. **`chrome_headless` field** — no spider Website API method for headless mode (it's configured at Chrome launch level, not Website level). Field is stored and forwarded to `WebDriverConfig.headless` but has no CDP equivalent wiring. Does the headless flag need to reach the Chrome launcher?
2. **`cache` feature** — should axon use spider's native disk cache (`cacache`) in addition to the custom URL dedup mechanism? Reports say 0 cache modes are working via spider's API — is this intentional?
3. **`chrome_intercept` default** — changed from hardcoded `false` to `cfg.chrome_intercept` (defaults `false`). Intercept blocks 3rd-party resources. Should the default be `true` to reduce bandwidth?
4. **Docker network rename impact** — any services outside docker-compose (e.g., Tailscale routing, external monitoring) referencing the `cortex` network by name will break silently.
5. **`crawl_jobs` dead code warnings** — 11 pre-existing warnings from `crawl_jobs/`. Is this module actively in development or should it be removed/completed?

---

## Next Steps

### Immediate (bugs from agent reports)
1. **Dual-pass transform fallback** — retry thin pages with `readability=false` before dropping (`engine.rs` content pipeline, ~30 lines)
2. **URL filename hash suffix** — append `{hash:016x}` in `url_to_filename()` to prevent silent overwrites (`content.rs`)
3. **`with_whitelist_url()`** — expose allow-only URL filter in Config + engine (mirror of existing blacklist)
4. **i18n path defaults** — add 28 language prefix defaults to `exclude_path_prefix` in `Config::default()`

### Medium-term
5. **JSONL manifest** — write `manifest.jsonl` alongside crawl markdown output
6. **Concurrent extraction** — refactor `remote_extract.rs` to use `Arc<Agent>` with semaphore
7. **`cortex agent` command** — implement using `spider_agent` crate; AI agent report has full Rust type sketches

### Infrastructure
8. **Bounce Docker Compose** — `docker compose down && docker compose up -d` to pick up network rename
9. **`cargo clippy`** — run after `crawl_jobs` dead code is resolved
