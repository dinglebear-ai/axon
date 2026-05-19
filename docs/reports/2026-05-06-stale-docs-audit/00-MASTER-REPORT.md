# Stale Docs Audit — Master Report

**Date:** 2026-05-06
**Branch:** main @ 69d0917b
**Scope:** Comprehensive audit of all active documentation in axon_rust (excluding `docs/sessions/`, `docs/plans/complete/`, `docs/reports/`, `docs/superpowers/`)
**Method:** 6 parallel audit agents, each scoped to a disjoint slice; verified every claim against current code.

---

## Executive Summary

The repo's docs lag behind a major architectural transition. Over the last few months the codebase removed **Postgres**, **AMQP/lapin**, **Redis**, the **Pulse chat UI**, the **graph/refresh/export** commands, and large parts of the **web app**, replacing them with a SQLite-backed lite-mode-only architecture. **Most of the documentation still describes the pre-transition system.**

| Domain | Files audited | Findings | Fixes applied | Status |
|--------|--------------:|---------:|--------------:|--------|
| A — Root + foundational | 7 | 39 | 11 | Partially fixed; 28 flagged |
| B — Command docs | 38 | 19 | 16 | Mostly clean (22 verified) |
| C — MCP docs | 12 | 25 | 22 | Largely fixed (1 fictional doc rewritten wholesale) |
| D — Per-crate CLAUDE.md/README | 16 | ~22 | ~15 | Mostly fixed; 5 crates were highly stale |
| E — Plugin skills + meta | 19 | 12 | 5 | Clean |
| F — Misc docs | 45 | many | 1 | **17 wholesale-obsolete; needs strategic decisions** |
| **Total** | **137** | **120+** | **70** | **47 files modified** |

**70 mechanical fixes applied** across 47 files. Lots of judgment-heavy items remain.

---

## Critical themes (cross-cutting)

### 1. Removed commands still documented
`refresh`, `graph`, `export`, `artifacts` are not in `CommandKind` (`crates/core/config/types/enums.rs`), have no parsers, and no handlers. They are still:
- Listed in `README.md` (~250 lines), `CLAUDE.md`, `docs/ARCHITECTURE.md`, `docs/commands/README.md`
- Have dedicated docs at `docs/commands/{export,graph,refresh}.md` — **flagged for deletion**
- Referenced as MCP actions in `docs/MCP-TOOL-SCHEMA.md`, `docs/mcp/TOOLS.md`, `docs/mcp/PATTERNS.md` — partially purged
- Mentioned in per-crate CLAUDE.md/README files

### 2. Removed multi-backend infra still documented
The codebase is **lite-mode-only** (SQLite + in-process workers). Removed but still in docs:
- Env vars: `AXON_PG_URL`, `AXON_REDIS_URL`, `AXON_AMQP_URL`
- CLI flags: `--pg-url`, `--amqp-url`, `--redis-url`
- "Full mode" / "lite mode" framing where lite is presented as opt-in (it's the only mode now)
- Postgres / Redis / RabbitMQ / lapin references throughout
- `ServiceCapabilities` mechanism with `ctx.capabilities.watch_scheduler` — `crates/services/context.rs` has only `{ cfg, jobs }`, no capabilities field

### 3. Fictional auth flow
`docs/auth/MCP-AUTH.md` documented a Google OAuth broker, `atk_` tokens, dynamic client registration, `/.well-known/oauth-*` endpoints, and Redis-backed token persistence. **None of this exists in code.** Real auth is just `AXON_MCP_HTTP_TOKEN` Bearer/x-api-key in `crates/mcp/auth.rs`. **Fully rewritten by Agent C.**

### 4. `crates/web` and `apps/web` are far smaller than docs claim
- `apps/web` is now `@axon/admin-panel` (admin/setup UI) — docs describe a Pulse/chat surface with `app/api/`, hooks, tests
- `crates/web/` is 4 admin-server files; docs reference `crates/web/execute/` tree that doesn't exist
- No `docker/` Dockerfiles or `axon-workers`/`axon-web` containers exist
- Many Justfile recipes documented (`docker-build`, `up`, `down`, `down-all`, `rebuild-fresh`, `web-dev`, `web-build`, `web-lint`, `web-format`, `workers`, `gen-mcp-schema`) are gone

### 5. Wrong default values
- `AXON_ASK_CANDIDATE_LIMIT` doc said 64; actual is 150 (`crates/core/config/types/config.rs:267` doc-comment also wrong)
- Status JSON shape no longer has `local_refresh_jobs` / `local_graph_jobs` keys
- `crates/mcp/assets/status_dashboard.html:218-219` references those dead keys

### 6. Per-crate file layouts fabricated
- `crates/cli/CLAUDE.md` referenced phantom `commands/graph.rs`
- `crates/jobs/CLAUDE.md` fabricated `crawl/{processor,repo,watchdog,worker,runtime}.rs`; real layout is `lite/workers/runners/{crawl,embed,extract,ingest}.rs`
- `crates/mcp/CLAUDE.md` claimed `config.rs` (doesn't exist) and missed several real handler files
- `crates/crawl/CLAUDE.md` treated `engine.rs` as flat (it has an 11-file `engine/` subdir)
- Helper functions misattributed to `common.rs`; real homes are `common_urls.rs`, `common_jobs.rs`

---

## Files in three buckets

### Bucket 1: Verified clean (no action)
- `docs/SHELL-COMPLETIONS.md`
- `docs/CONTEXT-INJECTION.md`
- `docs/repo/CLAUDE.md`, `docs/repo/MEMORY.md`
- `docs/stack/CLAUDE.md`, `docs/stack/PRE-REQS.md`, `docs/stack/TECH.md` (mostly)
- `docs/FEATURE-DELIVERY-FRAMEWORK.md`
- `plugins/axon/.mcp.json` (newly written)
- 22 of 32 command docs

### Bucket 2: Fixed inline (committed in this audit)
47 files — see `git status` output. Mechanical fixes for: version stamps, dead cross-refs, removed flag tables, renamed module paths, fabricated file-layout diagrams, missing default values, removed env vars, ServiceContext claims.

### Bucket 3: Need strategic decisions (NOT auto-fixed)

**Recommend deletion:**
- `docs/HEADLESS_OPTIONS.md` — brainstorming notes
- `docs/commands/export.md`, `docs/commands/graph.md`, `docs/commands/refresh.md` — commands don't exist
- `docs/auth/MCP-AUTH.md` — already rewritten but consider whether the original concepts are revisiting later
- `docs/commands/serve.md` — describes a fictional supervisor

**Recommend archive (move to `docs/plans/complete/` or delete):**
- `docs/CONFIG-DECOMPOSITION-PLAN.md` (refactor done)
- `docs/LOBE-WORKFLOW-VISION.md` (vision, never executed)
- `docs/REBOOT-UI.md` (UI was reverted to admin-only)
- `docs/observability-gaps.md` (was it ever closed?)
- `docs/RESTORE.md`, `docs/SCALING.md`, `docs/MIGRATIONS.md` (lite-mode-only architecture)

**Need full rewrite:**
- `docs/SECURITY.md`, `docs/JOB-LIFECYCLE.md`, `docs/INVENTORY.md`, `docs/PERFORMANCE.md`, `docs/OPERATIONS.md`
- `docs/repo/REPO.md`, `docs/repo/RECIPES.md`, `docs/repo/SCRIPTS.md`
- `docs/auth/API-TOKEN.md`, `docs/stack/ARCH.md`, `docs/CHECKLIST.md`
- `docs/CLAUDE.md`, `docs/ACP.md`, `docs/GRAPH.md`, `docs/GUARDRAILS.md`, `docs/TESTING.md`
- `docs/SPIDER-FEATURE-FLAGS.md`, `docs/repo/RULES.md`
- `docs/UI-DESIGN-SYSTEM.md`, `docs/WEB-ARCHITECTURE.md`, `docs/WS-PROTOCOL.md`, `docs/SERVE.md`, `docs/API.md`, `docs/CLAUDE-HOT-RELOAD.md`, `docs/services/MEM0.md`, `docs/ERROR-HANDLING.md`, `docs/modular.md`
- Root `CLAUDE.md` (large blocks describing removed commands and full-mode infra)
- Root `README.md` (~250 lines describing nonexistent commands)
- `docs/ARCHITECTURE.md` (path references to nonexistent files)

**Auto-generated docs (manual edits will be lost):**
- `docs/MCP-TOOL-SCHEMA.md` — generated by `scripts/generate_mcp_schema_doc.py`. Generator script needs updating.

---

## Top-priority action list

1. **Delete the 4 docs for removed commands** (`export.md`, `graph.md`, `refresh.md`, plus root `CLAUDE.md` / `README.md` blocks describing them).
2. **Update generator script** for `docs/MCP-TOOL-SCHEMA.md` to reflect current schema (or re-run if it auto-detects).
3. **Decide fate of obsolete docs** (Bucket 3 deletion / archive lists). Recommend a `git rm -r` pass with explicit consent for each.
4. **Rewrite `docs/SECURITY.md`, `docs/JOB-LIFECYCLE.md`, `docs/OPERATIONS.md`** — these are user-facing and most damaging when wrong.
5. **Rewrite root `CLAUDE.md` and `README.md` command tables** — these are the first thing readers see.
6. **Fix the doc-comment in `crates/core/config/types/config.rs:267`** (`Default: 64` → `Default: 150`).
7. **Fix `crates/mcp/assets/status_dashboard.html:218-219`** (dead `local_refresh_jobs` / `local_graph_jobs` references).
8. **Address `crates/cli/CLAUDE.md` and `crates/jobs/CLAUDE.md`** — both still contain fabricated module layouts (partial fix only).

---

## Detailed per-domain reports

- [A — Root + Foundational](./A-root-foundational.md) — 39 findings, 11 fixes
- [B — Commands](./B-commands.md) — 19 findings, 16 fixes
- [C — MCP](./C-mcp.md) — 25 findings, 22 fixes
- [D — Per-crate](./D-per-crate.md) — 22 findings, 15 fixes
- [E — Plugin skills](./E-plugin-skills.md) — 12 findings, 5 fixes
- [F — Misc docs](./F-misc-docs.md) — many findings, 1 fix; 17 wholesale-obsolete

---

## Files modified (47)

### Plugin manifest + skills
- `.claude-plugin/plugin.json` (skill count 15→16)
- `plugins/axon/CHANGELOG.md` (added 1.5.2/1.5.3/1.5.4 entries)
- `plugins/axon/README.md` (full rewrite from placeholder)
- `plugins/axon/skills/axon/SKILL.md`, `plugins/axon/skills/extract/SKILL.md`

### Root
- `CLAUDE.md`, `README.md`

### docs/ top level
- `docs/ARCHITECTURE.md`, `docs/CLAUDE.md`, `docs/DEPLOYMENT.md`, `docs/SETUP.md`
- `docs/MCP.md`, `docs/MCP-TOOL-SCHEMA.md`, `docs/auth/MCP-AUTH.md`

### docs/mcp/
- `CONNECT.md`, `DEPLOY.md`, `DEV.md`, `ENV.md`, `PATTERNS.md`, `TOOLS.md`, `TRANSPORT.md`

### docs/commands/
- `README.md`, `ask.md`, `domains.md`, `embed.md`, `evaluate.md`, `mcp.md`, `query.md`, `sources.md`, `stats.md`, `status.md`, `suggest.md`, `watch.md`

### Per-crate
- `crates/README.md`
- `crates/cli/CLAUDE.md`, `crates/cli/README.md`
- `crates/core/CLAUDE.md`, `crates/core/README.md`
- `crates/crawl/CLAUDE.md`, `crates/crawl/README.md`
- `crates/ingest/CLAUDE.md`, `crates/ingest/README.md`
- `crates/jobs/CLAUDE.md`, `crates/jobs/README.md`
- `crates/mcp/CLAUDE.md`, `crates/mcp/README.md`
- `crates/services/CLAUDE.md`
- `crates/vector/CLAUDE.md`
