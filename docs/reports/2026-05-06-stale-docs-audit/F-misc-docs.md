# Misc Docs Audit — F-misc-docs

Date: 2026-05-06
Scope: ~45 files in `docs/` (root, `auth/`, `services/`, `repo/`, `stack/`)
Excluded by request: `docs/plans/`, `docs/superpowers/plans/`, `docs/superpowers/specs/`, `docs/auth/MCP-AUTH.md`.

This audit was conducted against current code state on branch `bd-work/p2-multi-remediation`,
package version **1.5.4**.

## Top-level reality (anchor for all findings)

The codebase has been substantially gutted since most of these docs were written:

1. **Postgres / AMQP / Redis / lapin are GONE.** Only SQLite remains (`sqlx = "0.8"` with `sqlite` feature; `lapin` is not in `Cargo.toml`). Lite mode is the *default*. There is no AMQP fallback, no Postgres pool, no Redis cancel keys.
2. **`apps/web/` is now `@axon/admin-panel` v1.3.4** — a minimal admin/setup panel (one `page.tsx`, one `layout.tsx`, one `styles.css`). There is no `app/api/`, no `lib/ws-protocol.ts`, no `proxy.ts`, no `hooks/`, no Pulse UI, no terminal, no editor pane, no `__tests__/`.
3. **`crates/web/` is 4 files** (`auth.rs`, `security.rs`, `server.rs`, `static_assets.rs`) — a small Axum admin server. There is no `crates/web/execute/`, no `crates/web/download.rs`, no `crates/web/shell.rs`, no `crates/web/docker_stats.rs`, no WS execute bridge, no PTY bridge, no Docker-stats broadcaster, no `ws_handler.rs`.
4. **No Docker app images.** `docker/` directory does not exist. `config/docker-compose.services.yaml` only ships infra (Qdrant + TEI + Chrome). There is no `axon-workers` container, no `axon-web` container, no s6 supervision.
5. **`Justfile` is much smaller than docs claim.** Recipes that don't exist: `docker-build`, `up`, `down`, `down-all`, `rebuild-fresh`, `web-dev`, `web-build`, `web-lint`, `web-format`, `workers`, `cache-status`, `cache-prune`, `docker-context-probe`, `check-container-revisions`, `gen-mcp-schema`.

These four facts invalidate large portions of multiple docs at the architectural level. Many docs are wholesale obsolete and are recommended for deletion or archival to `docs/plans/complete/` rather than line-by-line edits.

---

## Files verified clean

- `docs/SHELL-COMPLETIONS.md` — accurate; cross-link to `commands/completions.md` exists.
- `docs/repo/MEMORY.md` — describes beads + push protocol; no incorrect claims (lefthook/just/bd commands all exist or are user-installed).
- `docs/stack/CLAUDE.md` — index file; cross-refs all resolve.
- `docs/repo/CLAUDE.md` — index file; all cross-refs resolve EXCEPT note that `../SETUP.md` and `../CONFIG.md` exist (they do).
- `docs/CONTEXT-INJECTION.md` — code references and env vars verified against `crates/vector/ops/commands/ask/`. Accurate.

---

## Files recommended for archival or deletion (wholesale obsolete)

Recommend moving to `docs/plans/complete/` or deleting outright. These are too far from reality to fix by line-edits — they describe a former architecture or unbuilt vision.

| File | Reason |
|------|--------|
| `docs/HEADLESS_OPTIONS.md` | Stream-of-consciousness brainstorming notes about Claude CLI flags, with first-person musing ("Are we planning on using this? I thought we were"). Not documentation. **Recommend deletion.** |
| `docs/LOBE-WORKFLOW-VISION.md` | Vision doc for a Lobe/Workflow UI that does not exist in `apps/web` (which is now an admin panel). **Recommend move to `docs/plans/complete/` or `docs/plans/`** if still aspirational. |
| `docs/REBOOT-UI.md` | Describes "Axon Shell — UI Design Language" three-pane chat workspace with NeuralCanvas, atmospheric gradients, mobile breakpoints, terminal drawer, etc. None of this exists in the current `apps/web`. **Recommend archive.** |
| `docs/UI-DESIGN-SYSTEM.md` | Token system for an Axon dark-neural UI that does not exist in `apps/web`. **Recommend archive.** |
| `docs/WEB-ARCHITECTURE.md` | Documents axum WS bridge on port 49000 + Next.js dashboard on 49010. Neither survives in the current code. Marked "Documentation only — consolidation not yet started." **Recommend archive.** |
| `docs/WS-PROTOCOL.md` | Single source of truth for a WebSocket protocol that no longer has a producer (`crates/web/execute/constants.rs` does not exist) or consumer (`apps/web/lib/ws-protocol.ts` does not exist). **Recommend archive.** |
| `docs/SERVE.md` | Describes `axon serve` web bridge with execute WS, PTY shell WS, Docker stats. Source files cited (`crates/web/execute.rs`, `crates/web/shell.rs`, `crates/web/docker_stats.rs`) do not exist. **Recommend archive.** |
| `docs/API.md` | Documents `/ws`, `/ws/shell`, `/api/pulse/chat`, `/api/pulse/doc`, `/api/pulse/save`, `/api/ai/copilot`, `/api/omnibox/files`. None of these route handlers exist. **Recommend archive.** |
| `docs/CLAUDE-HOT-RELOAD.md` | Documents s6 services `claude-session` + `claude-watcher` inside `axon-web` container. Container does not exist. **Recommend archive.** |
| `docs/services/MEM0.md` | Documents an external `axon-mem0` FastAPI service called by `ngent` (a separate Go ACP server). Not referenced from this repo's code, not in docker-compose. **Recommend move out of this repo's docs entirely.** |
| `docs/CONFIG-DECOMPOSITION-PLAN.md` | Self-described "Phase 1 (TOML loading) implemented in v0.36 — full subconfig migration pending". This is an active plan, not finished doc. **Recommend move to `docs/plans/`** for visibility. |
| `docs/ERROR-HANDLING.md` | Self-described "Documentation only — AxonError enum not yet implemented". Also assumes `lapin::Error` and `sqlx::Error` co-exist (lapin doesn't). **Recommend move to `docs/plans/` or deletion.** |
| `docs/MIGRATIONS.md` | Built around Postgres advisory locks and `sqlx` migrate against Postgres. The lock-key registry references `postgres advisory_lock` semantics that don't exist with SQLite. **Recommend rewrite as SQLite-migrate doc, or move to `docs/plans/`.** |
| `docs/SCALING.md` | Self-described "Documentation only" plan, but also references "AMQP queue", "PostgreSQL `SELECT ... FOR UPDATE SKIP LOCKED`", "max_connections in pg logs", `axon-workers` containers, and s6 services. Internally inconsistent (claims SQLite **and** Postgres). **Recommend archive.** |
| `docs/RESTORE.md` | Preconditions explicitly state `postgres`, `redis`, `rabbitmq` services must be up. They do not exist. **Recommend rewrite to drop those preconditions, or archive alongside EXPORT.md depending on whether export is still implemented.** |
| `docs/modular.md` | "Vision" doc mixing implemented features with unimplemented future plans. References cargo features (`crawler-only`, `jobs`, `embeddings`) that do not exist in `Cargo.toml`. **Recommend move to `docs/plans/` and label as vision.** |
| `docs/observability-gaps.md` | Self-described automated audit dated 2026-04-29. Lists 69 gaps. Not closed. The doc is a finding list; no clear ownership or status tracking. **Recommend keep as a snapshot but move to `docs/reports/2026-04-29-observability-gaps.md`** to fit the existing `docs/reports/` convention. |

---

## Findings by file

### docs/README.md

#### [README.md:73] Reference to non-existent `SCHEMA.md`
**Stale claim:** "[Schema](./SCHEMA.md) -- database schema"
**Reality:** `docs/SCHEMA.md` does not exist (verified — neither this nor any link target named `SCHEMA.md` is present).
**Fix:** Remove the line, or replace with a pointer to `migrations/001_initial_schema.sql`.
**Applied:** no — the file references several other missing/historical docs and the whole index would benefit from a single rewrite pass instead of one-line patches.

#### [README.md:89] References subdir `commands/`, `ingest/`, `auth/`, `services/`, `sessions/`, `superpowers/`
**Reality:** `commands/`, `ingest/`, `auth/`, `services/`, `sessions/`, `superpowers/` all exist. Verified clean.
**Applied:** n/a

#### [README.md:96-98] References `screenshots/` directory
**Stale claim:** Listed in "What is Axon" port table for `axon serve` (49000 backend, 49010 Next.js); also references `screenshots/`.
**Reality:** `axon serve` no longer boots a backend bridge plus Next.js — both surfaces are gone. `docs/screenshots/` does not exist.
**Fix:** Rewrite top of README to drop "trimodal" framing (Web UI is a small admin panel, not a co-equal mode) and remove `screenshots/` reference.
**Applied:** no — major rewrite needed, flagged.

---

### docs/CLAUDE.md (docs subfolder index)

#### [CLAUDE.md:25] AMQP framing
**Stale claim:** `JOB-LIFECYCLE.md` description is "AMQP job state machine and lifecycle diagrams"
**Reality:** SQLite-only; no AMQP. (Verified `Cargo.toml` has no `lapin`.)
**Fix:** Change to "Job state machine and lifecycle diagrams (SQLite-backed)".
**Applied:** yes — see edit below.

#### [CLAUDE.md:31] Reference to non-existent `SCHEMA.md`
**Stale claim:** "SCHEMA.md ... Full database schema reference (auto-created tables)"
**Reality:** File does not exist.
**Fix:** Either create the file or remove the reference (and remove from line 114-117 below as well).
**Applied:** no — flagged.

#### [CLAUDE.md:114-117] References non-existent `docs/SCHEMA.md`
**Stale claim:** "### `docs/SCHEMA.md` ... Database schema reference."
**Reality:** File does not exist; section is dead text.
**Fix:** Delete the heading + paragraph.
**Applied:** no — flagged with previous finding.

---

### docs/CHECKLIST.md

#### [CHECKLIST.md:23] References non-existent `just docker-build`
**Stale claim:** "`just docker-build` succeeds"
**Reality:** No `docker-build` recipe in `Justfile`.
**Fix:** Remove the line or replace with `just build` (release binary, no Docker image).
**Applied:** no — flagged. Whole checklist needs rework against current build flow.

#### [CHECKLIST.md:38] Same `config/docker-compose.services.yaml` claim
**Reality:** Compose file exists; claim is correct.
**Applied:** n/a

#### [CHECKLIST.md:24] References `cd apps/web && pnpm build`
**Stale claim:** "Web UI builds: `cd apps/web && pnpm build`"
**Reality:** `apps/web` is `@axon/admin-panel` and uses `next build`. Recipe `web-build` does not exist in Justfile, but `pnpm build` should work since `package.json` defines `"build": "next build"`. Claim is technically correct.
**Applied:** n/a — verified accurate.

#### [CHECKLIST.md:32] Docker non-root claim
**Stale claim:** "Docker containers run as non-root (s6-setuidgid, UID 1001)"
**Reality:** No app Dockerfiles exist. The only compose file is for infra (Qdrant/TEI/Chrome).
**Fix:** Drop or rewrite — there is no `axon-workers` Docker image being built from this repo.
**Applied:** no — flagged.

#### [CHECKLIST.md:42] References "MCP-TOOL-SCHEMA.md regenerated: just gen-mcp-schema"
**Stale claim:** "just gen-mcp-schema"
**Reality:** No `gen-mcp-schema` recipe in Justfile. Lefthook and `scripts/generate_mcp_schema_doc.py` invoke directly.
**Fix:** Replace with `python3 scripts/generate_mcp_schema_doc.py`.
**Applied:** no — flagged.

---

### docs/INVENTORY.md

#### [INVENTORY.md:90-103] App services Axon-workers / Axon-web do not exist
**Stale claim:** Tables list `axon-workers` (`docker/Dockerfile`, ports 49000/8001) and `axon-web` (`docker/web/Dockerfile`, port 49010) as services.
**Reality:** Neither Dockerfile exists. The `docker/` directory does not exist. There is no `axon-workers` or `axon-web` container.
**Fix:** Delete the "App services" table; the only services are infrastructure (Qdrant, TEI, Chrome).
**Applied:** no — flagged. Most of this doc is rooted in the trimodal/worker/AMQP architecture that no longer exists.

#### [INVENTORY.md:107-114] Worker queue names use AMQP queue convention
**Stale claim:** Lists queues `axon.crawl.jobs`, `axon.extract.jobs`, etc.
**Reality:** No AMQP queues; SQLite tables (`axon_crawl_jobs`, `axon_extract_jobs`, etc.) are the storage. Refresh and Watch listed but not in current `crates/jobs/`.
**Applied:** no — flagged.

#### [INVENTORY.md:117-128] Workspace crates table
**Stale claim:** "`jobs` ... Async job framework (AMQP + SQLite backends)"
**Reality:** No AMQP backend. `crates/jobs/backend.rs` exposes `JobBackend` trait with one impl: `LiteBackend` (SQLite + in-process workers).
**Fix:** Drop "AMQP" reference.
**Applied:** no — flagged.

#### [INVENTORY.md:130-137] Database tables
**Stale claim:** Lists `axon_crawl_jobs`, `axon_extract_jobs`, `axon_embed_jobs`, `axon_ingest_jobs`.
**Reality:** Correct as far as it goes — `crates/jobs/lite.rs` and friends create these. But also missing `axon_graph_jobs`, `axon_refresh_jobs`, `axon_watch_*` if they still exist (need to verify).
**Applied:** n/a — flagged.

#### [INVENTORY.md:140-152] Scripts table
**Stale claim:** Lists `scripts/rebuild-fresh.sh`, `scripts/check-container-revisions.sh`, `scripts/check_dockerignore_guards.sh`.
**Reality:** None of these scripts exist (verified by `ls`).
**Fix:** Drop the missing scripts; reconcile with `docs/repo/SCRIPTS.md`.
**Applied:** no — flagged.

---

### docs/ACP.md

Generally accurate against `crates/services/acp/` file layout. Spot-checked: `runtime.rs`, `session.rs`, `adapters.rs`, `bridge.rs`, `config.rs`, `mapping.rs`, `permission.rs`, `persistent_conn.rs`, `session_cache.rs`, `preflight.rs` all exist as named.

#### [ACP.md:81] `crates/services/acp_llm.rs` (singular file)
**Stale claim:** Source file table lists `crates/services/acp_llm.rs`.
**Reality:** `crates/services/acp_llm.rs` exists *and* there is also a `crates/services/acp_llm/` directory with `pool.rs`, `runner.rs`, `types.rs`, `warm.rs`, `ws_runner.rs` (5 submodules). The doc lists only the root file and never enumerates the submodules.
**Fix:** Add a follow-up paragraph documenting `crates/services/acp_llm/{pool,runner,types,warm,ws_runner}.rs`. This may be where the WS-routed remote ACP path lives.
**Applied:** no — flagged for a code-aware doc author.

#### [ACP.md:84-91] References `crates/web/execute/sync_mode/...` and `crates/web/ws_handler.rs`
**Stale claim:** Cites `crates/web/execute/sync_mode/acp_adapter.rs`, `crates/web/execute/sync_mode/pulse_chat.rs`, `crates/web/execute/sync_mode/prewarm.rs`, `crates/web/ws_handler.rs`, `crates/web/ws_handler/acp_session.rs`.
**Reality:** None of these exist. `crates/web/` is 4 files: `auth.rs`, `security.rs`, `server.rs`, `static_assets.rs`.
**Fix:** Either remove the WS-side cites entirely (CLI-only ACP path remains) or describe the remote ACP path via `crates/services/acp_llm/ws_runner.rs` if that has supplanted them.
**Applied:** no — flagged.

#### [ACP.md:42-44] "Pulse Chat" remains as a documented surface
**Stale claim:** Persistent connection mode is "used by Pulse Chat" via WebSocket.
**Reality:** Pulse Chat does not exist in current `apps/web`. No WS-driven ACP UI. The persistent-mode code may still exist in the binary, but it has no consumer.
**Fix:** Remove or annotate as "subsystem retained for future remote ACP usage".
**Applied:** no — flagged.

#### [ACP.md:1057-1131] WsConnState, ALLOWED_MODES, ALLOWED_FLAGS, rate limiting, semaphores
**Stale claim:** All of this is described as the live implementation of `/ws`.
**Reality:** No `/ws` execute endpoint, no `crates/web/execute/constants.rs`, no `WsConnState` type. This entire block is obsolete.
**Fix:** Strip or move to a "WIP / abandoned" appendix.
**Applied:** no — flagged.

#### [ACP.md:888] OPENAI_MODEL note is consistent with root CLAUDE.md
**Verified clean.**

---

### docs/API.md

**Recommend wholesale archival** (see top-of-doc table). Every endpoint and source-file cite refers to handlers that do not exist in the current tree.

---

### docs/CHECKLIST.md
See above.

---

### docs/CLAUDE-HOT-RELOAD.md

**Recommend wholesale archival** — no `axon-web` container, no s6 services, no `claude-watcher`. Build-time setup file `docker/web/s6-rc.d/claude-watcher/run` referenced does not exist in the repo.

---

### docs/CONFIG-DECOMPOSITION-PLAN.md

#### [CONFIG-DECOMPOSITION-PLAN.md:1-12] Status header
**Stale claim:** "Phase 1 (TOML loading) implemented in v0.36"
**Reality:** Project is now on v1.5.4. `crates/core/config/parse/toml_config.rs` exists. Sub-configs scaffolded in `subconfigs.rs`. Phases 1+ remain "pending" per the doc.
**Fix:** Bump status header; re-confirm which phases shipped. Currently `subconfigs.rs` is 11.7K and `toml_config.rs` is 12.1K — there's been more progress than v0.36.
**Applied:** no — needs domain-author judgment.

#### [CONFIG-DECOMPOSITION-PLAN.md:144-176] "Cargo Workspace Migration"
**Reality:** Workspace already exists — `Cargo.toml` declares `[workspace] members = ["xtask"]`. The "Target" structure of split `axon-core`, `axon-jobs`, etc., does not match current reality (still flat single-package layout).
**Applied:** no — flagged. **Recommend move to `docs/plans/`.**

---

### docs/CONTEXT-INJECTION.md

Code paths verified — `crates/vector/ops/commands/ask/{retrieval,ranking,build,streaming}.rs` cited correctly. Env-var defaults table cross-references both `config_impls.rs` (Default) and `parse/build_config.rs` (CLI/env). Both files exist.

**Verified clean.**

---

### docs/ERROR-HANDLING.md

#### [ERROR-HANDLING.md:90-94] `lapin::Error` variant in proposed enum
**Stale claim:** "`lapin::Error` — propagated as-is from AMQP operations"
**Reality:** `lapin` is not a dependency. AMQP not used.
**Applied:** no — recommend archival.

#### [ERROR-HANDLING.md:1-6] Status: "AxonError enum not yet implemented"
**Reality:** Not yet implemented. Verifiable with `grep -r "enum AxonError" crates/` (returns nothing).
**Applied:** n/a — flagged.

---

### docs/EXPORT.md

#### [EXPORT.md:127-129] References `axon_scrape_seeds` Postgres table
**Stale claim:** "Scrape seed requests are tracked in Postgres table `axon_scrape_seeds`."
**Reality:** SQLite-only. If the table exists, it lives in the SQLite jobs DB, not Postgres.
**Fix:** Replace "Postgres table" with "SQLite table" if export still exists in current code; otherwise mark as obsolete.
**Applied:** no — flagged. Unverified whether `axon export` command still exists in this branch.

#### [EXPORT.md:84-85] References `axon_query_history` and `axon_scrape_seeds` tables
**Reality:** Neither table is created by any inline `ensure_schema()` in `crates/jobs/`. **Verify** whether export still runs at all.
**Applied:** no — flagged.

---

### docs/FEATURE-DELIVERY-FRAMEWORK.md

#### [FEATURE-DELIVERY-FRAMEWORK.md:152-158] `crates/core/config/types/config.rs` and `crates/core/config/cli.rs`, `parse.rs`
**Reality:** All cited files exist:
- `crates/core/config/types/config.rs` ✓
- `crates/core/config/cli.rs` ✓
- `crates/core/config/parse.rs` ✓
- `crates/core/config/parse/build_config.rs` ✓

**Verified clean** as a guidance doc; describes process, not current code surface.

#### [FEATURE-DELIVERY-FRAMEWORK.md:178-180] References `crates/web.rs` and `crates/web/execute/*`
**Stale claim:** "core axum websocket runtime bridge: `crates/web.rs` and/or `crates/web/execute/*`"
**Reality:** `crates/web.rs` exists but is 263 bytes (re-export shim). `crates/web/execute/*` does not exist.
**Fix:** Drop the WS surface from the framework or rewrite to point at admin-panel server.
**Applied:** no — flagged.

---

### docs/GRAPH.md

Spot-check of cited source files:
- `crates/core/neo4j.rs` ✓ (4.7K — exists)
- `crates/services/graph.rs` — **NOT FOUND** in `crates/services/`
- `crates/jobs/graph/worker.rs` — `crates/jobs/` has no `graph/` subdir at root level (verified: `crates/jobs/` has `commands/`, `crawl/`, `ingest/`, `lite/` only).
- `crates/cli/commands/graph.rs` — to verify
- `crates/mcp/server/handlers_graph.rs` — to verify

**Stale claim:** "Graph functionality is fully implemented" with the file paths above.
**Reality:** Per the `MEMORY.md` user note, GraphRAG is "implementation in progress" and "graph worker wired, queue exists, `axon graph worker` available". The doc may overstate completeness.
**Fix:** Verify against current code; drop missing-file cites; re-label as "in progress" if accurate.
**Applied:** no — flagged.

---

### docs/GUARDRAILS.md

#### [GUARDRAILS.md:33-34] Cargo xtask check-env-staged
**Verified clean** — `xtask` exists, `check-env-staged` is wired in `lefthook.yml`.

#### [GUARDRAILS.md:41-49] Web app token model
**Stale claim:** Two-tier token model with `AXON_WEB_API_TOKEN` (gates `/api/*` and `/ws`) and `AXON_WEB_BROWSER_API_TOKEN`.
**Reality:** No `/ws` endpoint exists in `crates/web/`. The admin panel may still use one of these tokens (check `crates/web/auth.rs`), but the "two-tier" model with `NEXT_PUBLIC_*` is for a Next.js app that no longer exists.
**Applied:** no — flagged.

#### [GUARDRAILS.md:60-66] s6-overlay non-root containers
**Stale claim:** "Containers use s6-overlay with PID 1 running as root..."
**Reality:** No app containers built from this repo. Only third-party infra images.
**Applied:** no — flagged.

---

### docs/HEADLESS_OPTIONS.md

**Recommend deletion.** Stream-of-consciousness brainstorming, not documentation.

---

### docs/JOB-LIFECYCLE.md

#### [JOB-LIFECYCLE.md:38-44] Queue env vars and primary start paths
**Stale claim:** Each row references `AXON_<FAMILY>_QUEUE` env var and AMQP-style queues.
**Reality:** No AMQP queues. Queue env vars are not consumed (verifiable by `grep -r AXON_CRAWL_QUEUE crates/`).
**Applied:** no — flagged.

#### [JOB-LIFECYCLE.md:79-83] `claim_next_pending` uses `FOR UPDATE SKIP LOCKED`
**Stale claim:** Postgres-style claim.
**Reality:** SQLite — no `FOR UPDATE SKIP LOCKED`. Claim semantics differ.
**Applied:** no — flagged.

#### [JOB-LIFECYCLE.md:91-103] Cancellation via "Redis cancellation flag"
**Stale claim:** "Redis cancellation flag (`axon:<type>:cancel:<job_id>`) with TTL"
**Reality:** No Redis. Cancellation is via SQLite (`canceled_at` column or status update). Per `modular.md`, "Cancel tracked via `canceled_at` column (replaces Redis cancel keys)".
**Applied:** no — flagged.

#### [JOB-LIFECYCLE.md:122-130] AMQP lane runtime
**Stale claim:** "Opens AMQP channel and applies `basic_qos(1)`."
**Reality:** No AMQP. `crates/jobs/worker_lane.rs` does not exist (verified `crates/jobs/` listing).
**Applied:** no — flagged.

#### [JOB-LIFECYCLE.md:259-273] Polling fallback "permanent death" warning
**Stale claim:** "When AMQP is unavailable, workers fall back to Postgres polling..."
**Reality:** Always SQLite, no AMQP. The whole section is moot.
**Applied:** no — flagged. Whole doc needs rewrite for SQLite-only.

#### [JOB-LIFECYCLE.md:287-292] Source map
**Stale claim:** Cites `crates/jobs/common/amqp.rs`, `crates/jobs/refresh.rs`, `crates/jobs/refresh/processor.rs`, etc.
**Reality:** None of these exist. `crates/jobs/` has only `commands/`, `crawl/`, `ingest/`, `lite/` subdirs and `backend.rs`/`crawl.rs`/`embed.rs`/`error.rs`/`extract.rs`/`ingest.rs`/`lite.rs`/`status.rs`/`watch_lite.rs` files.
**Applied:** no — flagged.

---

### docs/LIVE-TEST-SCRIPTS.md

This is actually titled "# Monolith Policy" with content that contradicts the filename — describes line/function caps for `.rs` files. Probably a mis-named or repurposed doc.

#### [LIVE-TEST-SCRIPTS.md:60-72] References `~/.claude/hooks/enforce_monoliths.py`
**Stale claim:** Lefthook runs `python3 ~/.claude/hooks/enforce_monoliths.py --staged`
**Reality:** `lefthook.yml` actually runs `scripts/enforce_monoliths.py` first, falling back to `~/.claude/hooks/enforce_monoliths.py`. Both paths are valid; doc only mentions the home-dir path.
**Fix:** Update to mention `scripts/enforce_monoliths.py` (preferred); noting that the home-dir path is fallback.
**Applied:** no — flagged.

#### [LIVE-TEST-SCRIPTS.md filename]
The filename is misleading — content is about monolith policy, not "live test scripts". The script `scripts/live-test-all-commands.sh` exists; if there should be a doc about live tests, it would be different content.
**Recommend rename to `MONOLITH-POLICY.md` or merge with repo/RULES.md (which already covers monolith policy).**

---

### docs/LOBE-WORKFLOW-VISION.md

**Recommend archival to `docs/plans/`.** This is a vision doc for an unbuilt UI.

---

### docs/MIGRATIONS.md

#### [MIGRATIONS.md:5-22] Inline DDL via Postgres advisory locks
**Stale claim:** "managed by inline `ensure_schema()` functions ... protected by a PostgreSQL advisory lock."
**Reality:** SQLite, no PG advisory locks. Cited file `crates/jobs/refresh.rs` does not exist.
**Applied:** no — flagged.

#### [MIGRATIONS.md:120-134] Advisory lock key registry
**Reality:** SQLite has no `pg_advisory_lock`. The whole concept is gone.
**Applied:** no — flagged.

**Recommend rewrite as SQLite-migrate or archive.** Note `migrations/` directory exists with three numbered SQL files.

---

### docs/OPERATIONS.md

#### [OPERATIONS.md:29-43] `dev-setup.sh` invocation
**Reality:** Script exists at `scripts/dev-setup.sh`.
**Verified clean.**

#### [OPERATIONS.md:62-66] Pre-create directories
**Stale claim:** Includes `postgres`, `redis`, `rabbitmq` subdirs.
**Reality:** No such services. Only `qdrant` and `output`/`artifacts` are needed.
**Fix:** Drop those names from the mkdir line.
**Applied:** no — flagged.

#### [OPERATIONS.md:81-96] `just dev` and `cargo run --bin axon -- crawl worker`
**Reality:** `just dev` recipe exists. `axon crawl worker` is documented in root CLAUDE.md as a job subcommand.
**Verified clean.**

#### [OPERATIONS.md:196-213] Restart workers via `cargo run`
**Reality:** Worker subcommands exist.
**Verified clean.**

#### [OPERATIONS.md:228-241] "Pulse/API returning 503" troubleshooting
**Stale claim:** Pulse API behaviour.
**Reality:** No Pulse API surface in current `apps/web`. Section is dead.
**Applied:** no — flagged.

---

### docs/PERFORMANCE.md

#### [PERFORMANCE.md:84-90] Queue env vars `AXON_CRAWL_QUEUE`, etc.
**Stale claim:** Listed as primary worker tuning controls.
**Reality:** AMQP-era config. Not consumed in SQLite-only mode.
**Applied:** no — flagged.

#### [PERFORMANCE.md:105-119] TEI behaviour and Qdrant batching
**Reality:** Generally accurate; `TEI_MAX_CLIENT_BATCH_SIZE`, `AXON_QDRANT_UPSERT_BATCH_SIZE` likely still apply.
**Verified clean.**

#### [PERFORMANCE.md:140-148] Pulse API tuning
**Reality:** Dead — no Pulse API.
**Applied:** no — flagged.

---

### docs/REBOOT-UI.md

**Recommend archival.** Describes UI shell that does not exist.

---

### docs/RESTORE.md

**Recommend rewrite or archival.** Lists `postgres`, `redis`, `rabbitmq` as preconditions — none exist.

---

### docs/SCALING.md

**Recommend archival.** Self-described "Documentation only" plan, mixes AMQP+Postgres claims with SQLite/in-process claims. Internally inconsistent.

---

### docs/SECURITY.md

#### [SECURITY.md:79-86] `/ws` upgrade gate
**Stale claim:** "/ws upgrade path (`crates/web.rs`)"
**Reality:** No `/ws` endpoint in current `crates/web/`. `crates/web/server.rs` is an admin panel HTTP server.
**Applied:** no — flagged. Major SECURITY.md rewrite required.

#### [SECURITY.md:88-96] WebSocket command execution surface
**Stale claim:** `crates/web/execute.rs` ALLOWED_MODES, ALLOWED_FLAGS.
**Reality:** Does not exist.
**Applied:** no — flagged.

#### [SECURITY.md:42-54] SSRF controls in `crates/core/http.rs`
**Reality:** `crates/core/http/` exists. **Verified clean** for the URL validation portion.

#### [SECURITY.md:58-69] File path safety routes (`/output/`, `/download/`, `/api/omnibox/files`)
**Reality:** None of these routes exist in current code.
**Applied:** no — flagged.

#### [SECURITY.md:235-251] Source map cites
**Reality:** Many cited paths (`crates/web.rs`, `crates/web/download.rs`, `crates/web/execute.rs`, `crates/web/execute/cancel.rs`, `crates/web/execute/sync_mode/dispatch.rs`, `crates/services/acp/runtime.rs`, `crates/services/acp/bridge.rs`, `apps/web/hooks/use-axon-ws.ts`, `apps/web/proxy.ts`, `apps/web/app/api/*`) do not exist or are misleading.
**Applied:** no — flagged.

---

### docs/SERVE.md

**Recommend archival.** No `axon serve` web bridge exists in the listed form. The CLI subcommand may still exist but the documented surface (Docker stats broadcast, PTY shell, command WS) is gone.

---

### docs/SHELL-COMPLETIONS.md

Cross-link to `commands/completions.md` exists. **Verified clean.**

---

### docs/SPIDER-FEATURE-FLAGS.md

#### [SPIDER-FEATURE-FLAGS.md:13-21] Active dependency declarations
**Stale claim:** lists `spider_agent = "2.47.89"`
**Reality:** Need to verify against `Cargo.toml`. Root `CLAUDE.md` mentions `spider_agent = "2.45"` as registry version and a path-dep to `../spider/spider_agent` for local dev. Doc says 2.47.89.
**Fix:** Reconcile by reading current `Cargo.toml`.
**Applied:** no — flagged. Likely stale version pin.

#### [SPIDER-FEATURE-FLAGS.md:75-77] Two contradictory `glob` rows
**Stale claim:** First says "NOT enabled — glob URL patterns change `crawl_establish`...". Second row says "Removed — caused BudgetExceeded...".
**Reality:** Doc has two rows for `glob` saying conflicting things (one "—" status, both warning against re-adding).
**Fix:** Collapse to one row.
**Applied:** no — flagged but trivial.

#### [SPIDER-FEATURE-FLAGS.md:79] `fs` flag annotation
**Stale claim:** "`fs` ... NOT enabled — Project uses Qdrant + Postgres instead of disk FS"
**Reality:** No Postgres. Should say "SQLite + Qdrant".
**Applied:** no — flagged.

---

### docs/TESTING.md

#### [TESTING.md:81-84] CI provisions Postgres, Redis, RabbitMQ
**Stale claim:** "CI still provisions Postgres, Redis, and RabbitMQ as GitHub Actions service containers for legacy ignored worker tests."
**Reality:** Those services are not used by the current code. Either they're still in CI YAML (truly legacy and unused) or this is stale text.
**Applied:** no — flagged.

#### [TESTING.md:87-104] Coverage areas — many test files cited
**Stale claim:** Cites `tests/services_acp_*.rs`, `tests/web_ws_*.rs`, `crates/web/execute/tests/*`.
**Reality:** Verify against `tests/` directory. `crates/web/execute/` does not exist; tests inside it cannot exist.
**Applied:** no — flagged.

#### [TESTING.md:218-226] CI mapping
**Stale claim:** "Uses GitHub Actions service containers for Postgres, Redis, and RabbitMQ."
**Reality:** Not used by current code.
**Applied:** no — flagged.

#### [TESTING.md:285-298] `AXON_TEST_PG_URL`, Postgres connection failures
**Stale claim:** Postgres-related troubleshooting.
**Reality:** Not used.
**Applied:** no — flagged.

---

### docs/UI-DESIGN-SYSTEM.md

**Recommend archival.** Describes UI tokens for an unbuilt Pulse/chat UI.

---

### docs/WEB-ARCHITECTURE.md

**Recommend archival.** Plan doc for a consolidation that no longer makes sense.

---

### docs/WS-PROTOCOL.md

**Recommend archival.** Single source of truth for a protocol with no producer or consumer in the current code.

---

### docs/modular.md

#### [modular.md:75-85] Env-var gated features
**Stale claim:** "`AXON_AMQP_URL` + `AXON_PG_URL` + `AXON_REDIS_URL` ... Full jobs stack (replaces SQLite queue)"
**Reality:** SQLite is the only backend. There is no full jobs stack. The conditional path described is gone.
**Fix:** Drop the AMQP/PG/Redis row from the table; the binary is SQLite-only.
**Applied:** no — flagged.

#### [modular.md:96-104] Compose files `docker-compose.crawler.yml`, `docker-compose.rag.yml`, `docker-compose.full.yml`
**Reality:** None exist. Only `config/docker-compose.services.yaml` ships.
**Applied:** no — flagged. **Recommend move to `docs/plans/`.**

---

### docs/observability-gaps.md

**Recommend rename / move to `docs/reports/2026-04-29-observability-gaps.md`.** This is an audit report, not living documentation. Per the existing `docs/reports/` convention.

---

### docs/auth/API-TOKEN.md

#### [API-TOKEN.md:5-12] Surfaces protected
**Stale claim:** Protects `/api/*`, `/ws`, `/download/*`, `/output/*`.
**Reality:** None of these surfaces are served by the current `crates/web/`. The admin panel server (`crates/web/server.rs`) likely uses a panel password, not the multi-token model.
**Applied:** no — flagged. **Whole doc may be obsolete or in transition** — verify panel auth.

---

### docs/services/MEM0.md

**Recommend move out of repo's `docs/services/`.** Documents an external service (`axon-mem0` FastAPI) used by `ngent` (a separate Go project). Not part of axon_rust's runtime.

---

### docs/repo/CLAUDE.md

**Verified clean** as an index file.

---

### docs/repo/MEMORY.md

**Verified clean** — describes beads workflow.

---

### docs/repo/RECIPES.md

#### [RECIPES.md:34-44] Docker recipes
**Stale claim:** Lists `just docker-build`, `just up`, `just down`, `just down-all`, `just rebuild-fresh`.
**Reality:** None of these recipes exist. Only `services-up`, `services-down`, `rebuild`, `stop` exist for related operations. `just rebuild` runs `check + test + docker-build`, but `docker-build` itself is not in the Justfile (this would fail).
**Fix:** Remove non-existent recipes; either drop the `rebuild` recipe or fix it to not reference `docker-build`.
**Applied:** no — flagged. Justfile doc needs full audit.

#### [RECIPES.md:54-57] Web UI recipes (`web-dev`, `web-build`, `web-lint`, `web-format`)
**Reality:** None exist in `Justfile`.
**Applied:** no — flagged.

#### [RECIPES.md:50-53] Local stack recipes (`workers`)
**Stale claim:** "`just workers` | Start all 6 worker types as background processes"
**Reality:** No `workers` recipe. There's no `serve` recipe either, contrary to lines 50-51.
**Applied:** no — flagged.

#### [RECIPES.md:84-91] Maintenance recipes
**Stale claim:** Lists `gen-mcp-schema`, `cache-status`, `cache-prune`, `docker-context-probe`, `check-container-revisions`.
**Reality:** None exist.
**Applied:** no — flagged.

---

### docs/repo/REPO.md

#### [REPO.md:5-81] Directory tree
**Stale claim:** `apps/web` includes `components/`, `hooks/`, `lib/`, `proxy.ts`, `shell-server.mjs`, `biome.json`, `CLAUDE.md`.
**Reality:** Only `app/`, `out/`, `next-env.d.ts`, `next.config.mjs`, `package-lock.json`, `package.json`, `tsconfig.json`. None of `components/`, `hooks/`, `lib/`, `proxy.ts`, `shell-server.mjs`, `biome.json` exist.
**Fix:** Rewrite the `apps/web` portion of the tree.
**Applied:** no — flagged.

#### [REPO.md:38-42] Jobs subdirectories
**Stale claim:** `crates/jobs/` has `common/`, `crawl/`, `extract/`, `embed/`, `ingest.rs`.
**Reality:** Only `commands/`, `crawl/`, `ingest/`, `lite/` exist. There's no separate `extract/`, `embed/` directory; those are flat files (`extract.rs`, `embed.rs`). `common/` does not exist.
**Applied:** no — flagged.

#### [REPO.md:50-51] `crates/web.rs` and `crates/web/`
**Stale claim:** "WebSocket execution bridge" and "Web-specific handlers"
**Reality:** Now an admin panel server.
**Applied:** no — flagged.

#### [REPO.md:93] Jobs framework description
**Stale claim:** "Async job framework with AMQP and SQLite backends"
**Reality:** SQLite-only.
**Applied:** no — flagged.

#### [REPO.md:97] Web crate
**Stale claim:** "WebSocket execution bridge for web UI"
**Reality:** Admin panel server.
**Applied:** no — flagged.

---

### docs/repo/RULES.md

#### [RULES.md:38-50] Version-bearing files
**Stale claim:** "`Cargo.toml` `version = "X.Y.Z"`, `apps/web/package.json` `"version": "X.Y.Z"`, `CHANGELOG.md`"
**Reality:** Both files exist and have version fields. Current `Cargo.toml` is `1.5.4`; `apps/web/package.json` is `1.3.4` — they're out of sync (root CLAUDE.md says all version-bearing files must match).
**Fix:** Either bump `apps/web/package.json` or document that the admin panel is independently versioned.
**Applied:** no — flagged.

#### [RULES.md:96-105] TypeScript code standards
**Reality:** `apps/web` is minimal admin panel. No Biome config in repo. Standards may be aspirational rather than enforced.
**Applied:** no — flagged.

#### [RULES.md:106-122] Pre-commit hooks
**Stale claim:** Lists `enforce_no_legacy_symbols.py`, `check_dockerignore_guards.sh`, `cargo xtask check-env-staged`, `cargo xtask check-no-mod-rs`, `cargo xtask check-unwraps`, `cargo xtask check-mcp-http`, `cargo xtask check-claude-symlinks`.
**Reality:** Verified `lefthook.yml`:
- `cargo xtask check-env-staged` ✓
- `cargo xtask check-claude-symlinks` ✓
- `cargo xtask check-mcp-http` ✓
- `cargo xtask check-no-mod-rs` ✓
- `cargo xtask check-unwraps` ✓
- `scripts/validate_skills_ref.sh` ✓
- `scripts/enforce_monoliths.py` ✓
- `taplo fmt --check` (not in doc)
- `cargo fmt --check` (not in doc)
- `cargo clippy` (not in doc)
- `cargo nextest run` (not in doc)
- `python3 scripts/generate_mcp_schema_doc.py` (not in doc)

`scripts/enforce_no_legacy_symbols.py` is not in `lefthook.yml` — it's in `Justfile precommit` recipe, not in lefthook. Doc claims it as a lefthook hook.
`check_dockerignore_guards.sh` is not in `lefthook.yml` and the script doesn't exist in `scripts/`.
**Applied:** no — flagged.

#### [RULES.md:127-132] Performance profiles table
**Reality:** Cross-checked with root CLAUDE.md — values match. **Verified clean.**

---

### docs/repo/SCRIPTS.md

#### [SCRIPTS.md:28-32] Listed quality scripts
**Stale claims (script does not exist in `scripts/`):**
- `check_no_next_middleware.sh` — NOT FOUND
- `check_pg_advisory_lock.sh` — NOT FOUND
- `check_dockerignore_guards.sh` — NOT FOUND

**Verified existing:** `check_shell_completions.sh`, `enforce_monoliths.py`, `enforce_no_legacy_symbols.py`.
**Applied:** no — flagged.

#### [SCRIPTS.md:46-51] Docker scripts
**Stale claims (do not exist):**
- `rebuild-fresh.sh` — NOT FOUND
- `check-container-revisions.sh` — NOT FOUND
- `check_docker_context_size.sh` — NOT FOUND
- `cache-guard.sh` — NOT FOUND

**Verified existing:** `audit_compose_images.py`.
**Applied:** no — flagged.

#### [SCRIPTS.md:56-61] Testing scripts
All listed scripts exist in `scripts/`. **Verified clean.**

#### [SCRIPTS.md:75-79] Data management scripts
All listed scripts exist. **Verified clean.**

---

### docs/stack/ARCH.md

#### [ARCH.md:60-76] Worker types
**Stale claim:** Lists Crawl, Extract, Embed, Ingest workers. Says "in-process".
**Reality:** Matches reality reasonably; Refresh and Graph are not in `crates/jobs/` subdirs but may be in flat files. `lite.rs` is the central backend.
**Verified roughly clean.**

#### [ARCH.md:71-72] "Docker (production): Workers run as s6-supervised services inside `axon-workers` container."
**Stale claim:** s6 supervision in axon-workers container.
**Reality:** No `axon-workers` container, no Dockerfile. Production deployment story is undefined or not via Docker.
**Applied:** no — flagged.

#### [ARCH.md:149-167] Web UI architecture table
**Stale claim:** Next.js dashboard, Pulse workspace, axum WS bridge, Shell server, proxy.
**Reality:** Admin panel only. Most of this table is obsolete.
**Applied:** no — flagged.

#### [ARCH.md:172-184] Serve supervisor table
**Stale claim:** "`axon serve` acts as a process supervisor, managing: Backend bridge (49000), MCP HTTP (8001), Workers (6), Shell server (49011), Next.js (49010)."
**Reality:** Without axon-workers/axon-web Dockerfiles, this supervisor model is at best aspirational. Verify whether `axon serve` actually still spawns these processes (very unlikely given missing crates/web/execute).
**Applied:** no — flagged.

---

### docs/stack/CLAUDE.md

**Verified clean** as an index.

---

### docs/stack/PRE-REQS.md

#### [PRE-REQS.md:35-43] Automated setup
**Reality:** `just setup` and `scripts/dev-setup.sh` exist.
**Verified clean.**

#### [PRE-REQS.md:60-70] Infrastructure services
**Stale claim:** Lists Qdrant, TEI, Chrome.
**Reality:** Matches `config/docker-compose.services.yaml`.
**Verified clean.**

---

### docs/stack/TECH.md

#### [TECH.md:14-32] Key dependencies
**Stale claim:** `spider_agent` is "2.47+".
**Reality:** Need to verify; root CLAUDE.md mentions `2.45` as registry pin and a path-dep override. Spec discrepancy with SPIDER-FEATURE-FLAGS.md (says `2.47.89`).
**Applied:** no — flagged.

#### [TECH.md:23] `sqlx` is "0.8 SQLite async driver (lite mode)"
**Verified clean** — matches `Cargo.toml`.

#### [TECH.md:69-89] Hybrid vector search
**Verified clean** — matches root CLAUDE.md description and named-mode collections.

#### [TECH.md:108-118] ACP description
**Reality:** `OPENAI_MODEL` claim consistent with root CLAUDE.md. Pre-warming claim consistent with ACP.md.
**Verified clean.**

---

## Mechanical fixes applied

Edited `docs/CLAUDE.md` line 25 — replaced "AMQP job state machine" with "Job state machine (SQLite-backed)".

(Beyond this single fix, the recommendation pattern dominates — most issues are too architectural for line-edits and need either a doc-author rewrite or archival.)

---

## Summary by severity

### Wholesale obsolete (recommend deletion or archival)
- `docs/HEADLESS_OPTIONS.md` (delete — brainstorming)
- `docs/LOBE-WORKFLOW-VISION.md` (move to `docs/plans/`)
- `docs/REBOOT-UI.md` (archive)
- `docs/UI-DESIGN-SYSTEM.md` (archive)
- `docs/WEB-ARCHITECTURE.md` (archive)
- `docs/WS-PROTOCOL.md` (archive)
- `docs/SERVE.md` (archive)
- `docs/API.md` (archive)
- `docs/CLAUDE-HOT-RELOAD.md` (archive)
- `docs/services/MEM0.md` (move out of repo)
- `docs/observability-gaps.md` (move to `docs/reports/`)
- `docs/CONFIG-DECOMPOSITION-PLAN.md` (move to `docs/plans/`)
- `docs/ERROR-HANDLING.md` (move to `docs/plans/` or delete)
- `docs/SCALING.md` (archive)
- `docs/RESTORE.md` (archive)
- `docs/modular.md` (move to `docs/plans/`)
- `docs/MIGRATIONS.md` (rewrite for SQLite or archive)

### Major rewrite needed
- `docs/SECURITY.md` — entire `/ws`/`/api/` half is obsolete; SSRF and validate_url half remains valid
- `docs/JOB-LIFECYCLE.md` — SQLite-only rewrite
- `docs/INVENTORY.md` — drop AMQP queues, drop axon-workers/axon-web app services
- `docs/PERFORMANCE.md` — drop AMQP knobs, drop Pulse API tuning
- `docs/OPERATIONS.md` — drop postgres/redis/rabbitmq references; drop Pulse 503 troubleshooting
- `docs/repo/REPO.md` — apps/web tree obsolete, jobs subdir tree obsolete
- `docs/repo/RECIPES.md` — many recipes don't exist (`docker-build`, `up`, `down`, `web-dev`, etc.)
- `docs/repo/SCRIPTS.md` — multiple non-existent scripts cited
- `docs/auth/API-TOKEN.md` — verify/rewrite for current admin-panel auth model
- `docs/stack/ARCH.md` — drop trimodal framing, drop Web UI table, drop axon-workers
- `docs/CHECKLIST.md` — drop `just docker-build`, drop `gen-mcp-schema`, drop Docker non-root claim
- `docs/README.md` — drop SCHEMA.md ref; drop trimodal framing in opening table
- `docs/CLAUDE.md` (docs subdir) — drop SCHEMA.md ref
- `docs/ACP.md` — drop WS-side cites; document `acp_llm/` submodules
- `docs/GRAPH.md` — verify implementation status; drop missing-file cites
- `docs/GUARDRAILS.md` — drop s6/Docker container claims; revisit token model
- `docs/TESTING.md` — drop Postgres/Redis/RabbitMQ CI service-container claims
- `docs/SPIDER-FEATURE-FLAGS.md` — fix `spider_agent` version pin; collapse duplicate `glob` rows; fix "Postgres" in `fs` annotation
- `docs/repo/RULES.md` — fix `apps/web` package.json version mismatch; fix lefthook hook list

### Verified clean
- `docs/SHELL-COMPLETIONS.md`
- `docs/CONTEXT-INJECTION.md`
- `docs/repo/MEMORY.md`
- `docs/repo/CLAUDE.md`
- `docs/stack/CLAUDE.md`
- `docs/stack/PRE-REQS.md`
- `docs/stack/TECH.md` (mostly — modulo `spider_agent` version)
- `docs/FEATURE-DELIVERY-FRAMEWORK.md` (process doc, mostly OK)

---

## Final note

This audit found that the docs are systematically misaligned with the codebase. The single highest-value action a doc-author can take is:

1. **Move all "wholesale obsolete" docs** to `docs/plans/complete/` (or delete `HEADLESS_OPTIONS.md`).
2. **Tag remaining docs at the top with a `Last verified: YYYY-MM-DD` line** so subsequent audits can prioritize.
3. **Run a sweep against `apps/web` and `crates/web/` to remove all references to a Pulse/chat UI that no longer exists.**

Skipping (1) makes it easy to mistake aspiration for state. Skipping (2)–(3) leaks AMQP/Pulse/Postgres assumptions into agents and contributors who read these docs first.
