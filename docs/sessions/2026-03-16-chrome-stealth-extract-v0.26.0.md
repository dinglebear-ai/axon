# Session: Chrome Stealth Extract + v0.26.0 Release

**Date:** 2026-03-16
**Branch:** `feat/pulse-shell-and-hybrid-search`
**Version bump:** `0.25.3` → `0.26.0`
**Commit:** `b3bf4819`

---

## Session Overview

Executed the implementation plan at `docs/superpowers/plans/2026-03-16-extract-chrome-stealth.md` via subagent-driven development to add Chrome stealth rendering to the `axon extract` single-URL path. Completed two tasks (Task 1 in prior session, Task 2 in this session), resolved spec review and code quality review issues, applied final code reviewer fixes, then ran `/quick-push` to stage 19 uncommitted files from multiple feature branches, bump version to `0.26.0`, update CHANGELOG, and push.

---

## Timeline

1. **Task 2 spec review** — Spec compliance reviewer confirmed `PASS`; all required fields present in `ExtractWebConfig` and worker wiring correct.
2. **Task 2 code quality review (initial — FAIL)** — `engine.rs` at 525 effective lines exceeded the 500-line monolith limit. Also flagged `u8::MAX as usize` cast issue.
3. **Monolith fix** — Extracted `build_chrome_extract_website` + `run_single_url_extract_chrome` into new `crates/core/content/engine/chrome.rs` (165 lines). `engine.rs` dropped to 401 effective production lines. Commit `cb60b108`.
4. **Code quality re-review (PASS)** — Reviewer approved after extraction.
5. **Final code reviewer — three issues found**:
   - Missing CDP URL resolution (Chrome mode used raw URL, not `/json/version` discovery)
   - Missing `with_wait_for_idle_network0` for CSR framework support
   - `render_mode` not persisted in `ExtractJobConfig` (async path always used HTTP)
6. **Three-issue fix commit** (`ff50c232`) — Added `resolve_chrome_url()`, `chrome_network_idle_timeout_secs`, and `render_mode` field to `ExtractJobConfig`.
7. **`finishing-a-development-branch`** — Verified tests pass, PR option chosen.
8. **`/quick-push`** — Staged 19 uncommitted files from parallel work, bumped to `0.26.0`, updated CHANGELOG, ran cargo check, committed (`b3bf4819`, 1380 tests passed via lefthook), pushed.

---

## Key Findings

- **Spider single-URL Chrome**: `with_depth(0)` is required alongside `with_limit(1)` to prevent spider's link-find callbacks from executing on every href on the seed page.
- **CDP URL resolution**: `resolve_chrome_url()` in `engine/chrome.rs` mirrors the crawl engine's `resolve_cdp_ws_url` — ws:// shortcut, Docker hostname rewrite to `127.0.0.1`, `/json/version` fetch to extract `webSocketDebuggerUrl`.
- **`run_single_url_extract_chrome` pattern**: Uses `tokio::join!` + oneshot + biased `select!` (not `tokio::spawn`) to avoid `Send` bound requirement for spider's non-Send futures. Collected pages replayed through broadcast channel into `collect_page_results`.
- **`render_mode` in `ExtractJobConfig`**: Must use `#[serde(default = "default_render_mode")]` with `fn default_render_mode() -> RenderMode { RenderMode::Http }` for backward compat with existing DB `config_json` rows that lack this field.
- **Monolith limit enforcement**: Pre-commit hook (`enforce_monoliths.py`) excludes `#[cfg(test)] mod` blocks from line counts. `engine.rs` had 570 total lines but 401 effective production lines after extraction.

---

## Technical Decisions

- **Inline CDP resolution vs. cross-crate import**: `resolve_cdp_ws_url` in `crates/crawl` is `pub(crate)`, inaccessible from `crates/core`. Chose to inline equivalent logic in `chrome.rs` using `cdp_discovery_url` and `is_docker_service_host` which are available from `crates/core/http`.
- **`with_depth(0)` required**: Without depth=0, spider runs `set_on_link_find` callbacks for all discovered links even though `limit=1` prevents fetching them — wastes CPU and caused unexpected behavior in tests.
- **`RenderMode::Http` default for `ExtractJobConfig`**: Existing DB rows have no `render_mode` field. Using `Http` as default preserves existing behavior for jobs enqueued before this change.
- **Minor version bump (→ 0.26.0)**: Two `feat` commits in the set (`feat(vector): temporal search`, `feat(extract): Chrome stealth`) require minor bump per semver; multiple `fix` commits would only warrant patch.

---

## Files Modified

| File | Purpose |
|------|---------|
| `crates/core/content/engine.rs` | Extended `ExtractWebConfig` with 11 Chrome fields; Chrome branch in `run_extract_with_engine`; `mod chrome;` declaration; test for Chrome→HTTP fallback |
| `crates/core/content/engine/chrome.rs` | **New** — `resolve_chrome_url`, `build_chrome_extract_website`, `run_single_url_extract_chrome` |
| `crates/cli/commands/extract.rs` | Wired all 11 new `ExtractWebConfig` fields from `cfg` |
| `crates/jobs/extract/worker.rs` | Wired all 11 new fields; `render_mode` from `job_cfg.render_mode` |
| `crates/jobs/extract.rs` | Added `render_mode: RenderMode` to `ExtractJobConfig` with `serde(default)` |
| `crates/core/http/client.rs` | `build_client(timeout, user_agent: Option<&str>)` signature; `HTTP_CLIENT` reads `AXON_CHROME_USER_AGENT` |
| `crates/core/logging.rs` | Timestamps use `chrono::Local::now().format("%H:%M:%S")` |
| `crates/jobs/ingest/ops.rs` | `list_ingest_jobs` gets `source_filter: Option<&str>` param |
| `crates/ingest/github/files.rs` | `chunk_code`/`chunk_text` wrapped in `spawn_blocking` |
| `crates/ingest/github/files/batch.rs` | Hardening related to batch.rs spawning |
| `crates/jobs/refresh/schedule.rs` | `delete_refresh_schedule_with_pool` cascades to `delete_watch_def_with_pool` |
| `crates/jobs/refresh/url_processor.rs` | URL processor improvements |
| `crates/jobs/worker_lane/amqp.rs` | AMQP lane improvements |
| `crates/services/ingest.rs` | Ingest service improvements |
| `crates/services/system.rs` | System service improvements |
| `crates/vector/ops/tei/pipeline.rs` | Pipeline improvements |
| `crates/vector/ops/tei/tei_client.rs` | TEI client improvements |
| `crates/cli/commands/ingest_common.rs` | Ingest common improvements |
| `crates/cli/commands/probe.rs` | Probe improvements |
| `crates/core/health/doctor.rs` | Doctor health check improvements |
| `crates/crawl/engine/sitemap.rs` | Sitemap engine improvements |
| `config/mcporter.json` | Added `context7` MCP server (`npx -y @upstash/context7-mcp`) |
| `Justfile` | Justfile improvements |
| `Cargo.toml` | Version bump `0.25.3` → `0.26.0` |
| `Cargo.lock` | Updated by `cargo check` |
| `CHANGELOG.md` | Added `[0.26.0]` section documenting 15 commits + this one |

---

## Commands Executed

| Command | Result |
|---------|--------|
| `cargo check` | `Checking axon v0.26.0 ... Finished in 41.80s` |
| `git add . && git status --short` | 22 files staged |
| `git commit -m "feat: v0.26.0 ..."` | Pre-commit hook ran 1380 tests (0 failures), commit created `b3bf4819` |
| `git push` | `9b1291f4..b3bf4819 feat/pulse-shell-and-hybrid-search` |

---

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| `axon extract --render-mode chrome <url>` (single URL) | Used plain `reqwest` GET — no stealth, no CDP, no JS execution | Routes through Spider with Chrome stealth + fingerprint patches + `with_wait_for_idle_network0` |
| `axon extract` (async, render_mode chrome) | Worker always used HTTP regardless of originally requested mode | Worker reads `job_cfg.render_mode` from `config_json`, uses Chrome when requested |
| Logging timestamps | UTC epoch math (`1970 + secs`) | `chrono::Local::now().format("%H:%M:%S")` — readable local time |
| `axon embed <file>` HTTP client UA | Hardcoded reqwest default UA | Reads `AXON_CHROME_USER_AGENT` env var at singleton init |
| `axon ingest sessions list` | Listed all source types mixed | `source_filter: Some("sessions")` passed to SQL for clean separation |
| Refresh schedule delete | Deleted schedule only | Also deletes associated watch def |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check` | `Finished` with version 0.26.0 | `Checking axon v0.26.0 ... Finished in 41.80s` | ✓ PASS |
| Pre-commit test hook | All tests pass | `1380 tests, 0 failed` | ✓ PASS |
| `git push` | `9b1291f4..b3bf4819` | `9b1291f4..b3bf4819` pushed | ✓ PASS |
| `git log --oneline -1` | `b3bf4819 feat: v0.26.0...` | `b3bf4819 feat: v0.26.0 — Chrome stealth extract...` | ✓ PASS |

---

## Source IDs + Collections Touched

Not applicable — no Axon embed/retrieve operations were performed during implementation work. Session doc embedding follows below.

---

## Risks and Rollback

- **CDP URL resolution**: `resolve_chrome_url()` makes a network call to `/json/version`. If Chrome is unavailable, `build_chrome_extract_website` returns `None` and `run_single_url_extract_chrome` falls back to `run_single_url_extract` (HTTP). No regression risk.
- **`render_mode` in DB**: `#[serde(default)]` ensures old rows without the field deserialize as `RenderMode::Http`. Safe.
- **Rollback**: `git revert b3bf4819` reverts the version bump and uncommitted file changes. Chrome extract path requires reverting `ff50c232`, `cb60b108`, `8d58df42`, `f2150b2b` as well.

---

## Decisions Not Taken

- **Cross-crate `resolve_cdp_ws_url` import**: Would require making the crawl crate a dep of core or promoting the function to core — broke crate boundary discipline. Inlining was cleaner.
- **Keeping Chrome code in `engine.rs`**: Would leave `engine.rs` at 525 effective lines, violating monolith policy. Extraction to `engine/chrome.rs` was required.
- **`tokio::spawn` for Chrome page collection**: Would require Spider futures to be `Send`. Spider Chrome futures are `!Send` — `tokio::join!` + oneshot + `select!` is the Spider-canonical pattern.

---

## Open Questions

- GitHub Dependabot flagged 14 vulnerabilities (7 high, 7 moderate) on the default branch. These may predate this session — needs triage.
- `AXON_CHROME_USER_AGENT` was wired into `HTTP_CLIENT` init but is not documented in `.env.example`. Should add it.

---

## Next Steps

- Triage Dependabot alerts on `main` branch (14 vulnerabilities)
- Add `AXON_CHROME_USER_AGENT` to `.env.example`
- Create PR from `feat/pulse-shell-and-hybrid-search` → `main` when ready
- Test Chrome extract path against a live CSR SPA (e.g., shadcn.com) to confirm `with_wait_for_idle_network0` effect
