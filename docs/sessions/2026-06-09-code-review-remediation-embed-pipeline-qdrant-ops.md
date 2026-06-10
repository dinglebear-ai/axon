---
date: 2026-06-09 23:29:09 EST
repo: git@github.com:jmagar/axon.git
branch: feat/qdrant-affinity-tei-burst
head: 8236783f
working directory: /home/jmagar/workspace/axon/.worktrees/affinity
worktree: /home/jmagar/workspace/axon/.worktrees/affinity
pr: "#197 feat: embed-job fs-namespace claim affinity + doctor TEI concurrency drift warning — https://github.com/jmagar/axon/pull/197 (stacked on #196 — https://github.com/jmagar/axon/pull/196)"
beads: axon_rust-dmz8, axon_rust-o9y2, axon_rust-p2oc, axon_rust-qg8o
---

# Code-review remediation, embed-pipeline debugging, and Qdrant ops

## User Request

Review all code from merged PRs #185/#186/#188/#192, then "address all of those issues"; then build the release binary, sync the container, embed `~/docs`, and systematic-debug if it didn't embed everything; then implement all suggestions for the three follow-up issues (Qdrant pressure, job claim fs-affinity, TEI burst control) without raising the Qdrant memory cap; then give Qdrant more cores; then split the `router()` file flagged by the monolith gate.

## Session Overview

Ran a recall-biased multi-agent code review over four merged PRs (9 finder agents, 9 verifier agents; ~40 candidates verified down to 10 findings), fixed all 10, then live-verified `axon embed ~/docs` and systematic-debugged three more production bugs it exposed (container claiming host-path jobs and embedding the literal path string; a 31 MB JSON grinding the chunker; payload-index assertion failing embeds against a slow Qdrant). Final verification: 104/104 expected docs, 7,144 chunks, 0 failures. Then implemented the three follow-up beads: applied Qdrant lever B (`always_ram: false` + `hnsw on_disk: true`, **15 GiB → 6.2 GiB**), built embed-job filesystem-namespace claim affinity (migration 0008), added a doctor TEI-concurrency drift warning, shielded Qdrant from the host's global OOM killer, raised its CPU budget (2 optimizer threads / 4 CPUs → 4 / 8), and ran dedupe (0 duplicates). Two PRs open: #196 (5.6.1) and #197 stacked on it. A second concurrent Claude session worked in the same checkout throughout, which clobbered uncommitted edits once and required moving to the `.worktrees/affinity` worktree.

## Sequence of Events

1. **Sync check**: confirmed `main` clean/synced; listed 11 PRs merged in the prior 2 days.
2. **Code review (`/code-review high`)**: diffed the four merge commits (`d622382c`, `ceabe1dc`, `4b71f2fa`, `2fc65940`); 9 finder agents across line-scan/removed-behavior/cross-file/cleanup/altitude angles; 9 verifiers (several inverted their verdict labels — ranked by quoted evidence instead); produced 10 findings.
3. **Remediation (bead axon_rust-dmz8, branch `fix/code-review-findings-185-192`)**: fixed all 10 findings, bumped 5.6.0 → 5.6.1, discovered and fixed a pre-existing red gate (`just validate-plugin` forbids a `version` key in `plugins/axon/.claude-plugin/plugin.json`; PR #191 had re-added it), `just verify` green (2,816 tests), opened PR #196.
4. **Live verification**: `just sync-container`, then `axon embed ~/docs` — found only 1 doc/1 chunk embedded in the user's earlier attempts. Invoked systematic-debugging.
5. **Root causes found and fixed (commits `feda45f6`, `2b90bc37`)**: (a) container worker claimed the host-path job and the free-text fallback embedded the literal string `/home/jmagar/docs` (confirmed as a Qdrant point; deleted); (b) no size cap let a 31 MB `arrs/index.json` grind the chunker (90 s+ release CPU-bound, 23+ min in debug); (c) `ensure_payload_indexes` fired ~46 concurrent PUTs per embed and treated one timeout as fatal — fatal against a memory-strained Qdrant whose index PUTs took 30–60 s.
6. **Environmental fixes**: recreated `axon-tei` with `--env-file ~/.axon/.env` (it had silently been running `--max-concurrent-requests 32` instead of the env's 256, causing 429 "Model is overloaded" with an idle GPU).
7. **Final embed verification**: 104/104 expected docs (105 readable files minus the capped 31 MB JSON; symlinks and `__pycache__` correctly excluded), 7,144 chunks, 0 failures; content retrievable via `axon query`.
8. **Follow-up beads filed** (o9y2, p2oc, qg8o), explained to the user, then implemented on request.
9. **Qdrant lever B**: live `PATCH /collections/axon` (`always_ram: false`, `hnsw on_disk: true`); collection inventory showed all 36 non-`axon` collections total ~15 k points (0.3%) so the junk-deletion lever was rejected as a no-op.
10. **Session collision**: a second active session in the same checkout reverted this session's uncommitted affinity/doctor edits; moved to `.worktrees/affinity` (per repo worktree policy) and re-applied everything there as branch `feat/qdrant-affinity-tei-burst` (5.7.0), PR #197.
11. **Two host-level OOM kills of qdrant** during re-optimization (dmesg `CONSTRAINT_NONE` global OOM — host at 46/48 GiB with concurrent cargo builds; not the container's 16 G cap). Applied `oom_score_adj -500` live to the qdrant pid and durably in compose.
12. **"Give qdrant more cores"**: found the actual bottleneck was the collection's `max_optimization_threads: 2` (production.yaml default); PATCHed it to 4 live and raised the container CPU limit 4 → 8 via `docker update` + compose.
13. **"Split the file"**: split `router()` (121 lines > 120 limit) in `src/web/server/routing.rs` into `read_routes()`/`write_routes()`/`large_write_routes()` over a `ServeState` alias; also fixed `panel_routes<S>`'s broken generic signature (introduced at branch HEAD by the concurrent session) which had been failing every pre-push hook.
14. **Dedupe**: green-watch + auto-dedupe chain fired after the optimizer finished; first attempt failed on jobs-DB migration skew (DB already at migration 0008, old 5.6.1 binary refused); rebuilt and re-ran: **0 duplicate groups, 0 points deleted**. Qdrant settled at **6.18 GiB / 16 GiB**, status green.
15. Closed all four beads, `bd dolt push`, pushed all branches/PRs.

## Key Findings

- **Container claims host-path embed jobs**: the shared `~/.axon/jobs.db` is polled by workers in two filesystem namespaces; `read_inputs`'s free-text fallback (`src/vector/ops/tei/prepare.rs`) turned an unreadable path into a one-chunk "document" containing the path string. Confirmed garbage point in Qdrant with `chunk_text == "/home/jmagar/docs"`.
- **`just verify` had been red on main since PR #191** — `validate-plugin` (rule added in `b62c66e1`) forbids a `version` key in the plugin manifest; #191 re-added it.
- **TEI env drift**: `docker inspect axon-tei` showed `--max-concurrent-requests 32` while `~/.axon/.env` sets 256 — the container had been started without `--env-file`. TEI returned "no permits available" with a 0% GPU.
- **Qdrant memory math**: `axon` collection = 4,500,764 points; all 36 other collections ≈ 15 k points (0.3%) — collection deletion is not a memory lever; quantization/HNSW placement is (15 GiB → 6.2 GiB).
- **Host-level OOM, not cgroup**: both qdrant kills were `CONSTRAINT_NONE` global OOM (dmesg) on a 48 GiB host at ≥46 GiB used; qdrant was simply the largest RSS.
- **Optimizer bottleneck was thread cap, not CPU limit**: `config/qdrant/production.yaml:17` `max_optimization_threads: 2` (carried into the collection config) capped the rebuild regardless of container CPUs.
- **TEI in-flight semaphore already existed** (`AXON_TEI_MAX_CONCURRENT`, default 8, `src/vector/ops/tei/tei_client.rs:96`) — half of bead qg8o was already implemented; only the doctor drift check was missing.
- **Concurrent-session hazard**: two sessions editing one checkout silently clobber each other's uncommitted tracked files; untracked files survive. Worktrees + pathspec-limited commits (`git commit -- <path>`) were the workable coexistence pattern.

## Technical Decisions

- **Recall-biased review with evidence-over-labels**: several verifier agents emitted verdict labels contradicting their own quoted evidence; final ranking used the evidence.
- **Claim-side serialization for same-target ingests** (`src/jobs/ops/lifecycle.rs`) instead of enqueue-dedup: the race is about concurrent *execution*, and queuing behind a running sibling is legitimate.
- **CLI local-path embeds run in-process even without `--wait`**: a fire-and-forget CLI never services its own queue, so enqueuing a host path was never serviceable by anything that could read it.
- **`refresh` replays the original job's stored config snapshot** (max-depth/scoping/headers) and pins collection/endpoints to the current process — re-running with process defaults silently rescoped crawls.
- **Affinity via nullable `fs_namespace` column** (migration 0008): NULL = claim anywhere (URLs, free text, legacy rows); stamped only for path-like inputs; compose pins `AXON_FS_NAMESPACE: axon-container` because container hostnames change per recreate.
- **`ensure_payload_indexes` reads `payload_schema` from the GET `ensure_collection` already performs** — zero index PUTs on a warm collection; index failures warn instead of failing embeds (indexes are query-time optimizations, retried next embed).
- **`oom_score_adj -500` for qdrant** rather than raising the memory cap (user constraint): under host pressure the kernel should kill rebuildable builds, not the database.
- **Dedupe sequenced behind optimizer green** (auto-chained in a background watcher): interleaving a 4.5 M-point scroll+delete with an in-flight full re-optimization wastes rebuild work and re-creates the load profile that caused the OOM kills.
- **10 MB local-embed file cap matches the server validator default** (`mcp_embed_max_local_bytes`); skip+warn on walks, hard error when a file is explicitly named.

## Files Changed

All commits by this session (the concurrent session's commits — e.g. `79b2594c`, and the `fix/` branch's later 5.7.x commits — are excluded).

Branch `fix/code-review-findings-185-192` (PR #196):

| status | path | purpose | evidence |
|---|---|---|---|
| modified | src/services/embed.rs | prune-before-symlink validator order; delegate `looks_path_like_input` | dbfe755c, feda45f6 |
| modified | src/services/embed_tests.rs | pruned-dir symlink regression test | dbfe755c |
| modified | src/jobs/ops/lifecycle.rs | per-(source_type,target) ingest claim serialization | dbfe755c |
| modified | src/jobs/ops_tests.rs | claim-serialization test | dbfe755c |
| modified | src/jobs/ingest.rs, src/jobs/ingest/types.rs | `RE_INGESTABLE_SOURCE_TYPES` single-sourced | dbfe755c |
| modified | src/jobs/query.rs | `latest_crawl_config_json` / `latest_ingest_config_json` | dbfe755c |
| modified | src/services/refresh.rs, src/services/refresh_tests.rs | config-snapshot replay per origin; tests | dbfe755c |
| modified | src/cli/commands/refresh.rs | nonzero exit on partial enqueue failure | dbfe755c |
| modified | src/vector/ops/input.rs | `chunk_text_with_offsets` (true byte offsets) | dbfe755c |
| modified | src/ingest/github/files/prepare.rs, prepare_tests.rs, src/ingest/github/files_tests.rs | offset-based prose-fallback chunking; removed `next_search_start` | dbfe755c |
| modified | src/vector/ops/qdrant.rs, src/vector/ops/qdrant/utils.rs | `rank_points_by_query_overlap`, `render_points_in_doc_order` | dbfe755c |
| modified | src/vector/ops/commands/ask/context/build/appenders.rs | single-render full-doc budget fitting | dbfe755c |
| modified | plugins/axon/.claude-plugin/plugin.json | removed forbidden `version` key | dbfe755c |
| modified | CLAUDE.md | gotchas: stale-cleanup v7 visibility, symlink policy, version-bump file list | dbfe755c, feda45f6 |
| modified | src/vector/ops/tei/prepare.rs, prepare_tests.rs | path-like hard error, 10 MB cap, empty-doc logging, root-symlink test | dbfe755c, feda45f6 |
| modified | src/cli/commands/embed.rs | local paths embed in-process without `--wait` | feda45f6 |
| modified | src/vector/ops/input/select.rs | shared `looks_path_like` | feda45f6 |
| modified | src/vector/ops/tei/qdrant_store.rs, qdrant_store/payload_indexes.rs, payload_indexes_tests.rs | missing-only index asserts; non-fatal failures | 2b90bc37 |
| modified | Cargo.toml, Cargo.lock, README.md, CHANGELOG.md, apps/web/package.json, apps/web/openapi/axon.json | 5.6.0 → 5.6.1 + changelog | dbfe755c, feda45f6, 2b90bc37 |
| modified | plugins/axon/bin/axon | LFS dev-binary sync | 3afebbd0 |

Branch `feat/qdrant-affinity-tei-burst` (PR #197):

| status | path | purpose | evidence |
|---|---|---|---|
| created | src/jobs/migrations/0008_add_embed_fs_namespace.sql | `fs_namespace` column on embed jobs | 1d149d30 |
| modified | src/jobs/ops.rs, ops/enqueue.rs, ops/lifecycle.rs, ops_tests.rs | `fs_namespace()` helper; enqueue stamping; claim filter; tests | 1d149d30 |
| created | src/core/health/doctor/sqlite_tests.rs | TEI drift-warning tests | 1d149d30 |
| modified | src/core/health/doctor/sqlite.rs, src/cli/commands/doctor/render.rs | `tei_concurrency_warning` in report + human output | 1d149d30 |
| modified | docker-compose.prod.yaml | `AXON_FS_NAMESPACE: axon-container`; qdrant `oom_score_adj: -500`; qdrant `cpus: 4 → 8` | 1d149d30, d2398b18, 2e25fb68 |
| modified | docs/reference/env-matrix.toml | `AXON_FS_NAMESPACE` entry | 1d149d30 |
| modified | src/web/server/routing.rs | `router()` split into scoped builders; `panel_routes` pinned to `ServeState` | 8236783f |
| modified | Cargo.toml, Cargo.lock, README.md, CHANGELOG.md, apps/web/package.json, apps/web/openapi/axon.json | 5.6.1 → 5.7.0 + changelog | 1d149d30 |
| modified | plugins/axon/bin/axon | LFS dev-binary sync | 39bb1069 |

## Beads Activity

| bead | title | actions | final status | why it mattered |
|---|---|---|---|---|
| axon_rust-dmz8 | Fix code-review findings from PRs #185/#186/#188/#192 | created, claimed, closed | closed | tracked the 10-finding remediation + 3 live-debugging fixes (PR #196) |
| axon_rust-o9y2 | Qdrant memory pressure: 4.49M points, 15/16 GiB | created, comment-add attempted (output not captured — uncertain whether it landed), closed | closed | lever B applied (15 → 6.2 GiB), dedupe run (0 dupes), deletion lever rejected with inventory evidence, cap untouched per user |
| axon_rust-p2oc | Embed/ingest job claim affinity: host vs container fs namespaces | created, closed | closed | implemented (migration 0008 + claim filter, PR #197) |
| axon_rust-qg8o | TEI burst control: process-wide in-flight cap | created, closed | closed | semaphore already existed (`AXON_TEI_MAX_CONCURRENT`); doctor drift warning implemented (PR #197) |

`bd dolt push` run twice during the session ("Push complete.").

## Repository Maintenance

- **Plans**: no plans created this session; no plan moved to `docs/plans/complete/`. The committed plan `docs/superpowers/plans/2026-06-08-unify-code-file-ingestion-engine.md` is actively being implemented by the concurrent session (worktree `feat/axon_rust-8mu8`, "shared file-ingest engine" commit) — not safe to classify. The injected "Active plan" (`axon_rust/docs/plans/2026-05-27-android-phase2-stubbed-modes.md`) points at the deprecated `~/workspace/axon_rust` copy and was not part of this session. No-op, with reason.
- **Beads**: all four session beads created and closed (see table). No other beads touched.
- **Worktrees/branches** (evidence: injected `git worktree list` + branch listing): three worktrees — main checkout on `fix/code-review-findings-185-192` (concurrent session, **ahead 18 unpushed**, dirty), `.worktrees/affinity` (this session, pushed, other session's staged files present), `.worktrees/feat/axon_rust-8mu8` (concurrent session, 5.8.0 work). **Nothing deleted** — every branch/worktree is active or owned by the live concurrent session.
- **Stale docs**: CLAUDE.md gotchas and the version-bump section were corrected as part of the work itself (committed); `docs/reference/env-matrix.toml` updated for the new env var. No further stale-doc sweep attempted — the concurrent session was actively rewriting multiple CLAUDE.md files mid-session, making a broader pass unsafe.
- **Cleanup of test artifacts**: deleted the `debug_probe` Qdrant collection and the garbage `/home/jmagar/docs` point (curl delete, `status: ok`); removed nothing else.

## Tools and Skills Used

- **Skills**: `code-review` (review harness), `superpowers:systematic-debugging` (embed failure), `vibin:save-to-md` (this artifact).
- **Subagents**: 9 Explore finder agents + 9 Explore verifier agents for the review. Issue: Explore agents have a turn cap and three ended on narration instead of emitting JSON (re-run with explicit tool-call budgets); several verifiers emitted verdict labels contradicting their quoted evidence.
- **Shell/file tools**: git/gh/cargo/just/bd/docker/curl/sqlite3 throughout; Read/Edit/Write for all code changes.
- **Background tasks + Monitor**: qdrant optimizer green-watches, a green→dedupe auto-chain, long builds/pushes. Issue: piping through `tail` hid intermediate output and masked one exit code; one Monitor timed out (1 h cap) and was re-armed.
- **MCP/plugins**: lumen semantic search (one attempt — index only covered docs, not source; fell back to rg). PreToolUse hooks repeatedly suggested lumen; rg remained more reliable for this work.
- **No browser tools** used.

## Commands Executed

| command | result |
|---|---|
| `git diff <merge>^ <merge>` ×4 | review scope: +18 k/−2.4 k lines across 4 PRs |
| `just verify` (multiple) | green at 2,816 / 2,822 / 2,535 tests across stages |
| `just sync-container` ×2 | container rebuilt + recreated on dev binary |
| `./scripts/axon embed ~/docs` | finally: `embedded 7144 chunks from 104 docs into axon` |
| `curl PATCH /collections/axon` (quant/hnsw; optimizer threads) | `"result": true`; status yellow → green |
| `docker update --cpus 8 axon-qdrant`; `echo -500 > /proc/<pid>/oom_score_adj` | live CPU bump + OOM shield |
| `docker compose --env-file ~/.axon/.env -f docker-compose.prod.yaml up -d axon-tei` | TEI recreated with 256 permits |
| `./target/debug/axon dedupe --yes --collection axon` | `deduplicated 0 groups, deleted 0 points` |
| `gh pr create` ×2 | PR #196, PR #197 |

## Errors Encountered

- **Embed of `~/docs` produced 1 doc/1 chunk** — container claimed the host-path job; fixed at the reader + CLI + (later) claim layer.
- **TEI 429 "Model is overloaded" with idle GPU** — container running compose-default 32 permits instead of env's 256; recreated with `--env-file`.
- **Every embed failed at "collection init/cache"** — ~46 concurrent index PUTs against a thrashing Qdrant (30–60 s per PUT); fixed by missing-only asserts + non-fatal failures.
- **Qdrant OOM-killed twice** during re-optimization — host-level global OOM (`CONSTRAINT_NONE`), not the cgroup cap; mitigated with `oom_score_adj -500` (live + compose).
- **Concurrent session clobbered uncommitted edits** in the main checkout; recovered by re-applying in `.worktrees/affinity`. It later also committed to this worktree's branch; coexisted via pathspec-limited commits.
- **`git push` blocked by pre-push hook** — branch HEAD didn't compile (`panel_routes<S>` E0308 from the concurrent session's commit); fixed in the routing split commit.
- **Dedupe attempt #1 failed on migration skew** — jobs DB already at migration 0008; the 5.6.1 release binary refused (`migration 8 was previously applied but is missing`); re-ran with a current build. (The chain printed `EXIT CODE: 0` because the pipe swallowed the real status.)
- **`zsh` ate `?wait=true`** in an unquoted curl URL — the garbage-point delete silently didn't run the first time; re-run quoted.

## Behavior Changes (Before/After)

| area | before | after |
|---|---|---|
| `axon embed <host-dir>` (no --wait) | enqueued; container claimed it and embedded the literal path string as 1 chunk | runs in-process; path-like inputs that don't resolve are hard errors; oversized files (>10 MB) skipped with warning |
| Server-side embed of JS projects | rejected on `node_modules/.bin` symlinks | pruned dirs exempt from symlink check |
| Same-repo concurrent ingests | could race; stale cleanup could delete fresh points | serialized per (source_type, target) at claim |
| `axon refresh` | re-enqueued with process defaults; exit 0 on partial failure | replays original job config; nonzero exit on failures |
| Embed vs slow Qdrant | ~46 index PUTs per embed; one timeout failed the embed | zero PUTs on warm collections; failures warn and retry next embed |
| Qdrant memory | 15 GiB / 16 GiB, requests 30–60 s | 6.2 GiB, green, idle |
| Qdrant under host OOM | kernel's first victim (killed 2×) | `oom_score_adj -500` (live + compose) |
| Qdrant optimizer | 2 threads / 4 CPUs | 4 threads / 8 CPUs (live PATCH + `docker update` + compose) |
| TEI | 32 concurrent-request permits (env drift) | 256 per env; `axon doctor` now warns on this drift |
| `router()` | 121-line monolith violation; branch didn't compile | split into scoped builders; gates green |

## Verification Evidence

| command | expected | actual | status |
|---|---|---|---|
| `just verify` (fix branch, final) | all gates green | 2,816 tests passed, 6 skipped | pass |
| `just verify` (worktree, 5.7.0) | all gates green | 2,822 tests passed, 6 skipped | pass |
| `./scripts/axon embed ~/docs` | 104 docs (105 readable − 1 capped) | `7144 chunks from 104 docs`, 0 failures | pass |
| `axon query "homelab proxies swag configuration"` | ~/docs content retrievable | `/home/jmagar/docs/homelab/proxies.md` rank 2 | pass |
| `curl /collections/axon` post-optimization | status green | `status: green`, 6.18 GiB | pass |
| `axon dedupe --yes` | completes | `0 groups, 0 points deleted` | pass |
| `docker inspect axon-tei` args | `--max-concurrent-requests 256` | 256 | pass |
| `git push` (final, worktree) | pre-push green + pushed | clippy + 2,535 tests green; `79b2594c..8236783f` | pass |

## Risks and Rollback

- **Two open stacked PRs sharing commits with a live concurrent session**: PR #197's branch now interleaves both sessions' commits (it bumped versions to 5.7.x independently on the fix branch). Merge #196 first, then #197, after the concurrent session winds down; expect CHANGELOG/version merge conflicts.
- **Collection-level optimizer threads now 4** (persisted in collection config; production.yaml default stays 2 for new collections): future optimizations use more transient CPU/RAM — acceptable with the on-disk layout + OOM shield; revert with `PATCH {"optimizers_config":{"max_optimization_threads":2}}`.
- **Quantization/HNSW on disk trades query latency for RAM**; revert with the inverse PATCH (no re-embed needed).
- **Migration 0008 is applied to the shared jobs DB** — binaries older than this branch will refuse to open it (observed). Roll forward, not back.
- All compose changes revert by `git revert` + `docker compose up -d`.

## Decisions Not Taken

- **Deleting session collections in Qdrant** — inventory proved them irrelevant (≈0.3% of points) and they are real ingested data.
- **Raising the Qdrant memory cap** — explicitly excluded by the user; lever B sufficed.
- **Enqueue-time dedup for same-target ingests** — claim-time serialization solves the actual race without rejecting legitimate queued re-ingests.
- **Adding a monolith allowlist entry for `router()`** — the gate's own message and the user both said split.
- **Stashing/reverting the concurrent session's files** — never safe; coexisted via worktree + pathspec commits.

## Open Questions

- Whether the `bd comment` on axon_rust-o9y2 actually landed (output was truncated both attempts); the bead is closed either way and PR #197 documents the same content.
- Exact cause of TEI permit exhaustion at ≤16 client in-flight vs 32 server permits during the first failed embed (likely cross-process bursts + retry collisions); moot at 256 permits, and `axon doctor` now surfaces the config drift.
- The `fix/` branch is **ahead 18 unpushed** in the main checkout under the concurrent session — PR #196's remote state lags whatever it is doing.

## Next Steps

1. **Let the concurrent session finish**, then merge **PR #196** → **PR #197** (in that order; #197 is stacked). Reconcile version numbers (the branches independently reached 5.7.x).
2. After merge, **recreate the prod stack** so compose changes (OOM shield, qdrant CPUs, `AXON_FS_NAMESPACE`) apply to fresh containers: `docker compose --env-file ~/.axon/.env -f docker-compose.prod.yaml up -d`.
3. Run `axon doctor` after any TEI/container recreate — it now catches the 32-vs-256 permit drift.
4. Optional: consider whether the collection's `max_optimization_threads: 4` should be reverted to 2 once steady-state behavior is observed.
5. Host memory over-commit on dookie (48 GiB, routinely ≥46 used with stacked builds + sessions) remains the systemic risk behind the OOM kills — worth a separate look (fewer parallel heavy builds, or more RAM for the VM).
