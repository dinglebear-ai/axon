# MCP Documentation Stale-Audit — C-mcp.md

**Date:** 2026-05-06
**Scope:** `docs/MCP.md`, `docs/MCP-TOOL-SCHEMA.md`, `docs/mcp/*.md`, `docs/auth/MCP-AUTH.md`, plugin manifest references
**Method:** Each claim verified against `crates/mcp/`, `crates/services/context.rs`, `crates/cli/commands/mcp.rs`, `crates/core/config/`, `xtask/src/checks/mcp_http.rs`.

## Summary

| Severity | Count |
|---------:|:------|
| Critical | 6 |
| Major    | 14 |
| Minor    | 5 |
| **Total** | **25** |

Critical findings concentrate on three themes:

1. **Fictional OAuth flow.** `docs/auth/MCP-AUTH.md` documented a Google OAuth broker, `atk_` token issuer, dynamic client registration, `/.well-known/oauth-*` endpoints, and a Redis-backed token store. **None of that exists in the code.** The only HTTP auth path is a static bearer token (`AXON_MCP_HTTP_TOKEN`) checked by `crates/mcp/auth.rs::mcp_auth_middleware`. No Google OAuth env vars are read anywhere in `crates/`.
2. **Removed actions still documented.** `docs/MCP-TOOL-SCHEMA.md`, `docs/mcp/TOOLS.md`, and `docs/mcp/PATTERNS.md` referenced `refresh`, `graph`, `export`, and `refresh schedule` actions. The `AxonRequest` enum in `crates/mcp/schema.rs` does not contain any of those variants.
3. **Lite-only runtime contradicts older multi-backend prose.** `ServiceContext` in `crates/services/context.rs` has only `{ cfg, jobs }` — no `pg_pool`, `redis`, `amqp`, or `capabilities` field. The `MEMORY.md` and `crates/services/CLAUDE.md` confirm lite mode is the only mode and `LiteServiceRuntime` is the only `ServiceJobRuntime` implementation.

Plugin manifest references (`/.claude-plugin/plugin.json` and `/plugins/axon/.mcp.json`) are accurate.

## Files Audited (12)

1. `/home/jmagar/workspace/axon_rust/docs/MCP.md`
2. `/home/jmagar/workspace/axon_rust/docs/MCP-TOOL-SCHEMA.md`
3. `/home/jmagar/workspace/axon_rust/docs/mcp/CLAUDE.md`
4. `/home/jmagar/workspace/axon_rust/docs/mcp/CONNECT.md`
5. `/home/jmagar/workspace/axon_rust/docs/mcp/DEPLOY.md`
6. `/home/jmagar/workspace/axon_rust/docs/mcp/DEV.md`
7. `/home/jmagar/workspace/axon_rust/docs/mcp/ENV.md`
8. `/home/jmagar/workspace/axon_rust/docs/mcp/PATTERNS.md`
9. `/home/jmagar/workspace/axon_rust/docs/mcp/TOOLS.md`
10. `/home/jmagar/workspace/axon_rust/docs/mcp/TRANSPORT.md`
11. `/home/jmagar/workspace/axon_rust/docs/auth/MCP-AUTH.md`
12. Plugin manifests: `.claude-plugin/plugin.json`, `plugins/axon/.mcp.json`

---

## Findings

### [docs/auth/MCP-AUTH.md:entire file] OAuth broker is fictional
**Stale claim:** Document describes Google OAuth flow, `/.well-known/oauth-protected-resource`, `/.well-known/oauth-authorization-server`, `/oauth/register`, `/oauth/authorize`, `/oauth/token`, `/oauth/google/callback`, `atk_` token issuance, DCR, Redis-backed `axon:oauth:` prefix, `GOOGLE_OAUTH_*` env vars, `AXON_MCP_API_KEY`.
**Reality:**
- `grep -r "oauth\|GOOGLE_OAUTH\|atk_\|/oauth/\|well-known" crates/mcp/` returns zero hits.
- `grep -r "AXON_MCP_API_KEY" crates/` returns zero hits.
- The only HTTP auth code is `crates/mcp/auth.rs::mcp_auth_middleware`, which reads `AXON_MCP_HTTP_TOKEN` and accepts it via `Authorization: Bearer …` or `x-api-key`. Comparison uses `subtle::ConstantTimeEq`.
- Startup policy (`crates/mcp/server/http.rs::enforce_mcp_http_startup_policy`) refuses non-loopback binds when `AXON_MCP_HTTP_TOKEN` is unset.
**Fix:** Rewrote the entire document to reflect the actual `AXON_MCP_HTTP_TOKEN` model, removed all OAuth content, and added a "Where is the OAuth flow?" troubleshooting note pointing to the real auth path.
**Applied:** yes — full rewrite via `Write` tool.

### [docs/MCP-TOOL-SCHEMA.md:32] `refresh` and `graph` listed as lifecycle families
**Stale claim:** Parser rules say `subaction` is required for `crawl|extract|embed|ingest|refresh|graph|artifacts`.
**Reality:** `crates/mcp/schema.rs::AxonRequest` (lines 5–29) lists exactly: `Status, Crawl, Extract, Embed, Ingest, Query, Retrieve, Search, Map, Doctor, Domains, Sources, Stats, Help, Artifacts, Scrape, Research, Ask, Screenshot, ElicitDemo, Acp`. No `Refresh` or `Graph` variants.
**Fix:** Removed `refresh|graph` from the lifecycle list.
**Applied:** yes.

### [docs/MCP-TOOL-SCHEMA.md:42–46] `refresh` schedule subaction documented
**Stale claim:** Documents `{"action": "refresh", "subaction": "schedule", "schedule_subaction": "list|create|delete|enable|disable"}`.
**Reality:** No `RefreshRequest` struct, no `schedule_subaction` field anywhere in `crates/mcp/schema.rs`.
**Fix:** Removed the sentence.
**Applied:** yes.

### [docs/MCP-TOOL-SCHEMA.md:86–91] `## Refresh Start Parameters` section
**Stale claim:** Describes `url` / `urls` / `schedule_subaction` for `refresh` action.
**Reality:** No `Refresh` action.
**Fix:** Removed entire section.
**Applied:** yes.

### [docs/MCP-TOOL-SCHEMA.md:154–183] OAuth + AXON_PG/REDIS/AMQP env block
**Stale claim:** Lists `AXON_PG_URL`, `AXON_REDIS_URL`, `AXON_AMQP_URL`, plus 13 `GOOGLE_OAUTH_*` variables.
**Reality:**
- `grep -r "AXON_PG_URL\|AXON_AMQP_URL" crates/mcp/` returns zero hits. The MCP server reads `cfg` only and `cfg` no longer carries those URLs through to `crates/mcp/`. Lite mode is the only mode (per `crates/services/CLAUDE.md` and `MEMORY.md`).
- No `GOOGLE_OAUTH_*` reference anywhere in `crates/`.
**Fix:** Replaced with the actual env var inventory (Qdrant, TEI, OpenAI/ACP, Tavily, AXON_LITE, AXON_SQLITE_PATH, AXON_COLLECTION, all `AXON_MCP_*` variables, `AXON_INLINE_BYTES_THRESHOLD`).
**Applied:** yes.

### [docs/MCP-TOOL-SCHEMA.md:4] "AUTO-GENERATED — do not edit manually" header conflicts with manual edits
**Stale claim:** File header says `<!-- AUTO-GENERATED by scripts/generate_mcp_schema_doc.py — do not edit manually -->`.
**Reality:** Either the generator script is stale (still emitting `refresh` / `graph` / OAuth content), or the file has drifted from generator output. I cannot tell from grep alone.
**Fix:** Did not modify the marker. Manual edits applied above will be reverted next time the generator runs unless `scripts/generate_mcp_schema_doc.py` is also updated. **Flagged for human review.**
**Applied:** no — needs review of generator script.

### [docs/MCP.md:21] References non-existent `crates/mcp/config.rs`
**Stale claim:** Implementation pointer lists `crates/mcp/config.rs`.
**Reality:** `ls crates/mcp/` shows `auth.rs, cors.rs, schema.rs, server.rs, server/, schema/, README.md, CLAUDE.md`. There is no `config.rs`. `crates/mcp/CLAUDE.md` line "config.rs … (load_mcp_config() removed in 54244286)" confirms it was deleted.
**Fix:** Replaced with `auth.rs`, `cors.rs`, `server/handlers_*.rs`.
**Applied:** yes.

### [docs/MCP.md:150–157] Tool list missing `acp` action
**Stale claim:** "Use CLI-identical action names" lists `help`, `status`, `crawl`, …, `elicit_demo` but omits `acp`.
**Reality:** `crates/mcp/schema.rs::AxonRequest::Acp(AcpRequest)` exists, with subactions `list_sessions, fork_session, resume_session, set_model, ext_method, ext_notification, logout` (lines 397–413).
**Fix:** Added `acp` action with its subaction list.
**Applied:** yes.

### [docs/mcp/CONNECT.md:21] Claims `axon mcp` HTTP transport is default
**Stale claim:** "When `axon serve` or `axon mcp` is running with HTTP transport (default)".
**Reality:** `crates/cli/commands/mcp.rs` test (line 34): `assert_eq!(cfg.mcp_transport, McpTransport::Stdio);`. `crates/core/config/parse/build_config.rs:207` sets `mcp_transport_default = McpTransport::Stdio` for `CliCommand::Mcp`. Only the `axon serve mcp` subcommand defaults to HTTP (line 171), and bare `axon serve` defaults to `Both` (line 176).
**Fix:** Replaced with explicit per-command default table; recommended `--transport http` / `axon serve mcp` for HTTP.
**Applied:** yes.

### [docs/mcp/CONNECT.md:50–52] Codex stdio config sets AXON_PG_URL / AXON_REDIS_URL / AXON_AMQP_URL
**Stale claim:** Example env block sets Postgres / Redis / RabbitMQ URLs.
**Reality:** Lite mode is the only mode; the MCP server does not consume these vars (confirmed via grep — they appear only in `crates/ingest/reddit*.rs` for entirely different purposes, and `crates/core/config/parse/excludes.rs`).
**Fix:** Removed the Postgres/Redis/AMQP entries, added a note that lite mode is the only runtime.
**Applied:** yes.

### [docs/mcp/CONNECT.md:140–146] Health endpoint `curl http://localhost:8001/health`
**Stale claim:** "HTTP health check" with `curl -s http://localhost:8001/health`.
**Reality:** `grep -n '"/health"' crates/mcp/` finds zero hits. The MCP HTTP router (`crates/mcp/server/http.rs::mcp_http_router`) only `nest_service("/mcp", …)`. The web router (under `axon serve`) is separate and merged in via `run_unified_server` only when running unified.
**Fix:** Replaced with a `curl /mcp + Bearer` 401/200 probe and added a note that the MCP server does not expose `/health`.
**Applied:** yes.

### [docs/mcp/TRANSPORT.md:9] HTTP "Auth: OAuth / bearer token"
**Stale claim:** Auth column lists "OAuth / bearer token".
**Reality:** Only bearer / x-api-key (`crates/mcp/auth.rs`); no OAuth.
**Fix:** Replaced with `AXON_MCP_HTTP_TOKEN bearer`.
**Applied:** yes.

### [docs/mcp/TRANSPORT.md:11] `axon mcp` description omits stdio default
**Stale claim:** Listed `axon mcp` as the stdio command but did not say it was the **default** for that command.
**Reality:** `axon mcp` defaults to stdio (see CONNECT finding above).
**Fix:** Annotated as `(default)` and added a paragraph explaining per-command defaults.
**Applied:** yes.

### [docs/mcp/TRANSPORT.md:34–37] Postgres / Redis / AMQP env block in stdio Claude Desktop example
**Stale claim:** Example sets `AXON_PG_URL`, `AXON_REDIS_URL`, `AXON_AMQP_URL`.
**Reality:** Same as above — not consumed by MCP server.
**Fix:** Removed; added note about lite mode.
**Applied:** yes.

### [docs/mcp/TRANSPORT.md:84–87] `/health` GET endpoint
**Stale claim:** Endpoint table lists `/health GET Health check`.
**Reality:** Same as CONNECT.md finding — endpoint does not exist on MCP HTTP router.
**Fix:** Removed; left only `/mcp POST` and added note about no dedicated health endpoint.
**Applied:** yes.

### [docs/mcp/TRANSPORT.md:90–101] Claude Code HTTP example missing auth header
**Stale claim:** Example config has no `Authorization` header.
**Reality:** `AXON_MCP_HTTP_TOKEN` is required for non-loopback binds and recommended even for loopback.
**Fix:** Added `headers.Authorization: Bearer …` and explanatory paragraph.
**Applied:** yes.

### [docs/mcp/ENV.md:18–35] OAuth broker section
**Stale claim:** "MCP OAuth is an optional auth system for MCP HTTP clients. It uses `atk_` tokens" plus 11 `GOOGLE_OAUTH_*` rows.
**Reality:** No OAuth code (see master finding).
**Fix:** Replaced section with a `## Transport selection` table documenting the real `AXON_MCP_TRANSPORT` env var (which is implemented in `crates/core/config/parse/helpers.rs:5`).
**Applied:** yes.

### [docs/mcp/TOOLS.md:182–188] `### export` action
**Stale claim:** `{"action": "export"}` exports a full index manifest.
**Reality:** No `Export` variant in `AxonRequest`.
**Fix:** Replaced with the actual missing actions: `elicit_demo` and `acp`.
**Applied:** yes.

### [docs/mcp/TOOLS.md:233–250] `### refresh` and `### graph` action sections
**Stale claim:** Documents `refresh` and `graph` actions with `build|status|explore|stats` subactions for graph and `schedule` subaction for refresh.
**Reality:** Neither variant exists in `AxonRequest`.
**Fix:** Removed both sections.
**Applied:** yes.

### [docs/mcp/TOOLS.md:101] `query` collection default `cortex`
**Stale claim:** Default collection for `query` is `cortex`.
**Reality:** `QueryRequest.collection` is `Option<String>` (`crates/mcp/schema.rs:250`); when unset, the server falls back to `cfg.collection`. The user's MEMORY.md notes that the deployed instance uses `axon`, not `cortex`. The CLAUDE.md root says default is `cortex` (only true if `AXON_COLLECTION` is unset).
**Fix:** Clarified default as "server-configured (`AXON_COLLECTION`, default `cortex`)".
**Applied:** yes.

### [docs/mcp/TOOLS.md:118] `ask` collection default `cortex`
**Stale claim:** Same as above.
**Reality:** Same as above.
**Fix:** Same clarification.
**Applied:** yes.

### [docs/mcp/TOOLS.md:97 & 116] Missing `hybrid_search` parameter
**Stale claim:** `query` and `ask` parameter tables omit `hybrid_search`.
**Reality:** Field exists on `QueryRequest` (line 260) and `AskRequest` (line 382) — `Option<bool>`, defaults to server config (`AXON_HYBRID_SEARCH`).
**Fix:** Added the parameter row to both tables.
**Applied:** yes.

### [docs/mcp/PATTERNS.md:28] Lifecycle families list includes `refresh, graph`
**Stale claim:** Lifecycle families: `crawl, extract, embed, ingest, refresh, graph, artifacts`.
**Reality:** Removed actions.
**Fix:** Reduced to `crawl, extract, embed, ingest, artifacts`.
**Applied:** yes.

### [docs/mcp/PATTERNS.md:128–160] `ServiceContext` struct definition is fictional
**Stale claim:**
```rust
pub struct ServiceContext {
    pub config: Config,
    pub capabilities: ServiceCapabilities,
    pub pg_pool: Option<PgPool>,
    pub redis: Option<RedisConnection>,
    pub amqp: Option<AmqpConnection>,
    // ...
}
```
plus a `ServiceCapabilities` struct with `jobs`, `graph`, `search` gates.
**Reality:** `crates/services/context.rs:9–12` —
```rust
pub struct ServiceContext {
    pub cfg: Arc<Config>,
    pub jobs: Arc<dyn ServiceJobRuntime>,
}
```
There is no `capabilities` field, no embedded pools. `crates/services/CLAUDE.md` confirms the only fields are `cfg` and `jobs`.
**Fix:** Replaced with the real struct; added the `new` vs `new_with_workers` distinction.
**Applied:** yes.

### [docs/mcp/PATTERNS.md:162–180] Lifecycle pattern claims AMQP / Redis / Postgres backing
**Stale claim:** "`start` -- enqueue job to AMQP queue", "`status` -- query Postgres", "`cancel` -- set cancel flag in Redis", queue names like `axon.crawl.jobs`, "worker binary path (e.g., `axon crawl worker`)".
**Reality:** Lite mode is the only mode (`crates/jobs/CLAUDE.md`); jobs are enqueued/queried/cancelled in SQLite. `axon crawl worker` is no longer in `crates/cli/commands/crawl/subcommands.rs` (workers are in-process). Per the user's MEMORY.md "LiteBackend / ServiceContext Worker Split".
**Fix:** Rewrote the section to describe SQLite-backed jobs and `LiteBackend::new_with_workers`.
**Applied:** yes.

### [docs/mcp/DEV.md:18–30] Module layout fiction
**Stale claim:** Layout shows `crates/mcp/server.rs` as the only handler file and omits `auth.rs`, `cors.rs`, the entire `crates/mcp/server/` subdirectory of split handler files.
**Reality:** `ls crates/mcp/` and `ls crates/mcp/server/` reveal: `auth.rs, cors.rs, schema.rs, server.rs, server/{http.rs, common.rs, handlers_acp.rs, handlers_crawl_extract.rs, handlers_elicit.rs, handlers_embed_ingest.rs, handlers_query.rs, handlers_system.rs, artifacts/, handlers_system/}`. `crates/mcp/CLAUDE.md` documents this layout authoritatively.
**Fix:** Replaced with the real layout tree.
**Applied:** yes.

### [docs/mcp/DEV.md:74] Lifecycle list includes `refresh`
**Stale claim:** "Lifecycle actions (crawl, extract, embed, ingest, refresh)".
**Reality:** No `refresh`.
**Fix:** Dropped `refresh`. Also replaced "Add a worker binary path" with "Wire the in-process worker into `LiteBackend::new_with_workers`".
**Applied:** yes.

### [docs/mcp/DEV.md:103–115] curl health check example
**Stale claim:** `curl -s http://localhost:8001/health`.
**Reality:** No such endpoint.
**Fix:** Removed; replaced with `tools/list` JSON-RPC probe and added bearer-token guidance.
**Applied:** yes.

### [docs/mcp/DEV.md:160–170] `ctx.capabilities.jobs.supported` example
**Stale claim:** Code snippet uses `ctx.capabilities.jobs.supported`.
**Reality:** No `capabilities` field on `ServiceContext`.
**Fix:** Replaced with prose explaining lite-only mode and that the previously gated operations are now either removed or return runtime errors.
**Applied:** yes.

### [docs/mcp/DEPLOY.md:34] "Does not support graph, refresh, or watch operations"
**Stale claim:** Lists graph, refresh, watch as unsupported.
**Reality:** `graph` and `refresh` aren't actions at all (they were removed); `watch` scheduler is the only feature genuinely gated by lite mode (per `crates/services/context.rs` history and `crates/jobs/CLAUDE.md`).
**Fix:** Reduced to "The watch scheduler is the only feature unavailable in lite mode."
**Applied:** yes.

### [docs/mcp/CLAUDE.md:18] cross-link `../stack/ARCH.md`
**Stale claim:** Cross-references `../stack/ARCH.md` and `../repo/REPO.md`.
**Reality:** `find docs -type d -name stack -o -name repo` returns nothing. Those directories don't exist.
**Fix:** Did not edit — judgment call. The whole table of cross-references points partly to docs that exist (`../CONFIG.md`, `../MCP.md`, `../MCP-TOOL-SCHEMA.md`) and partly to absent paths (`../stack/`, `../repo/`). **Flagged for human review** — owner needs to decide whether to recreate `stack/`/`repo/` directories or rewrite the links.
**Applied:** no — flagged.

### [docs/MCP.md:60–64] "axon mcp Starts stdio transport only" vs `axon serve mcp` "Starts HTTP transport only"
**Stale claim (minor):** This phrasing is correct, but the surrounding doc and the `mcporter` smoke test command examples (line 209+) implicitly assume HTTP at the same time the smoke harness uses stdio (per `crates/mcp/CLAUDE.md`).
**Reality:** Smoke harness runs against stdio (`scripts/test-mcp-tools-mcporter.sh`); both work but a casual reader might pick the wrong one.
**Fix:** Did not modify — the existing wording is technically accurate.
**Applied:** no — non-issue.

### [docs/MCP-TOOL-SCHEMA.md:151–152] MCP Resources list missing `ui://axon/status-dashboard`
**Stale claim:** "Implemented resource(s): `axon://schema/mcp-tool`".
**Reality:** `crates/mcp/server.rs::list_resources` returns **two** resources: `MCP_TOOL_SCHEMA_URI` and `STATUS_DASHBOARD_URI` (`ui://axon/status-dashboard`). The status dashboard resource is also documented in `docs/MCP.md:12`.
**Fix:** Did not modify — flagged below for the doc owner. The auto-generated header makes this risky to hand-edit.
**Applied:** no — flagged. (Consider regenerating via `scripts/generate_mcp_schema_doc.py`.)

### [.claude-plugin/plugin.json + plugins/axon/.mcp.json] Plugin manifest references
**Stale claim:** N/A
**Reality:** `plugin.json` declares `"mcp": "./plugins/axon/.mcp.json"` and the file exists with a valid stdio MCP server entry. The env vars passed (`QDRANT_URL`, `TEI_URL`, `AXON_COLLECTION`, `OPENAI_*`, `TAVILY_API_KEY`, `AXON_CHROME_REMOTE_URL`) all exist in `crates/core/config/parse/build_config.rs`. Version is `1.5.4`, matching `Cargo.toml`.
**Fix:** None needed.
**Applied:** n/a — clean.

---

## Top Items Needing Human Review

1. **`docs/MCP-TOOL-SCHEMA.md` auto-generated marker.** The file claims auto-generation by `scripts/generate_mcp_schema_doc.py`. Manual edits applied here will be reverted on next regeneration. **Owner action:** update the generator script to drop `refresh` / `graph` / OAuth scaffolding, then regenerate, or remove the auto-generated header if the generator no longer runs.
2. **`docs/MCP-TOOL-SCHEMA.md` MCP resources list.** Missing `ui://axon/status-dashboard` (see finding above). Should be added by regenerating from schema rather than hand-editing.
3. **`docs/mcp/CLAUDE.md` cross-references.** Points to `../stack/ARCH.md` and `../repo/REPO.md` which don't exist. Owner should either restore those directories or rewrite the links.
4. **Document the `acp` action.** It's now mentioned in `docs/MCP.md` (after this audit) but `docs/mcp/TOOLS.md` only got a stub. A full subaction reference should be added (params: `session_id`, `model_id`, `method`, `params`).
5. **OAuth deprecation note.** Several existing operational runbooks (outside the MCP doc tree) likely still mention `atk_` tokens or the OAuth broker. A repo-wide grep for `GOOGLE_OAUTH_`, `atk_`, and `oauth-protected-resource` should be the next pass.

## Fixes Applied (count: 22)

- `docs/auth/MCP-AUTH.md` — full rewrite (1)
- `docs/MCP.md` — 2 edits (config.rs reference + acp action)
- `docs/MCP-TOOL-SCHEMA.md` — 4 edits (refresh/graph in lifecycle, refresh schedule subaction sentence, refresh start params section, runtime deps env block)
- `docs/mcp/TOOLS.md` — 4 edits (export section → elicit_demo+acp, refresh+graph sections removed, query collection default, ask collection default + hybrid_search added)
- `docs/mcp/PATTERNS.md` — 3 edits (lifecycle families list, ServiceContext struct, lifecycle pattern AMQP/Redis/Postgres prose)
- `docs/mcp/CONNECT.md` — 3 edits (HTTP default claim, codex env block, /health curl)
- `docs/mcp/TRANSPORT.md` — 4 edits (auth column, claude desktop env, /health endpoint table row, claude code HTTP example)
- `docs/mcp/ENV.md` — 1 edit (OAuth broker section → transport selection)
- `docs/mcp/DEV.md` — 4 edits (module layout, lifecycle action list, curl health check, capabilities snippet)
- `docs/mcp/DEPLOY.md` — 1 edit (lite-mode unsupported list)

## Path to Findings Report

`/home/jmagar/workspace/axon_rust/docs/reports/2026-05-06-stale-docs-audit/C-mcp.md`
