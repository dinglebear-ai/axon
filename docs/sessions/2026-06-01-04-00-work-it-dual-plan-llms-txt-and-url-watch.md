# Session: work-it dual-plan — llms.txt probe (#152) + URL change-detection watch (#151)

- **Date:** 2026-06-01
- **Coordinator branch / HEAD:** `main` @ `129dbe51`
- **Skill:** `vibin:work-it` run over `@docs/superpowers/plans/2026-05-31` (two plans, executed in parallel, each in its own isolated worktree)
- **PRs:**
  - [#152 — llms.txt probe → v4.17.0](https://github.com/jmagar/axon/pull/152) — branch `feat/llms-txt-probe`, worktree `.worktrees/llms-txt-probe`
  - [#151 — URL change-detection watch → v4.18.0](https://github.com/jmagar/axon/pull/151) — branch `feat/url-watch-change-detection`, worktree `.worktrees/url-watch-change-detection`

## What was built

### #152 — `/llms.txt` probe (plan `2026-05-31-llms-txt-probe.md`, 10 tasks)
Probe `/llms.txt` at a site root during crawl and `map`, parse its markdown links (`pulldown-cmark`), resolve + host-scope them like sitemap URLs, and **union** them (dedup, no blanket truncation) into the same backfill candidate set as sitemap discovery. New `src/crawl/engine/llms_txt.rs`; shared `loc_in_scope`; `.md`/`.markdown`/`.txt` passthrough; discovery-document body-size cap (llms.txt 512 KB, sitemap 50 MB) with HTML page backfill left uncapped + charset-aware. Config `scrape.discover-llms-txt` (default on) / `scrape.max-llms-txt-urls` (512) across all layers + request surfaces. Closes beads `axon_rust-6s51.1`–`.5`.

### #151 — URL change-detection watch (plan `2026-05-31-url-watch-change-detection.md`, 12 tasks)
Turn `watch` into a URL change detector: per-tick conditional probe (ETag/Last-Modified) → scrape → normalize + ignore-pattern filter → SHA-256 fast-equal skip → `services::diff::compute_diff` vs a stored snapshot (`axon_watch_url_state`, migration `0007`) → meaningfulness threshold → LLM summary + `url-change` artifact → clustered, in-flight-guarded crawl-on-change. `task_type` cut over `refresh` → `watch` (with a runtime back-compat alias).

## Process (work-it pipeline, both tracks)

1. **Isolated worktrees** branched from HEAD; rebased onto `origin/main` after discovering the local base was one commit stale (the `fix(plugin): ship axon setup script` commit) — removed a spurious plugin-revert from both diffs.
2. **Implementation agents** (one per worktree) executed each plan via `superpowers:executing-plans`, committing per task, gating to green `--lib`.
3. **PRs opened immediately** once green (to start external review early).
4. **Internal review waves** — 11 report-only agents on #151, 9 on #152 (security, data-integrity, correctness, silent-failure, tests, type-design, comments, simplicity, 3× simplifier). Consolidated, vetted findings applied by single per-worktree fix agents.
5. **CI gate fixes** — #152: split `map_with_sitemap` under the monolith cap, registered the new fields in the MCP schema-doc generator (`CRAWL_FIELD_DESCRIPTIONS`), regenerated the OpenAPI spec/types, and bumped `apps/web/package.json`/lock to satisfy `version_bearing_files_stay_in_sync`.
6. **External review** (CodeRabbit / Copilot / cubic / codex) addressed in two rounds; all threads resolved with replies (fix-or-rationale).
7. **CI green** on both PRs; all review threads resolved.

## High-value findings the review waves caught (beyond cosmetics)

- **#152 — two silent behavior regressions:** the merged backfill was capping the previously-uncapped sitemap-URL path at 512 (repurposing a doc-count knob), and the 512 KB body cap + strict UTF-8 had been applied to the *shared* fetch helper, silently dropping large sitemaps/HTML pages and non-UTF8 bodies. Fixed: cap scoped to discovery docs; sitemap union uncapped; lossy decode.
- **#151 — dead code:** link-change detection read a `links` payload field no scrape path populated; `build_scrape_json` now emits anchor-scoped links. **Snapshot-clobber storm:** `.ok().flatten().unwrap_or_default()` + full-row `ON CONFLICT` could NULL a just-written snapshot on a transient DB error → re-crawl loop; fixed via Err/None distinction + targeted `set_crawl_job_id` upsert. **run-now/scheduled race:** confirmed `run-now` (CLI/REST) genuinely can overlap a leased scheduled run; `run_id`+pool now threaded instead of a "newest running run" SQL lookup.
- **Bots caught real flaws in the fixes themselves:** the anchor link helper was not anchor-scoped (matched `<link>`/`data-href`); the probe-failure fallback wiped ETag/Last-Modified; `join_origin_path` could leak `user:pass@` credentials — all fixed.
- **SSRF posture verified safe** end-to-end on both (per-hop redirect re-validation + DNS-rebind resolver + host-scope drop), with added defense-in-depth (CLI `validate_url`, crawl-seed validation).

## Versioning / merge order

Pre-assigned to avoid a cross-PR collision: **#152 = 4.17.0**, **#151 = 4.18.0**. Intended merge order: **#152 first, then #151** (rebase). Both bump `Cargo.toml`, `Cargo.lock`, `README.md`, `CHANGELOG.md`, and `apps/web` (package.json/lock + openapi). `plugins/axon/.claude-plugin/plugin.json` has no version field.

## Verification (final)

- Both PRs: `cargo test --lib` green (#152 ≈ 2359; #151 ≈ 2379, +tests, 0 failed); full-workspace `test`, `rest-api-parity` (incl. `openapi:check`), `monolith`, `mcp-schema-doc-sync`, `clippy`, `fmt` all green in CI.
- Both PRs: **0 unresolved review threads.**

## Known / pre-existing (not introduced here)

- `just verify` is red on `main` itself: its `validate-plugin` step reads root `.claude-plugin/plugin.json`, which moved to `plugins/axon/.claude-plugin/` in an earlier plugin split. Every real CI gate passes individually; intentionally not bundled into these feature PRs.

## Remaining risks / follow-ups

- Vertical scrape payloads still omit `links` (TODO note left in `src/services/scrape.rs`) — link-change detection works for the generic scrape path only.
- The watch first-run path is covered by a live-network integration test; deeper offline branch coverage (304 short-circuit, fast-equal skip) would need a probe/scrape injection seam (noted by the test reviewer).
