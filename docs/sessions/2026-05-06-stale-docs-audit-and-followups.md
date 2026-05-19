---
date: 2026-05-06 19:13:36 EST
repo: git@github.com:jmagar/axon.git
branch: main
head: b5efbc28
agent: Claude (claude-sonnet-4-6 / claude-opus-4-7)
working directory: /home/jmagar/workspace/axon_rust
---

## User Request

Wire `plugin.json` to `.mcp.json` and configure userConfig for the axon plugin; then "dispatch a team of agents to do a comprehensive, systematic, thorough, and complete stale docs hunt — leave no stone unturned in your epic quest to deliver 100% accurate and complete docs for the entire repo." Then iterate through deletion, rewrite, and 6 follow-up code/doc fixes.

## Session Overview

Wired the axon Claude plugin manifest, then ran a 6-agent parallel audit of every active doc in the repo, then a 4-agent deletion/archival pass, then a 4-agent rewrite of the highest-priority docs (SECURITY/JOB-LIFECYCLE/OPERATIONS + root cleanup), then a 4-agent follow-up wave that implemented per-job heartbeats, cancellation tokens, size-based logging, an API-token doc rewrite, a Chrome endpoint security audit, and an openssl CVE patch. 7 commits totaling ~110 unique files modified.

## Sequence of Events

1. Confirmed no `axon` MCP tools were registered in the active session
2. Added `"mcp": "./plugins/axon/.mcp.json"` to `.claude-plugin/plugin.json`
3. Populated `plugins/axon/.mcp.json` with `axon mcp` stdio server entry
4. Fetched Claude Code plugins-reference docs to learn `userConfig` schema
5. Added `userConfig` block (8 fields, 2 sensitive) to `plugin.json`
6. Wired `${user_config.*}` substitutions into `.mcp.json` env block
7. Bumped 1.5.3 → 1.5.4, committed, fast-forward merged branch → main
8. Dispatched 6 parallel audit agents (root, commands, MCP, per-crate, plugin skills, misc docs)
9. Synthesized master report at `docs/reports/2026-05-06-stale-docs-audit/00-MASTER-REPORT.md`
10. Executed deletion/archive pass (7 deletes, 14 moves, 8 cross-ref cleanups)
11. Regenerated `docs/MCP-TOOL-SCHEMA.md` via `scripts/generate_mcp_schema_doc.py`
12. Fixed `config.rs:267` doc-default (64 → 150) and `status_dashboard.html` dead sections
13. Dispatched 4 parallel rewrite agents (SECURITY, JOB-LIFECYCLE, OPERATIONS, root CLAUDE/README)
14. Dispatched 4 parallel follow-up agents (heartbeat+cancellation, logging+API-TOKEN, Chrome security, dependabot)
15. Hit 124-line monolith violation; extracted `try_enqueue_embed_handoff()` helper
16. Bumped 1.5.4 → 1.5.6 (1.5.5 was security-only stub, 1.5.6 covers feature additions)
17. Discovered I had been incorrectly restoring user's `plugins/`-rooted layout to `plugins/axon/` 3 times via misdiagnosed `git checkout`; properly moved files via `git mv` to match the manifest

## Key Findings

- 17 docs were wholesale-obsolete from the pre-lite-mode era (Postgres + AMQP + Redis + Pulse UI removed months ago, docs lagged behind)
- `docs/auth/MCP-AUTH.md` documented a Google OAuth broker / `atk_` tokens / dynamic client registration that don't exist in code (real auth is just `AXON_MCP_HTTP_TOKEN` bearer)
- `crates/jobs/CLAUDE.md` claimed a "Tier 2 content-aware heartbeat" with constants `STALE_STREAK_WARN_THRESHOLD` / `STALE_STREAK_KILL_THRESHOLD` — none existed anywhere in the tree (heartbeat was actually missing entirely)
- `apps/web` and `crates/web/` were far smaller than docs claimed — Pulse chat surface, `/app/api/` routes, `/ws`, `/output/*`, `/download/*` all gone
- 5 of 8 per-crate `CLAUDE.md` files contained fabricated module layouts (e.g. `crates/jobs/CLAUDE.md` referenced `crawl/{processor,repo,watchdog,worker,runtime}.rs` instead of real `lite/workers/runners/{crawl,embed,extract,ingest}.rs`)
- Chrome ports (6000/9222/9223) were already bound to `127.0.0.1` in compose — exposure concern was unfounded
- All 16 SSRF call sites cited in agent C's audit verified
- `cdp_render.rs` only does `Runtime.evaluate` on injected HTML; never `Page.navigate` — no user URLs reach Chrome via that path
- openssl 0.10.78 transitively depended on by `native-tls` had CVE-2026-42327 / GHSA-xp3w-r5p5-63rr (UB in `X509Ref::ocsp_responders`)
- I (Claude) misinterpreted "untracked files at `plugins/`" as stale dupes 3 times and restored them to `plugins/axon/` — the user was actually moving them to the manifest-correct location

## Technical Decisions

- **Parallel agents over sequential**: 6+4+4 dispatched in 3 waves to fit the "epic quest" framing and minimize wall time. Disjoint file scopes prevented edit conflicts.
- **3 buckets in deletion pass**: delete (no value), move-to-plans/complete (planning done/abandoned), archive-to-pre-lite-mode (kept for history). Avoids permanently destroying context.
- **Heartbeat at 30s, watchdog tick at 60s, threshold 360s**: ~12x safety margin; hard-coded rather than env-configurable per user's "never add config not requested" rule
- **`HeartbeatGuard` RAII pattern**: matches existing tokio-based lifecycle in the workers; no separate cleanup path needed
- **Cancellation via `tokio::select!` at runner boundary**: coarse-grained but matches existing ingest pattern; deeper plumbing into engine layers flagged for future work
- **`SizeRotatingFile` custom writer**: `tracing-appender 0.2.5` only supports time-based rotation; size-based required custom impl wrapped in `non_blocking` (single worker thread, no internal locking)
- **1.5.5 reserved for security-only entry**: dependabot agent's CHANGELOG entry; 1.5.6 covers the larger feature wave

## Files Modified

### New files (3)
- `crates/jobs/lite/workers/heartbeat.rs` — `HeartbeatGuard` RAII type
- `crates/core/logging/size_rotating.rs` — `SizeRotatingFile` writer + 4 unit tests
- `docs/archive/pre-lite-mode/README.md` — explains what's in the archive
- `docs/reports/2026-05-06-stale-docs-audit/{00-MASTER,A-F-*}.md` — 7 audit reports

### Major rewrites
- `docs/SECURITY.md` — full rewrite for lite-mode auth model
- `docs/JOB-LIFECYCLE.md` — full rewrite for SQLite + in-process workers
- `docs/OPERATIONS.md` — full rewrite as SQLite-era runbook
- `docs/auth/API-TOKEN.md` — full rewrite (4 real tokens documented)
- `plugins/README.md` — full rewrite from placeholder
- `crates/jobs/README.md`, `crates/jobs/CLAUDE.md` — accurate heartbeat + lite-mode descriptions
- `docs/MCP-TOOL-SCHEMA.md` — regenerated via `scripts/generate_mcp_schema_doc.py`
- `docs/MCP.md` and 7 docs under `docs/mcp/` — purged refresh/graph/export references

### Code changes
- `crates/jobs/lite/workers.rs` — heartbeat spawning, periodic watchdog ticker, `cancel_store` registration in `worker_loop`
- `crates/jobs/lite/workers/runners/{crawl,embed,extract,ingest}.rs` — `Option<CancellationToken>` plumbing + `tokio::select!`
- `crates/jobs/lite/ops/lifecycle.rs` — `touch_heartbeat()` helper
- `crates/jobs/lite/ops/tests.rs` — `touch_heartbeat_advances_updated_at_only_on_running_rows`
- `crates/jobs/lite/workers/runners.rs` — `extract_runner_returns_canceled_when_token_pre_cancelled`
- `crates/core/logging.rs` — switched from daily/7-file to size-based rotation
- `crates/core/config/types/config.rs:267` — fixed `Default: 64` → `Default: 150` doc-comment
- `crates/mcp/assets/status_dashboard.html:213-220` — removed dead `local_refresh_jobs` / `local_graph_jobs` sections
- `Cargo.lock` — `openssl 0.10.78 → 0.10.79` (CVE-2026-42327)

### Deletions / archival
- 7 deleted: `docs/commands/{export,graph,refresh}.md`, `docs/{EXPORT,GRAPH,HEADLESS_OPTIONS}.md`, `docs/services/MEM0.md`
- 5 moved → `docs/plans/complete/`: CONFIG-DECOMPOSITION-PLAN, ERROR-HANDLING, LOBE-WORKFLOW-VISION, REBOOT-UI, modular
- 1 moved → `docs/reports/`: observability-gaps
- 9 archived → `docs/archive/pre-lite-mode/`: API, CLAUDE-HOT-RELOAD, MIGRATIONS, RESTORE, SCALING, SERVE, UI-DESIGN-SYSTEM, WEB-ARCHITECTURE, WS-PROTOCOL

### Plugin layout fix (commit b5efbc28)
- `git mv plugins/axon/{*,skills,agents} → plugins/` (20 files, history preserved) to match manifest paths

## Commands Executed

```bash
# 6 parallel audit agents (Agents A-F) — wave 1
# 4 parallel rewrite agents — wave 2
# 4 parallel follow-up agents — wave 3
git mv plugins/axon/{*,skills,agents} plugins/   # final layout fix
cargo update -p openssl                           # 0.10.78 → 0.10.79 (CVE patch)
cargo check --workspace                           # clean (22 crates)
cargo test --workspace --lib                      # 1475 passed, 5 ignored
python3 scripts/generate_mcp_schema_doc.py        # regenerated MCP-TOOL-SCHEMA.md
```

## Errors Encountered

- **Pre-commit hook failure**: 124-line `run_crawl_job_lite` exceeded 120-line monolith limit. Resolved by extracting `try_enqueue_embed_handoff()` helper.
- **Pre-commit hook failure**: clippy `useless_vec` in `size_rotating.rs:140`. Replaced `vec![b'x'; 80]` with `[b'x'; 80]`.
- **Plugin layout regression (3x)**: Each time the user moved files from `plugins/axon/` to `plugins/`, I diagnosed the resulting "untracked files at `plugins/`" as stale duplicates and ran `git checkout HEAD -- plugins/axon/` to "restore" them. Resolved by recognizing the user's intent was to match the manifest's already-correct `./plugins/skills` etc. paths, then using `git mv` properly.

## Behavior Changes (Before/After)

| Aspect | Before | After |
|--------|--------|-------|
| Plugin install | No MCP server registered when plugin enabled | `axon mcp` stdio server registered with 8 user-config-prompted env vars |
| Heartbeat | `updated_at` only advanced on progress events; long blocking phases left rows stale | 30s ticker bumps `updated_at` on every claimed job; periodic 60s watchdog reclaims stale rows |
| Cancellation | Only ingest-Reddit consumed `CancellationToken`; crawl/embed/extract ran to completion | All 4 runners respect `cancel_row` via `tokio::select!` |
| Logging | Daily rotation, 7 files retained | Size-based: 10 MiB max per file, 3 retained; configurable via `AXON_LOG_MAX_BYTES` / `AXON_LOG_MAX_FILES` |
| Documentation | ~17 docs described removed Postgres/AMQP/Redis era | Pre-lite-mode docs archived, lite-mode-only docs accurate |
| `openssl` | 0.10.78 (CVE-2026-42327, high) | 0.10.79 (patched) |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check --workspace` | clean | clean (22 crates) | ✅ |
| `cargo test --workspace --lib` | all pass | 1475 passed, 5 ignored | ✅ |
| `python3 scripts/generate_mcp_schema_doc.py` | regenerates MCP-TOOL-SCHEMA.md | wrote 6837 bytes | ✅ |
| Lefthook pre-commit (final commit) | all gates pass | taplo, monolith, rustfmt, mcp-http-only, env-guard, claude-symlinks, no-mod-rs, unwrap-warn, test, clippy all ✅ | ✅ |
| `git push` | accepted | `91c2246c..b5efbc28 main → main` | ✅ |

## Risks and Rollback

- **Heartbeat overhead**: 30s ticker per running job. With 4 lanes × bounded queue depth this is negligible, but high-fanout deployments may want a configurable interval. Rollback: revert commit `91c2246c`.
- **Cancellation drops in-flight reqwest IO**: `tokio::select!` at runner boundary drops the engine future but in-flight HTTP continues until reqwest timeout. Markdown already on disk persists; partial Qdrant upserts not rolled back. Matches existing ingest semantics.
- **Logging format change**: Old `axon.log.YYYY-MM-DD` daily files → new `axon.log.{1,2,3}` rotated. Operators with log-shipping that parses the date suffix will need to update parsers.
- **Archived docs are kept** at `docs/archive/pre-lite-mode/` rather than deleted — recoverable without git history archaeology.

## Decisions Not Taken

- Did not make heartbeat interval (30s) or watchdog tick (60s) env-configurable per user's "never add config not explicitly requested" rule.
- Did not deepen cancellation into `run_crawl_once` / `run_embed_pipeline` / `run_extract_with_engine` engine layers — coarse `select!` at runner boundary matches existing ingest pattern. Flagged for future work.
- Did not delete `docs/auth/API-TOKEN.md` after rewrite, even though it's smaller than before, since it still serves as a useful reference.
- Did not refactor `crates/services/setup/deploy.rs` to refuse non-loopback `chrome_remote_url` — could break legitimate single-host-with-tunnel setups; documented in SECURITY.md instead.

## References

- [Claude Code Plugins Reference](https://code.claude.com/docs/en/plugins-reference.md) — userConfig schema, `${user_config.*}` substitution
- [GHSA-xp3w-r5p5-63rr / CVE-2026-42327](https://github.com/advisories/GHSA-xp3w-r5p5-63rr) — openssl OCSP responder UB
- `docs/reports/2026-05-06-stale-docs-audit/00-MASTER-REPORT.md` — synthesized master report
- `docs/reports/2026-05-06-stale-docs-audit/{A,B,C,D,E,F}-*.md` — per-domain audit reports

## Open Questions

- The `--graph` global flag still exists in `global_args.rs` but all `graph` commands were deleted. Should it be removed too, or kept for a possible future Neo4j retrieval revival?
- `docs/auth/MCP-AUTH.md` (rewritten today) and `docs/auth/API-TOKEN.md` (rewritten today) overlap on the MCP HTTP token; consolidate or keep both as parallel references?
- `crates/services/setup/deploy.rs` accepts non-loopback `chrome_remote_url` without warning — should it refuse or warn at setup time?
- The user has 3 unrelated dirty files at session end (`.env.example`, `config/.gitignore`, `crates/mcp/server/artifacts/path.rs`) — they own those.

## Next Steps

**Started but not completed:**
- (none — all dispatched work landed)

**Follow-on tasks not yet started:**
- Investigate the `HeartbeatGuard` re-export visibility issue surfaced during parallel agent runs (may have been transient; final commit compiles clean)
- Address GitHub dependabot alert if dependabot UI hasn't auto-resolved alert #78 after the openssl bump push
- Decide whether to remove the dangling `--graph` global flag or keep it
- Consider adding env-configurable heartbeat / watchdog intervals if user load patterns warrant it
- The user's 3 unrelated dirty files (`.env.example`, `config/.gitignore`, `crates/mcp/server/artifacts/path.rs`) are restored from stash and ready for them to commit independently
