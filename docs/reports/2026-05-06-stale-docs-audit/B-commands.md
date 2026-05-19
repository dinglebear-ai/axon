# B-commands.md — CLI Command Documentation Audit
**Date:** 2026-05-06
**Scope:** `docs/commands/*.md` (33 files) + `docs/ingest/*.md` (5 files) = 38 total
**Auditor:** Stale-docs scan against `crates/cli/`, `crates/core/config/`, `crates/services/`, `crates/mcp/` as ground truth.

---

## Canonical command list (ground truth)

`crates/core/config/types/enums.rs::CommandKind` and `crates/core/config/cli.rs::CliCommand` define **28** subcommands:

`Scrape, Crawl, Watch, Map, Extract, Search, Embed, Debug, Doctor, Query, Retrieve, Ask, Evaluate, Suggest, Sources, Domains, Stats, Status, Dedupe, Ingest, Sessions, Research, Screenshot, Completions, Mcp, Serve, Setup, Migrate`.

**There is no `Export`, `Github`, `Graph`, `Reddit`, `Refresh`, or `Youtube` subcommand.** The `github`/`reddit`/`youtube` doc files are intentional redirects that point at `axon ingest` (the auto-detecting unified command). The `export.md`, `graph.md`, and `refresh.md` files document features that have been removed entirely.

---

## Verified clean

These files are accurate to the code:

- `docs/commands/completions.md`
- `docs/commands/dedupe.md`
- `docs/commands/debug.md`
- `docs/commands/doctor.md`
- `docs/commands/github.md` (redirect-only, accurate)
- `docs/commands/map.md`
- `docs/commands/migrate.md`
- `docs/commands/research.md`
- `docs/commands/retrieve.md`
- `docs/commands/scrape.md`
- `docs/commands/screenshot.md`
- `docs/commands/search.md` (see Cross-cutting #2 below — doc is right; project root CLAUDE.md disagrees but is out of scope)
- `docs/commands/sessions.md`
- `docs/commands/extract.md`
- `docs/commands/ingest.md`
- `docs/commands/sources.md` (after PG/Redis/AMQP fix)
- `docs/commands/domains.md` (after PG/Redis/AMQP fix)
- `docs/ingest/ingest.md`
- `docs/ingest/github.md`
- `docs/ingest/reddit.md`
- `docs/ingest/sessions.md`
- `docs/ingest/youtube.md`

---

## Files needing full rewrite or deletion

| File | Action | Reason |
|------|--------|--------|
| `docs/commands/export.md` | **Delete** | `Export` not in `CommandKind` enum; no CLI parser; `crates/cli/commands/` has no `export.rs`. Top-level `docs/EXPORT.md` may still document a backup design but the CLI subcommand does not exist. |
| `docs/commands/graph.md` | **Delete** | `Graph` not in `CommandKind` enum; no `axon graph` subcommand exists. The `crates/cli/CLAUDE.md` file mentions it in a stale dispatch example, but the actual `lib.rs::run_once` match has no `Graph` arm and the parser has no `Graph` variant. Top-level `docs/GRAPH.md` may document an aspirational design but this CLI doc points at non-existent commands. |
| `docs/commands/refresh.md` | **Delete** | `Refresh` not in `CommandKind` enum; no `axon refresh` parser. The `Cargo.toml`/CLI dispatch has no refresh route. `axon watch` is the live scheduler surface (per `crates/core/config/cli.rs::WatchSubcommand`). |
| `docs/commands/serve.md` | **Full rewrite** | Currently describes a fictional supervisor that manages Next.js dev server, shell-server, and full-mode workers. The actual implementation (`crates/cli/commands/serve.rs:5-8`) is a one-liner that calls `crates::mcp::run_unified_server(...)` on `mcp_http_host:mcp_http_port` (default `127.0.0.1:8001`). All claims about port 49000, AXON_SERVE_PORT, /ws bridges, /download endpoints, container preflight, and process supervision are not in the code. **Flag-only: doc requires a full rewrite, not patches.** |

> Per the audit policy, these files are **flagged for deletion** and not removed unilaterally. Operator follow-up required.

---

## Cross-cutting findings (applied across multiple files)

### Cross-cutting #1: Phantom required env vars (`AXON_PG_URL`, `AXON_REDIS_URL`, `AXON_AMQP_URL`)
**Stale claim:** Eight command docs listed these three env vars as "Required by global config parsing (all commands)."
**Reality:** Lite mode is the default operating mode (`crates/core/config/cli/global_args.rs:155` — `--lite` flag with `default_value_t = false` but project CLAUDE.md confirms lite is the default behavior; jobs are stored in SQLite and workers run in-process). None of these three env vars are required for any documented command in lite mode. The actual config parser (`crates/core/config/parse/build_config.rs`) does not error on missing PG/Redis/AMQP.
**Fix:** Removed the rows from the required-env tables and added a note that the command runs in lite mode by default.
**Applied:** yes — `ask.md`, `embed.md`, `query.md`, `sources.md`, `domains.md`, `stats.md`, `suggest.md`, `evaluate.md`.

### Cross-cutting #2: Search auto-queues crawl jobs (project CLAUDE.md vs reality)
**Stale claim:** Project root `CLAUDE.md` line ~38 says `search` "auto-queues crawl jobs for results."
**Reality:** `crates/cli/commands/search.rs` contains no `enqueue` or `queue` calls. `docs/commands/search.md` correctly notes "search does not enqueue crawl jobs."
**Fix:** None — the doc is correct. The project CLAUDE.md is the stale party but is out of scope.
**Applied:** no — flagging discrepancy; project CLAUDE.md needs separate update by an editor with that scope.

---

## Per-file findings

### docs/commands/README.md

#### [README.md:15-23] Index lists removed commands
**Stale claim:** Lists `export`, `graph`, `refresh` as core commands and `github`, `reddit`, `youtube` as separate ingest source commands.
**Reality:** `Export`, `Graph`, `Refresh` are not in `CommandKind`. `github`/`reddit`/`youtube` doc files are migration redirects. `sessions.md` was missing from the Core list despite `Sessions` being a real subcommand (`enums.rs:26`, `cli.rs:67`).
**Fix:** Removed dead links to export/graph/refresh; renamed the "Ingest Source Commands" subsection to "Ingest Source Redirects" with explicit redirect notes; added `sessions` to the Core list.
**Applied:** yes — `docs/commands/README.md`.

---

### docs/commands/ask.md

#### [ask.md:24-32] Phantom required env vars
See Cross-cutting #1. **Applied:** yes.

#### [ask.md:72] Wrong default for `AXON_ASK_CANDIDATE_LIMIT`
**Stale claim:** Doc says default is `64` (in two places: pipeline step 2 and the tuning table).
**Reality:** `crates/core/config/types/config_impls.rs:97` sets `ask_candidate_limit: 150`. The doc-comment in `config/types/config.rs:267` is also wrong (says "Default: 64") — that's a code comment but matches the wrong value the docs propagated.
**Fix:** Updated both occurrences in `ask.md` to `150`.
**Applied:** yes — `ask.md` lines for pipeline step 2 and tuning table.

> **Side note (out of doc-audit scope):** `crates/core/config/types/config.rs:267` doc-comment claims `Default: 64`. The actual default in `config_impls.rs:97` is `150`. Recommend a follow-up code edit to fix the doc comment.

#### [ask.md:43] `--graph` flag claim
**Stale claim:** Documents `--graph` flag as "`false` Enable graph-enhanced retrieval via Neo4j (requires `AXON_NEO4J_URL`)."
**Reality:** The flag exists in `crates/core/config/cli/global_args.rs:170-171` with `ArgAction::SetTrue`, but the corresponding command code path (graph/Neo4j) is not wired into the lite-mode `ask` retrieval. Based on the `crates/mcp/CLAUDE.md` notes and the absence of a Graph CommandKind, graph features were removed in the lite-mode simplification. The flag remains parseable but is effectively a no-op.
**Fix:** Not applied — the flag is still a valid parse target, so the doc is technically not lying about its existence. Recommend a follow-up that either re-wires graph or removes the flag and the doc row in the same change.
**Applied:** no — flag-only; needs code-side decision.

---

### docs/commands/crawl.md

#### [crawl.md:52-58] Lite-mode paragraph spliced inside the flag table
**Stale claim:** Lines 52–58 mix a flag-table row, a free-form paragraph about lite-mode `--wait false` semantics, and then continue the flag table at line 58.
**Reality:** Markdown rendering breaks the table. The text is correct, just misplaced.
**Fix:** Not applied — formatting cleanup only; flagged for editor pass.
**Applied:** no — minor formatting; keeps the text correct.

The `audit` and `diff` subcommands listed at line 37-38 **are real** (verified at `crates/cli/commands/crawl/subcommands.rs:64-71`), so that part is accurate.

---

### docs/commands/embed.md

#### [embed.md:21-27] Phantom required env vars
See Cross-cutting #1. **Applied:** yes.

---

### docs/commands/evaluate.md

#### [evaluate.md:27-35] Phantom required env vars
See Cross-cutting #1. **Applied:** yes.

#### [evaluate.md:39-46] Missing flags + wrong "always JSON" claim
**Stale claim:** Flag table only lists `--query` and `--collection`. Note says "Output is always JSON" and the examples show no human output.
**Reality:** `crates/core/config/cli.rs:250-261` defines `EvaluateArgs` with three additional flags:
- `--diagnostics` (`SetTrue`) — print retrieval diagnostics
- `--responses-mode` enum: `inline`, `side-by-side` (default), `events`
- `--retrieval-ab` (`SetTrue`) — replace baseline with hybrid-disabled RAG
And `crates/cli/commands/evaluate.rs:18-49` only emits JSON when `cfg.json_output` is true; otherwise it calls `print_evaluate_output()` which renders side-by-side or inline human text.
**Fix:** Added the three missing flags to the flag table; replaced "always JSON" with corrected `--json` semantics; updated examples and notes section.
**Applied:** yes — `evaluate.md`.

---

### docs/commands/query.md

#### [query.md:24-30] Phantom required env vars
See Cross-cutting #1. **Applied:** yes.

---

### docs/commands/sources.md

#### [sources.md:21-26] Phantom required env vars
See Cross-cutting #1. **Applied:** yes.

---

### docs/commands/domains.md

#### [domains.md:23-28] Phantom required env vars
See Cross-cutting #1. **Applied:** yes.

---

### docs/commands/stats.md

#### [stats.md:7] "Postgres-derived job/command metrics" claim
**Stale claim:** "Combines Qdrant collection snapshots with Postgres-derived job/command metrics."
**Reality:** Lite mode (default) reads job/command metrics from the local SQLite jobs database. Postgres is not part of the default stack — see `crates/jobs/CLAUDE.md` and `crates/core/config/types/config.rs::sqlite_path`.
**Fix:** Replaced "Postgres-derived" with "derived from the local SQLite jobs database"; updated the trailing notes section accordingly.
**Applied:** yes — `stats.md`.

#### [stats.md:21-26] Phantom required env vars
See Cross-cutting #1. **Applied:** yes.

---

### docs/commands/status.md

#### [status.md:4, 26-45, 64] References non-existent `refresh` and `graph` queues
**Stale claim:** Doc says status reports across "crawl, extract, embed, ingest, refresh, and graph queues"; lists "Refresh" and "Graph" sections in human output; JSON shape lists `local_refresh_jobs` and `local_graph_jobs`; notes section says "`--active` and `--recent` apply to graph jobs as well as other job families."
**Reality:** `crates/services/system.rs:389-408::build_status_payload` produces only four job arrays (`local_crawl_jobs`, `local_extract_jobs`, `local_embed_jobs`, `local_ingest_jobs`) plus a `totals` object with the same four families. There are no refresh or graph entries. The MCP HTML dashboard asset (`crates/mcp/assets/status_dashboard.html:218-219`) still references those keys but the API never returns them, so the dashboard rows render empty.
**Fix:** Updated synopsis, section list, and JSON shape; removed the graph reference from the notes section. Added the `totals` object (which the doc was missing entirely).
**Applied:** yes — `status.md`.

> **Side note:** `crates/mcp/assets/status_dashboard.html` still has dead `local_refresh_jobs`/`local_graph_jobs` keys. Out of scope for this doc audit but worth filing.

---

### docs/commands/suggest.md

#### [suggest.md:24-31] Phantom required env vars + wrong LLM env names
**Stale claim:** Lists `AXON_PG_URL`, `AXON_REDIS_URL`, `AXON_AMQP_URL` as required; lists `OPENAI_BASE_URL` and `OPENAI_MODEL` as the LLM endpoint.
**Reality:** Per project CLAUDE.md "ACP-backed completion path" section: `suggest` runs through ACP (`AXON_ACP_ADAPTER_CMD`), not directly to OPENAI_BASE_URL. `OPENAI_MODEL` is the model override knob for ACP-backed calls. PG/Redis/AMQP are not required.
**Fix:** Removed the three phantom envs; replaced the OPENAI_BASE_URL row with `AXON_ACP_ADAPTER_CMD`; kept `OPENAI_MODEL` (still the model knob).
**Applied:** yes — `suggest.md`.

---

### docs/commands/watch.md

#### [watch.md:84-86] References removed `axon refresh schedule`
**Stale claim:** "`axon refresh schedule ...` remains available as a compatibility interface and is backed by watch definitions with `task_type=refresh`."
**Reality:** `Refresh` is not in `CommandKind`. `axon refresh` and any `refresh schedule` subcommand are gone. `axon watch` is the only scheduler surface.
**Fix:** Replaced the section with a note that the legacy compatibility surface has been removed.
**Applied:** yes — `watch.md`.

---

### docs/commands/mcp.md

#### [mcp.md:46] Lists `refresh` as supported MCP action
**Stale claim:** "Supported top-level action families include: ..., `refresh`, ..."
**Reality:** `crates/mcp/schema.rs:5-29::AxonRequest` has no `Refresh` variant. Per `crates/mcp/CLAUDE.md`: "(`graph`, `refresh`, and `export` actions were removed in the lite-mode simplification — see commit 05da3b44.)" The actual variants are: `Status, Crawl, Extract, Embed, Ingest, Query, Retrieve, Search, Map, Doctor, Domains, Sources, Stats, Help, Artifacts, Scrape, Research, Ask, Screenshot, ElicitDemo, Acp`.
**Fix:** Removed `refresh` from the action list; added the previously-undocumented `acp` and `elicit_demo` actions.
**Applied:** yes — `mcp.md`.

---

### docs/commands/serve.md

#### [serve.md:1-71] Documents fictional supervisor
**Stale claim:** Describes `axon serve` as managing a Rust WebSocket bridge on port 49000, an MCP HTTP server on 8001, an `apps/web` Next.js dev server on 49010, a `shell-server.mjs` on 49011, full-mode workers, container preflight checks, and a list of `/ws`, `/output`, `/download/{job_id}/...` HTTP routes.
**Reality:** `crates/cli/commands/serve.rs` (entire file is 8 lines):
```rust
pub async fn run_serve(cfg: &Config) -> Result<(), Box<dyn Error>> {
    acp_llm::init_warm_pool(cfg);
    crate::crates::mcp::run_unified_server(cfg.clone(), &cfg.mcp_http_host, cfg.mcp_http_port).await
}
```
The actual command warms the ACP pool and runs the MCP HTTP server on `mcp_http_host:mcp_http_port` (default `127.0.0.1:8001`). There is no port 49000, no AXON_SERVE_PORT, no shell server, no /ws bridge, no /download routes, no Next.js dev server, no full-mode workers, no Docker preflight.
**Fix:** Not applied — too much rewriting required, and it's unclear whether the documented behavior is the design intent (with the implementation lagging) or whether the spec was abandoned.
**Applied:** no — flagged for full rewrite.

---

## Per-file findings — files flagged for deletion

### docs/commands/export.md

#### [export.md:1-44] Documents non-existent `axon export` command
**Stale claim:** Documents `axon export`, `axon export verify`, `--include-history`, `--output`, `--json` flags, and a backup-manifest contract.
**Reality:** No `Export` variant in `CommandKind` (`crates/core/config/types/enums.rs`). No `ExportArgs` in `cli.rs`. No `run_export` handler. No file in `crates/cli/commands/` named `export.rs`. The top-level `docs/EXPORT.md` may describe an aspirational backup design, but the CLI subcommand does not exist.
**Fix:** **Delete the file** (not applied unilaterally per audit policy).
**Applied:** no — flagged for deletion.

---

### docs/commands/graph.md

#### [graph.md:1-22] Documents non-existent `axon graph` command
**Stale claim:** Documents `axon graph build`, `status`, `explore`, `stats`, `worker` subcommands.
**Reality:** No `Graph` variant in `CommandKind`. No graph routing in `lib.rs::run_once`. The `crates/cli/CLAUDE.md` example dispatch in its "Dispatch" section incorrectly shows `CommandKind::Graph => run_graph(cfg).await?`, but the live code does not contain that arm. Per `crates/mcp/CLAUDE.md`, graph actions were removed in the lite-mode simplification.
**Fix:** **Delete the file** (not applied unilaterally per audit policy).
**Applied:** no — flagged for deletion.

---

### docs/commands/refresh.md

#### [refresh.md:1-121] Documents non-existent `axon refresh` command
**Stale claim:** Documents `axon refresh <url>`, `axon refresh schedule add|list|enable|disable|delete|worker|run-due`, tier presets, github-repo schedules.
**Reality:** No `Refresh` variant in `CommandKind`. No `RefreshArgs` in `cli.rs`. No `run_refresh` handler. Per project CLAUDE.md and `crates/mcp/CLAUDE.md`, refresh was removed in the lite-mode simplification; `axon watch` (with `task_type=refresh`) is the supported scheduler surface.
**Fix:** **Delete the file** (not applied unilaterally per audit policy).
**Applied:** no — flagged for deletion.

---

## Summary

**Total files audited:** 38 (33 command + 5 ingest)

**Verified clean:** 22

**Findings by severity:**

| Severity | Count |
|----------|-------|
| Critical (file documents non-existent command, or core flag/default wrong) | 6 |
| Major (wrong required env vars; wrong JSON shape; wrong default value) | 9 |
| Minor (formatting; outdated cross-reference; small wording) | 4 |
| **Total** | **19** |

**Critical findings:**
1. `export.md` — non-existent command (delete)
2. `graph.md` — non-existent command (delete)
3. `refresh.md` — non-existent command (delete)
4. `serve.md` — fictional implementation (full rewrite needed)
5. `status.md` — JSON shape includes non-existent `local_refresh_jobs`/`local_graph_jobs`
6. `ask.md` — `AXON_ASK_CANDIDATE_LIMIT` default off by 86 (64 vs 150)

**Major findings:**
- Cross-cutting #1: PG/Redis/AMQP listed as required across 8 doc files (lite mode is default)
- `evaluate.md` — three missing flags + "always JSON" wrong
- `mcp.md` — lists `refresh` as a supported action (removed)

**Minor findings:**
- `crawl.md` — flag table interrupted by paragraph (formatting)
- `watch.md` — references removed `axon refresh schedule` (fixed)
- `README.md` — drops dead links and reorganizes ingest section (fixed)
- `ask.md` — `--graph` flag is parseable but no-op in lite mode (flag remains; recommend follow-up)

**Total fixes applied:** 16 distinct edits across 11 files:
- `ask.md` (3 edits: candidate_limit ×2, env table)
- `embed.md` (1 edit: env table)
- `query.md` (1 edit: env table)
- `sources.md` (1 edit: env table)
- `domains.md` (1 edit: env table)
- `stats.md` (3 edits: synopsis, env table, notes)
- `suggest.md` (1 edit: env table)
- `evaluate.md` (3 edits: env table, flags table, output/notes)
- `status.md` (3 edits: synopsis, sections+JSON shape, notes)
- `mcp.md` (1 edit: action families)
- `watch.md` (1 edit: refresh schedule note)
- `README.md` (1 edit: index reorganization)

**Files needing operator action:**
- 3 deletions: `export.md`, `graph.md`, `refresh.md`
- 1 full rewrite: `serve.md`

**Out-of-scope follow-ups (filed as side notes, not fixed):**
- `crates/core/config/types/config.rs:267` — doc comment `Default: 64` is wrong; actual default 150
- `crates/mcp/assets/status_dashboard.html:218-219` — references dead `local_refresh_jobs`/`local_graph_jobs` keys
- Project root `CLAUDE.md` line ~38 — claims `search` "auto-queues crawl jobs" (it does not)
- `crates/cli/CLAUDE.md` Dispatch section example shows non-existent `CommandKind::Graph` arm

**Report path:** `/home/jmagar/workspace/axon_rust/docs/reports/2026-05-06-stale-docs-audit/B-commands.md`
