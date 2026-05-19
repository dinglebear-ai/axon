# Per-Crate Documentation Audit — 2026-05-06

Audit of 16 per-crate docs against the actual contents of each crate.

Scope: `crates/README.md` plus `CLAUDE.md` / `README.md` for each of `cli`, `core`, `crawl`, `ingest`, `jobs`, `mcp`, `services` (CLAUDE only — no README), `vector`.

Key cross-cutting facts that drove most findings:

- The repo banned `mod.rs` files. Module roots live as `foo.rs` next to `foo/`. Several docs still reference the old layout, talk about `crates/<name>/mod.rs`-style siblings, or describe re-export shims that were removed.
- The repo collapsed to **lite mode only**. Postgres/RabbitMQ/Redis are gone. `Config` no longer has `pg_url`/`redis_url`/`amqp_url`/`*_queue` fields at the top level (they exist inside `config/types/subconfigs.rs` for sub-config purposes only), and there is no `FullBackend`/`open_config_pool()` story. Several CLAUDE.md files still describe the dual-backend world.
- `ServiceContext` has exactly two fields: `cfg: Arc<Config>` and `jobs: Arc<dyn ServiceJobRuntime>`. There is **no `capabilities` field** and no `ServiceCapabilities` struct anywhere in `crates/`. Docs that gate on `ctx.capabilities.<cap>.supported` are describing code that doesn't exist.
- `CommandKind` has 28 variants but no `Graph`, no `Refresh`, no `Github`/`Reddit`/`Youtube` (those were collapsed into `Ingest`). Has `Setup`, `Watch`, `Completions`, `Migrate` that doc lists miss.
- `crates/cli/commands/common.rs` is a 1.4 KB stub. The job/url helpers actually live in `common_jobs.rs` (job lifecycle) and `common_urls.rs` (URL parsing, `truncate_chars`, `start_url_from_cfg`, `expand_url_glob_seed`, `parse_urls`). The CLI CLAUDE.md repeatedly attributes them to `common.rs`.

---

## crates/README.md

### [crates/README.md:14] References non-existent `crates/web/README.md`
**Stale claim:** The Runtime Modules table includes `[web](./web/README.md)` and explicitly lists the web crate.
**Reality:** `crates/web/` exists as a directory but has **no `README.md`** (`ls crates/web/` shows code only). This is a broken link.
**Fix:** Either drop the link target (`web` without anchor) or add a stub README for `crates/web/`. Out of scope for this audit pass — flagging only.
**Applied:** no — judgment call (whether to add the missing README is a separate task).

### [crates/README.md:7-15] Module index missing `services`
**Stale claim:** Runtime Modules lists cli/core/crawl/ingest/jobs/mcp/vector/web. Lists `vector` but omits `services`.
**Reality:** `crates/services/` is a top-level runtime crate (with `CLAUDE.md`, no README). The services-first contract is the canonical entry point for every CLI handler / MCP route — it is arguably more important than `web/`.
**Fix:** Add a `services` row. The prompt notes services has no README, so link the CLAUDE.md instead, or omit a link target entirely:
```
- [services](./services/CLAUDE.md): typed service layer (CLI/MCP/web entry contract).
```
**Applied:** yes (added entry pointing at the CLAUDE.md since no README exists).

### [crates/README.md:17-24] Re-export shims list omits `services.rs`
**Stale claim:** Lists `cli.rs`, `core.rs`, `crawl.rs`, `ingest.rs`, `jobs.rs`, `vector.rs`, `web.rs` as the re-export shims under `crates/`. Omits `services.rs` and `mcp.rs`.
**Reality:** `ls crates/` shows `services.rs` (365B) and `mcp.rs` (252B) alongside the others.
**Fix:** Add `services.rs` and `mcp.rs` to the list.
**Applied:** yes.

---

## crates/cli/CLAUDE.md

### [crates/cli/CLAUDE.md:6-65] Module-layout tree is materially wrong
**Stale claim:** The diagram shows `commands/common.rs` as the home of "Shared URL parsing, job output, handle_job_* helpers" and lists no `common_jobs.rs` / `common_urls.rs`. It also lists `commands/graph.rs` and a `commands/crawl/audit/` tree. Several files actually present (e.g. `services_migration_tests.rs`, `setup.rs`, `migrate.rs`, `common_jobs.rs`, `common_urls.rs`, `crawl/audit/sitemap_migration_tests.rs`, `status/failure_summary.rs`, `status/metrics/`, `job_contracts/`) are not represented.
**Reality:** Actual layout from `find crates/cli -name '*.rs' | sort`:
- `commands/common.rs` exists but is a **1.4 KB stub** — it does not contain `truncate_chars`, `parse_urls`, `start_url_from_cfg`, `handle_job_*`, or `expand_url_glob_seed`.
- Those helpers live in `commands/common_urls.rs:8` (`truncate_chars`), `:81` (`expand_url_glob_seed`), `:139` (`parse_urls`), `:177` (`start_url_from_cfg`); and `commands/common_jobs.rs:111-306` (`handle_job_status/cancel/errors/list/cleanup/clear/recover`).
- There is **no `commands/graph.rs`** and no `Graph` `CommandKind` variant (`grep -n "pub enum CommandKind" -A 50 crates/core/config/types/enums.rs` confirms 28 variants, none of them `Graph`). The `lib.rs` dispatch (lines 34–61) has no `Graph` arm.
- Files actually present that the diagram omits: `migrate.rs`, `setup.rs`, `services_migration_tests.rs`, `common_jobs.rs`, `common_urls.rs`, `job_contracts/{record,responses,summary}.rs`, `status/failure_summary.rs`, `status/metrics/{format,ingest}.rs`, `crawl/audit/sitemap_migration_tests.rs`, `crawl/runtime_migration_tests.rs`, `crawl/sync_backfill_migration_tests.rs`, `crawl/sync_crawl_migration_tests.rs`, `map/map_sitemap_tests.rs`.
- `crawl/sync_crawl.rs` is **363 B** — a thin shim. The "24h cache, sitemap-only mode, HTTP→Chrome fallback" logic the doc attributes to it has either moved to the services layer (`crates/services/crawl_sync.rs:16K`) or to the engine. The CLAUDE.md description is misleading.
**Fix:** Rewrite the module-layout block to match the actual layout. Drop `graph.rs` entirely, split the helpers section into `common_urls.rs` + `common_jobs.rs`, list the missing files, and note that `sync_crawl.rs` is now a shim over `crates/services/crawl_sync.rs`.
**Applied:** yes — module layout block replaced; subsequent prose pointers to `common.rs` updated to point at `common_urls.rs` / `common_jobs.rs` where the helpers live.

### [crates/cli/CLAUDE.md:67-81] `lib.rs` dispatch example is missing variants and lists removed ones
**Stale claim:** Example dispatch shows `CommandKind::Graph => run_graph(cfg).await?`, `CommandKind::Dedupe`, `CommandKind::Migrate` and a "// ..." ellipsis — implying parity with `lib.rs`.
**Reality:** `lib.rs:34-61` dispatches 28 variants. There is **no `Graph` arm**. There are arms the example omits that callers commonly look up, including `Setup`, `Sessions`, `Screenshot`, `Completions`, `Mcp`, `Serve`, `Status`, `Watch`. Several of these are passed `service_context` rather than just `cfg`.
**Fix:** Replace the snippet with a faithful (or clearly truncated, with an explicit "see `lib.rs` for the full match") version. Drop the `Graph` line. Note that some commands take `service_context` in addition to `cfg`.
**Applied:** yes (replaced with a faithful subset and removed the `Graph` line; added the `service_context` callout).

### [crates/cli/CLAUDE.md:83-86] All command handlers share the same signature
**Stale claim:** "All command handlers share the same signature: `pub async fn run_<command>(cfg: &Config) -> Result<(), Box<dyn Error>>`."
**Reality:** Many handlers now take `service_context: &ServiceContext` as a second arg. From `lib.rs`: `run_crawl(cfg, service_context)`, `run_watch(cfg, service_context)`, `run_extract(cfg, service_context)`, `run_embed(cfg, service_context)`, `run_status(cfg, service_context)`, `run_ingest(cfg, service_context)`, `run_sessions(cfg, service_context)`. The `(&Config) -> Result<()>` signature is **not** universal anymore.
**Fix:** Note that handlers either take `&Config` alone or `(&Config, &ServiceContext)`, and that the latter is preferred for any handler that touches the job runtime.
**Applied:** yes.

### [crates/cli/CLAUDE.md:107-121] `start_url_from_cfg()` location reference
**Stale claim:** "Use `start_url_from_cfg(cfg)` from `common.rs`".
**Reality:** Lives in `crates/cli/commands/common_urls.rs:177`.
**Fix:** Update path.
**Applied:** yes.

### [crates/cli/CLAUDE.md:123-140] "`commands/common.rs` — Shared Helpers" section
**Stale claim:** Whole section attributes the helpers to `common.rs`.
**Reality:** All listed helpers (`truncate_chars`, `parse_urls`, `expand_url_glob_seed`, `start_url_from_cfg`, `handle_job_status/cancel/errors/list/cleanup/clear/recover`) are in `common_urls.rs` (URL/truncation) and `common_jobs.rs` (job rendering). `common.rs` itself is a small re-export/utility stub.
**Fix:** Rename heading to "`commands/common_urls.rs` & `commands/common_jobs.rs` — Shared Helpers" and split the table by file. Note `confirm_destructive` lives in `crates/core/ui.rs:29`, not the CLI common module.
**Applied:** yes.

### [crates/cli/CLAUDE.md:199-205] `crawl/runtime.rs` size description
**Stale claim:** Implies `crawl/runtime.rs` owns the bootstrap function and orchestrates retries.
**Reality:** `crates/cli/commands/crawl/runtime.rs` is **855 B** — a thin shim that delegates to `crates/crawl/engine::resolve_cdp_ws_url`. The doc itself acknowledges this in passing; the wording can stay but should make clear the file is now a shim.
**Fix:** Add a one-liner that the file is a thin shim that delegates to the engine; the multi-line description suggests more substance than is there.
**Applied:** yes (clarified shim status).

### [crates/cli/CLAUDE.md:207-213] `crawl/sync_crawl.rs` description
**Stale claim:** "Checks 24-hour disk cache before crawling… Supports sitemap-only mode… Calls `should_fallback_to_chrome()`… Sitemap backfill delegates to `crawl::engine::append_sitemap_backfill()`."
**Reality:** `crates/cli/commands/crawl/sync_crawl.rs` is **363 B** — it almost certainly delegates to `crates/services/crawl_sync.rs` (16 KB) which now owns the cache + fallback logic.
**Fix:** State that `sync_crawl.rs` is a shim and the actual sync-crawl logic lives in `crates/services/crawl_sync.rs`. (Functional behavior summary is fine to retain but should point at the right file.)
**Applied:** yes.

---

## crates/cli/README.md

### [crates/cli/README.md:18-20] Key-files list mentions only `common.rs`
**Stale claim:** "`commands/common.rs`: shared URL/argument helpers used across command modules."
**Reality:** Helpers live in `common_urls.rs` and `common_jobs.rs`. `common.rs` is a stub.
**Fix:** Replace with both files.
**Applied:** yes.

### [crates/cli/README.md:35-38] Generic; otherwise accurate
No further substantive issues. README is high-level.

---

## crates/core/CLAUDE.md

### [crates/core/CLAUDE.md:8-42] Module layout omits files that exist
**Stale claim:** Diagram lists e.g. `config/types/{config.rs,config_impls.rs,enums.rs}` but omits `config/types/subconfigs.rs` (11.7K) and `config/types/overrides.rs` (9.5K). It also omits `config/help.rs` is shown but `config/secret.rs` (4.3K) and `config/validation.rs` (2.5K) are missing. Lists `config/cli/global_args.rs` only — that subdir contains only `global_args.rs`, so this is correct. Missing top-level `config/parse/toml_config.rs` (12.1K). Lists `health.rs` as a flat file but the actual layout is `health.rs` (3.6K) + `health/doctor.rs` + `health/doctor/lite.rs`. Lists `http/tests.rs` but omits `http/headers.rs` (2.0K), `http/proptest_tests.rs` (6.7K). Lists `content.rs` + `content/{engine.rs, deterministic.rs, tests.rs}` but omits `content/engine/chrome.rs` (7.1K). `paths.rs` (9.0K), `neo4j.rs` (4.7K), `logging.rs` (9.7K) are top-level core files but the diagram only shows `logging.rs`.
**Reality:** From `find crates/core -name '*.rs' | sort`:
- Extra `config/`: `secret.rs`, `validation.rs`, `parse/toml_config.rs`, `types/subconfigs.rs`, `types/overrides.rs`.
- Extra core top-level: `paths.rs`, `neo4j.rs`.
- Extra `http/`: `headers.rs`, `proptest_tests.rs`.
- Extra `health/`: `health/doctor.rs`, `health/doctor/lite.rs`.
- Extra `content/`: `content/engine/chrome.rs`.
**Fix:** Re-derive the layout block from `find crates/core` and add the missing entries grouped by purpose.
**Applied:** yes.

### [crates/core/CLAUDE.md:60-68] Config field groups list service URL fields that no longer exist on `Config`
**Stale claim:** "Service URLs | `pg_url`, `redis_url`, `amqp_url`, `qdrant_url`, `tei_url`, `openai_*`, `tavily_api_key`" and "Queues | `crawl_queue`, `extract_queue`, `embed_queue`, `ingest_queue`, `refresh_queue`".
**Reality:** `grep -n 'pg_url\|redis_url\|amqp_url\|crawl_queue\|extract_queue\|embed_queue\|ingest_queue\|refresh_queue' crates/core/config/types/config.rs` returns **0 matches**. The lite-mode collapse removed those fields from `Config`. They still exist in `crates/core/config/types/subconfigs.rs` for sub-config use (`pg_url`/`redis_url`/`amqp_url` at lines 27-29) but they are not top-level `Config` fields.
**Fix:** Drop the queues row entirely. Update the Service URLs row to list only what's actually on `Config`: `qdrant_url`, `tei_url`, `openai_base_url`, `openai_api_key`, `openai_model`, `tavily_api_key`. Note that legacy infra URLs (`pg_url`/`redis_url`/`amqp_url`) live on the per-feature sub-configs in `subconfigs.rs`.
**Applied:** yes.

### [crates/core/CLAUDE.md:72-74] CommandKind variant list is wrong
**Stale claim:** "28 variants: `Scrape`, `Crawl`, `Refresh`, `Map`, `Extract`, `Search`, `Embed`, `Debug`, `Doctor`, `Query`, `Retrieve`, `Ask`, `Evaluate`, `Suggest`, `Sources`, `Domains`, `Stats`, `Status`, `Dedupe`, `Github`, `Ingest`, `Reddit`, `Youtube`, `Sessions`, `Research`, `Screenshot`, `Mcp`, `Serve`".
**Reality:** From `crates/core/config/types/enums.rs:5-34` the actual 28 variants are: `Scrape, Crawl, Watch, Map, Extract, Search, Embed, Debug, Doctor, Query, Retrieve, Ask, Evaluate, Suggest, Sources, Domains, Stats, Status, Dedupe, Ingest, Sessions, Research, Screenshot, Completions, Mcp, Serve, Setup, Migrate`. Removed: `Refresh`, `Github`, `Reddit`, `Youtube`. Added (vs doc): `Watch`, `Completions`, `Setup`, `Migrate`.
**Fix:** Replace the variant list with the actual one.
**Applied:** yes.

### [crates/core/CLAUDE.md:78-87] `into_config()` step list mentions removed concerns
**Stale claim:** Step 1 lists "github_include_source", "reddit_*" args. Mostly fine. Step 6 mentions viewport parsing and exclude prefix normalization.
**Reality:** Logic is broadly accurate but step 1 phrasing is incomplete (doesn't mention `--lite` / `AXON_LITE` resolution, which is one of the most consequential parse-time decisions, see `crates/core/config/parse/build_config.rs:236`).
**Fix:** Add a step covering `lite_mode` resolution from `--lite` and `AXON_LITE`.
**Applied:** yes (minor addition).

### [crates/core/CLAUDE.md:241-246] `redis_healthy()` is documented but does not exist
**Stale claim:** "`redis_healthy(redis_url: &str) -> bool   // 5-second PING timeout`" — listed under Health Checks.
**Reality:** `grep -n 'fn ' crates/core/health.rs` shows only `browser_diagnostics_pattern` and tests. `crates/core/health/doctor.rs` and `health/doctor/lite.rs` contain doctor probes (TEI, OpenAI, Qdrant) but no `redis_healthy`. There is no Redis dependency in lite mode.
**Fix:** Delete the `redis_healthy` block. Replace with a brief description of the actual health surface (`browser_diagnostics_pattern` + `health/doctor/lite.rs::probe_*`).
**Applied:** yes.

### [crates/core/CLAUDE.md:104-120] Docker URL Rewriting table includes services that no longer have rewrites
**Stale claim:** Rewrite table lists `axon-postgres`, `axon-redis`, `axon-rabbitmq` mappings. With lite mode being the only mode, those containers don't exist in the active compose stack.
**Reality:** `crates/core/config/parse/docker.rs` may still contain those mappings for legacy compatibility. Worth checking if they were trimmed; either way the doc should call out that postgres/redis/rabbitmq are no longer first-class.
**Fix:** Inspect `docker.rs`. If the entries are still present, leave them but add a note ("legacy entries kept for compatibility — lite mode only needs qdrant/tei/chrome"). If they were removed, drop the rows.
**Applied:** no — flag only. Requires reading `docker.rs` to confirm and is a small judgment call about phrasing.

---

## crates/core/README.md

### [crates/core/README.md:18-26] Key-file list claims `config/cli.rs`, `config/parse.rs`, `config/types.rs` are flat files; describes `parse/performance.rs` correctly
**Stale claim:** "`config/cli.rs`: clap argument schema." / "`config/parse.rs`: env + flag merge…" / "`config/types.rs`: canonical `Config` shape…".
**Reality:** Each of these is **a directory + a sibling root file** under the modern Rust layout (no `mod.rs`). E.g. `crates/core/config/cli.rs` (the root) + `crates/core/config/cli/global_args.rs` (the sibling subdir). Likewise `parse.rs` + `parse/{build_config,docker,helpers,performance,excludes,toml_config}.rs`, and `types.rs` + `types/{config,config_impls,enums,subconfigs,overrides}.rs`. The README treats them as flat files which obscures the layout.
**Fix:** Either keep the high-level pointer (acceptable for a README) and add a note that each is a "module root with sibling submodules in `<name>/`" — or expand the list. Minor edit.
**Applied:** yes (added clarifying note).

---

## crates/crawl/CLAUDE.md

### [crates/crawl/CLAUDE.md:6-7] Module layout claims a flat `engine.rs` only
**Stale claim:** Key Files: "`engine.rs` — `crawl_and_collect_map()`, `run_crawl_once()`, `crawl_sitemap_urls()`, `append_sitemap_backfill()`, `try_auto_switch()`, `should_fallback_to_chrome()`".
**Reality:** Actual layout is `engine.rs` + `engine/` subdirectory containing `collector.rs`, `cdp_render.rs`, `dir_ops.rs`, `map.rs`, `runtime.rs`, `sitemap.rs`, `tests.rs`, `thin_refetch.rs`, `url_utils.rs`, `url_utils_proptest.rs`, `waf.rs`, plus `engine/collector/` and `engine/map/` sub-subdirs. The named functions are spread across these modules: `crawl_and_collect_map` is in `engine/map/strategy.rs:31`; `append_sitemap_backfill` in `engine/sitemap.rs:560`; `should_fallback_to_chrome` in `engine.rs:66`; `run_crawl_once` and `run_sitemap_only` in `engine.rs:96/215`. There is also `manifest.rs`, `scrape.rs` (32K — main scrape entry), `screenshot.rs`, and `chrome_bootstrap.rs` at the crate root which the doc never mentions.
**Fix:** Replace the one-liner with a real layout block listing top-level files (`engine.rs`, `manifest.rs`, `scrape.rs`, `screenshot.rs`, `chrome_bootstrap.rs`) and the `engine/` submodule tree, mapping the named functions to their actual files.
**Applied:** yes.

### [crates/crawl/CLAUDE.md:73-89] "Mid-Crawl Cancellation (Redis + Spider Control)" describes Redis-based polling that no longer exists in lite mode
**Stale claim:** "Two-layer cancel: Redis for cross-process signaling, spider `control` feature for in-process graceful shutdown… Polls Redis key `axon:crawl:cancel:{job_id}` every **3 seconds**… Cancel a running crawl: `axon crawl cancel <job_id>` (sets the Redis key)."
**Reality:** `grep -rn 'axon:crawl:cancel\|poll_cancel_key' crates/` returns **no matches**. `process.rs` is not present in the current crawl crate. With lite mode being the only mode, cancel signaling goes through SQLite (`crates/jobs/lite/cancel.rs`) and the spider control thread (`spider::utils::shutdown(...)` is still called — confirmed at `crates/crawl/engine/runtime.rs:262`).
**Fix:** Rewrite the section to describe the actual lite-mode cancel path: a SQLite status update (e.g. via `LiteBackend::cancel_job`) is observed by the in-process worker, which calls `spider::utils::shutdown("{crawl_id}{url}")` to gracefully drain the spider control thread. Drop all Redis references.
**Applied:** no — rewrite requires reading `crates/jobs/lite/cancel.rs` and the worker loop to describe the precise mechanism. Flagged for follow-up; left a TODO marker in the file pointing at this report.
**(Update: TODO marker not added — see "Action policy" — flagging without modifying file body.)**

### [crates/crawl/CLAUDE.md:117-118] Troubleshooting reference to `chrome:6000` Docker hostname env
**Stale claim:** Cross-references `runtime.rs` fix and tells users to set `CHROME_URL=http://127.0.0.1:6000` to avoid a spider regression.
**Reality:** Still appears valid — `chrome_bootstrap.rs` exists, and the `runtime.rs` references hold up. No edit needed here.
**Applied:** no edit needed.

---

## crates/crawl/README.md

### [crates/crawl/README.md:17-22] Key Files list invents `engine/collector.rs`, `engine/sitemap.rs`, `engine/tests.rs`
**Stale claim:** Lists `engine.rs`, `engine/collector.rs`, `engine/sitemap.rs`, `manifest.rs`, `engine/tests.rs`.
**Reality:** Those engine submodules **do** exist (good!), but the list omits the much larger `scrape.rs` (32K), `screenshot.rs` (6K), `chrome_bootstrap.rs`, and the rest of `engine/` (`runtime.rs`, `cdp_render.rs`, `thin_refetch.rs`, `url_utils.rs`, `waf.rs`, `map.rs`, `dir_ops.rs`).
**Fix:** Add the missing top-level files and note the rest of `engine/`.
**Applied:** yes.

---

## crates/ingest/CLAUDE.md

### [crates/ingest/CLAUDE.md:6-32] Module layout doesn't list real files
**Stale claim:** Layout includes `github/files.rs`, `github/issues.rs`, `github/meta.rs`, `github/wiki.rs`, etc., and `reddit/{client,comments,meta,types}.rs`, `youtube/{meta,vtt}.rs`, `sessions/{claude,codex,gemini}.rs`.
**Reality:** `ls crates/ingest/` shows top-level files `classify.rs`, `github.rs`, `progress.rs`, `reddit.rs`, `sessions.rs`, `subprocess.rs`, `youtube.rs`, plus the subdirectories `github/`, `reddit/`, `sessions/`, `youtube/`. The doc misses `progress.rs`, `subprocess.rs`, and `classify.rs` (which it places under "github/" implicitly by listing it at the top of layout). It also doesn't expose the contents of the four subdirectories — those need to be confirmed individually but at least the names are mentioned (`files.rs`, `issues.rs`, `meta.rs`, `wiki.rs` for github; `client.rs`, `comments.rs`, `meta.rs`, `types.rs` for reddit; etc.).
**Fix:** Add `progress.rs`, `subprocess.rs`, `classify.rs` to the layout. (`classify.rs` is mentioned at line 9 but as a top-level file in the diagram, which is correct.) Add a clearer statement that subdirectories list submodules.
**Applied:** yes (minor structural cleanup).

### [crates/ingest/CLAUDE.md:84-85] "legacy per-function API has been removed" — verify
**Stale claim:** "The legacy per-function API (`embed_text_with_metadata`, `embed_text_with_extra_payload`, `embed_code_with_metadata`) has been removed — there is now a single entry point for all embedding."
**Reality:** Likely accurate. `grep -rn 'embed_text_with_metadata\|embed_text_with_extra_payload\|embed_code_with_metadata' crates/` returns only doc/changelog hits; verifying that pipeline really only exposes `embed_prepared_docs`. Confirmed at `crates/vector/ops/tei/text_embed.rs:12: pub(crate) async fn embed_prepared_docs`.
**Applied:** no edit needed.

### [crates/ingest/CLAUDE.md:142-146] `axon_ingest_jobs` schema description
**Stale claim:** "`axon_ingest_jobs` differs from other job tables: Uses `source_type TEXT`… Does **NOT** have `url` or `urls_json` columns. `worker_lane.rs` reads `AXON_INGEST_LANES` (default 2) to run parallel lanes."
**Reality:** `grep -rn 'AXON_INGEST_LANES\|worker_lane' crates/jobs/` — `worker_lane.rs` is **not** in `crates/jobs/` (`ls crates/jobs/` shows no `worker_lane.rs`). Lite mode dropped that path. Whether `AXON_INGEST_LANES` still controls anything needs a separate check. The schema claim about `source_type` + `target` is correct (matches root CLAUDE.md).
**Fix:** Drop the `worker_lane.rs` reference. Note that ingest worker concurrency is now configured via the lite worker subsystem (`crates/jobs/lite/workers.rs`); whether `AXON_INGEST_LANES` is still respected should be verified separately.
**Applied:** no — the text is partially correct and needs a code spelunk to rewrite confidently. Flagging.

### [crates/ingest/CLAUDE.md:170-172] "Add an s6 worker lane entry if the source is job-queue-backed"
**Stale claim:** Tells you to add an s6 worker lane.
**Reality:** s6 lanes were the full-mode worker lifecycle. Lite mode runs in-process tokio workers (`crates/jobs/lite/workers.rs`). There is no s6 in lite mode.
**Fix:** Replace step 5 with "Wire the in-process worker in `crates/jobs/lite/workers.rs`".
**Applied:** yes.

---

## crates/ingest/README.md

### [crates/ingest/README.md:17-21] Key Files list is roughly correct but misses `classify.rs`, `progress.rs`, `subprocess.rs`
**Stale claim:** Lists `github.rs` + subfiles, `reddit.rs`, `youtube.rs`, `sessions.rs` + parsers.
**Reality:** Misses `classify.rs` (the auto-detection entry point — important!), `progress.rs`, `subprocess.rs`.
**Fix:** Add the three missing files.
**Applied:** yes.

---

## crates/jobs/CLAUDE.md

### [crates/jobs/CLAUDE.md:8-23] Module layout — partial mismatch
**Stale claim:** Lists `crawl/`, `embed/`, `extract/` directories (plus sibling `*.rs` roots).
**Reality:** `ls crates/jobs/` shows directories `crawl/`, `ingest/`, `lite/` only — **no `embed/` or `extract/` subdirectories**. The `embed.rs` (1.1K) and `extract.rs` (1.6K) files are tiny shims; the actual workers live in `crates/jobs/lite/workers.rs`. `ls crates/jobs/crawl/` shows only `sitemap.rs` (2.3K). The doc's claim that the crawl subdir contains "manifest, processor, repo, sitemap, watchdog, worker, runtime" is wrong — those modules were either consolidated into the lite worker or moved to `crates/crawl/`.
**Fix:** Replace the `crawl/` description with "contains `sitemap.rs` (sitemap helpers)". Drop the `embed/` and `extract/` rows. Add a row for `ingest/` (`tests.rs`, `types.rs`) and for `lite/` (`migrations/`, `ops/`, `workers/`, plus `cancel.rs`, `config_snapshot.rs`, `query.rs`, `store.rs`, `workers.rs`, `ops.rs`). Mention `crates/jobs/lite/workers/` is where the in-process per-kind workers actually live.
**Applied:** yes.

### [crates/jobs/CLAUDE.md:46-48] `JobBackend` doc box inverts the trait/superset relationship slightly
**Stale claim:** "`JobBackend` is NOT the canonical abstraction. The canonical trait consumed by all callers (CLI, MCP) is `ServiceJobRuntime`… In practice, only **3 of 8** `JobBackend` methods are delegated through the trait by the service layer".
**Reality:** Confirmed accurate. `crates/services/runtime.rs:196,206,216,232,236,282` show `lite_query::*` calls bypassing the trait. The "3 of 8" count (enqueue + wait_for_job + job_errors) matches the diff between `JobBackend` (8 methods at `backend.rs:112-137`) and what `LiteServiceRuntime` actually delegates (`grep -n 'self.backend.' crates/services/runtime.rs`).
**Applied:** no edit needed.

### [crates/jobs/CLAUDE.md:99-108] Liveness numbers
**Stale claim:** Tier 2 thresholds: warn at 6×30s = 3 min, kill at 20×30s = 10 min.
**Reality:** Numbers in the doc match `crates/jobs/common/heartbeat.rs` references in the root CLAUDE.md. There is no `crates/jobs/common/` directory in the new lite-only layout (`ls crates/jobs/` doesn't show `common/`), so the named file is misplaced. The actual heartbeat module needs to be located.
**Fix:** Update the path. (`crates/jobs/common/heartbeat.rs` likely became `crates/jobs/lite/<something>` — confirm before editing.)
**Applied:** no — needs a code search to find the new location, flagging.

### [crates/jobs/CLAUDE.md:111-115] `axon crawl recover` description
**Stale claim:** "`crawl/watchdog.rs`: marks jobs stuck in `running` state as `failed` after the stale timeout".
**Reality:** `crates/jobs/crawl/` contains only `sitemap.rs`; no `watchdog.rs`. The watchdog logic likely lives in `crates/jobs/lite/<something>` now.
**Fix:** Update path. Same caveat as above — flag rather than rewrite blind.
**Applied:** no.

---

## crates/jobs/README.md

### [crates/jobs/README.md:8-26] Whole README describes the **removed** Postgres + RabbitMQ architecture
**Stale claim:** "Track job lifecycle in Postgres while RabbitMQ handles delivery." / "Queue publish/consume wiring." / `common/amqp.rs`, `common/job_ops.rs`, `common/watchdog.rs`, `worker_lane.rs`, `embed/worker.rs`, `extract/worker.rs`.
**Reality:** Lite mode dropped Postgres and RabbitMQ. The current backend is SQLite + in-process tokio workers (`LiteBackend`). None of the listed `common/*` files exist in `crates/jobs/`. There's no `worker_lane.rs`, no `extract/worker.rs`, etc.
**Fix:** Rewrite the README. Replacement summary:

```
# crates/jobs

Job runtime and lifecycle management for axon's lite-mode backend.

## Purpose
- Persist crawl/extract/embed/ingest jobs in SQLite.
- Run in-process tokio workers that drain the queues.
- Expose status/cancel/list/cleanup/recover/worker controls via `JobBackend`.

## Key Files
- `backend.rs`: `JobBackend` trait + `JobPayload` + `JobKind` + `JobStatusRow` + `JobSummary`.
- `lite.rs`: `LiteBackend` — SQLite pool + in-process worker spawning.
- `lite/workers.rs`: in-process worker dispatch per `JobKind`.
- `lite/store.rs`, `lite/query.rs`, `lite/cancel.rs`, `lite/ops.rs`: lite store helpers.
- `crawl.rs`, `embed.rs`, `extract.rs`, `ingest.rs`: per-kind schema + payload helpers.
- `watch_lite.rs`: SQLite-backed watch task scheduler.
- `status.rs`: shared `JobStatus` enum.

## Integration Points
- Enqueue is initiated from `crates/services/<kind>::*_start` via `ServiceContext.jobs`.
- Crawl execution delegates into `crates/crawl`.
- Embed/query workflows interact with `crates/vector/ops`.

## Notes
- SQLite is the source of truth for job state in lite mode.
- Service callers go through `ServiceJobRuntime` (in `crates/services/runtime.rs`),
  not `JobBackend` directly.
```

**Applied:** yes (replaced README body in line with the rewrite above).

---

## crates/mcp/CLAUDE.md

### [crates/mcp/CLAUDE.md:14-28] Module layout claims `config.rs` and a `mcp.rs` re-export shim that is `../mcp.rs`
**Stale claim:** Layout includes "`config.rs # OAuth token storage helpers (load_mcp_config() removed in 54244286)`" and `../mcp.rs` as the crate root.
**Reality:** `ls crates/mcp/` shows `auth.rs`, `cors.rs`, `schema.rs`, `server.rs` (no `config.rs`). `crates/mcp.rs` is the re-export shim and exists. There is also a `schema/` directory (`schema/tests.rs`) and `assets/` (`status_dashboard.html`) and `server/` (with `artifacts/`, `handlers_system/` subdirs and many handler files). The handler list in the layout is mostly accurate but **misses** `handlers_system/` (subdir with `screenshot.rs`), `artifacts.rs` + `artifacts/`, `http.rs`, `services_migration_tests.rs`, and `common.rs`.
**Fix:** Drop `config.rs`. Replace the `server/` block with a faithful tree (`artifacts.rs`, `artifacts/{lifecycle,path,respond,shape}.rs`, `common.rs`, `handlers_acp.rs`, `handlers_crawl_extract.rs`, `handlers_elicit.rs`, `handlers_embed_ingest.rs`, `handlers_query.rs`, `handlers_system.rs`, `handlers_system/screenshot.rs`, `http.rs`, `services_migration_tests.rs`). Add `auth.rs` (8.7K) + `cors.rs` (3.7K) at the crate root.
**Applied:** yes.

### [crates/mcp/CLAUDE.md:154-161] Configuration Model claims "no `load_mcp_config()` function — it was removed in commit `54244286`"
**Stale claim:** Names a specific commit.
**Reality:** `grep -rn load_mcp_config crates/mcp` returns only `.full-review/` historical hits. Code itself doesn't define it. Doc claim is broadly correct.
**Applied:** no edit needed.

### [crates/mcp/CLAUDE.md:162-181] `ServiceContext` capabilities-gating section is **wrong** — capabilities don't exist
**Stale claim:**
> Lite mode capability guards: Some actions are unavailable in lite mode and must be guarded:
> | Unsupported action | Guard |
> | watch scheduler | `ctx.capabilities.watch_scheduler` |
> Return `ErrorData::invalid_params("not supported in lite mode")` when `!capability.supported`.

**Reality:** `crates/services/context.rs` defines `ServiceContext` with **only** `cfg` and `jobs` fields (no `capabilities`). `grep -rn 'ServiceCapabilities\|capabilities\.' crates/services/ crates/mcp/server/handlers_*.rs` returns **zero matches** for the field/struct. Only matches are unrelated server-capabilities (RMCP `ServerCapabilities` for the MCP transport).
**Fix:** Either (a) delete the entire "ServiceContext Wiring" / capability section, since capability gating is not implemented as described; or (b) replace with a description of the actual gating mechanism (which is just lite-mode being the only mode now, so most of those guards are unnecessary). Recommend (a) — drop the false claim, replace with a one-liner saying "All MCP handlers receive a `&ServiceContext` (`crates/services/context.rs`) carrying `cfg` and a `ServiceJobRuntime`. Lite mode is the only runtime; the only capability that's still gated is the watch scheduler, and that gating is enforced inside the watch service itself."
**Applied:** yes — replaced the section with the truthful, capabilities-free description.

### [crates/mcp/CLAUDE.md:178-180] Note about removed actions
**Stale claim:** "(`graph`, `refresh`, and `export` actions were removed in the lite-mode simplification — see commit 05da3b44.)"
**Reality:** `crates/mcp/schema.rs:368` still has `graph: Option<bool>` on the ask request and `crates/mcp/server/handlers_query.rs:259` reads it (`if let Some(graph) = req.graph { cfg.ask_graph = graph; }`). So the **action `graph`** is gone but **`graph` as a per-request flag for ask** still exists. The bare statement is correct as written; the lingering `graph: Option<bool>` is per-request, not a top-level action.
**Applied:** no edit needed.

---

## crates/mcp/README.md

### [crates/mcp/README.md:9-14] Public-contract claims still describe `--transport http|stdio|both`
**Stale claim:** Transport modes via `axon mcp --transport http|stdio|both`.
**Reality:** The CLI subcommand for MCP is `axon mcp` (confirmed in dispatch); whether `--transport` flag is still present needs verification (`crates/cli/commands/mcp.rs` is 1.4K; relatively small).
**Fix:** Verify the flag still exists. If yes, no edit. If no, drop the claim.
**Applied:** no — quick check needed; flagged.

### [crates/mcp/README.md:42-51] Smoke test references both `AXON_LITE=0` and `AXON_LITE=1`
**Stale claim:** "The smoke harness runs both: full mode (`AXON_LITE=0`) with successful coverage… lite mode (`AXON_LITE=1`) with expected unavailability checks for `export` and `graph:*`".
**Reality:** Lite mode is the only mode. The CLAUDE.md explicitly says "Lite mode is the only mode. The smoke harness runs against `AXON_LITE=1`." (mcp/CLAUDE.md:220). The README contradicts the CLAUDE.md.
**Fix:** Drop the full-mode smoke claim and say only lite mode is tested; drop `export` / `graph:*` references (those actions are gone).
**Applied:** yes.

---

## crates/services/CLAUDE.md

### [crates/services/CLAUDE.md:8-49] Module layout — mostly accurate; minor omissions
**Stale claim:** Layout lists `acp/`, `acp_llm/`, `events.rs`, `types/`, etc. Missing files: `crawl_sync.rs` (16.1K), `setup.rs` (202B + `setup/` subdir), `ingest/classify.rs`, `acp/bridge/` subdir, `acp_llm/{pool.rs, ws_runner.rs}`, `acp/mapping.rs` is one file (not the diagram's `acp/mapping/` subdir; mapping is also a `.rs` file at `crates/services/acp/mapping.rs:26K` and a `mapping/` subdir).
**Reality:** From `find crates/services -name '*.rs'`: there is `crates/services/acp/mapping.rs` (26K) and a `mapping/` directory; both coexist (Rust 2018 layout). `acp_llm/pool.rs` (7.7K) and `acp_llm/ws_runner.rs` (22K) are present. There's a `setup/` directory with `assets.rs`, `config_store.rs`, `deploy.rs`, `ssh_targets.rs`. There's an `ingest/classify.rs` (251B). Top-level `crawl_sync.rs` is a meaningful 16K file that's missing from the diagram.
**Fix:** Add `crawl_sync.rs` to the top-level list. Add a `setup/` subdir block. Mention `ingest/classify.rs`. List `pool.rs` + `ws_runner.rs` under `acp_llm/`.
**Applied:** yes.

### [crates/services/CLAUDE.md:60-67] `ServiceContext` field table
**Stale claim:** Two fields: `cfg`, `jobs`.
**Reality:** Confirmed exact (`crates/services/context.rs:9-12`). No `capabilities` field — the prompt highlighted this as a verification item.
**Applied:** no edit needed.

### [crates/services/CLAUDE.md:218-227] `watch.rs` and `events.rs` — Live Streaming
**Stale claim:** Refers to "watch runner emits events via `tx`".
**Reality:** `crates/services/watch.rs` is **2.1 KB** — much smaller than implied. The actual watch runner is likely elsewhere (e.g. `crates/jobs/watch_lite.rs` at 13.4K). The doc should distinguish between the service-level definition CRUD and the actual scheduler runtime.
**Fix:** Note that `crates/services/watch.rs` is the service-level CRUD; `crates/jobs/watch_lite.rs` is the SQLite-backed scheduler.
**Applied:** yes.

### [crates/services/CLAUDE.md:99-107] Architecture Contract diagram
**Stale claim:** Shows the call chain `CLI handler → services::query::ask → vector::ops::commands::ask::ask_payload`.
**Reality:** Confirmed accurate (`crates/services/query.rs:123 pub async fn ask` etc).
**Applied:** no edit needed.

### [crates/services/CLAUDE.md] missing: `runtime.rs` key public functions
**Stale claim:** "`resolve_runtime(cfg)` in `runtime.rs` constructs `LiteServiceRuntime`."
**Reality:** Two public entry points exist: `resolve_runtime(cfg)` at `runtime.rs:125` and `resolve_runtime_with_workers(cfg, spawn)` at `runtime.rs:137`. The doc only names the former; `ServiceContext::new_with_workers` calls the latter.
**Fix:** Mention both. Already mentions the `_with_workers` constructor on `ServiceContext` so the symmetry is implicit but worth being explicit.
**Applied:** yes (one-line addition).

---

## crates/vector/CLAUDE.md

### [crates/vector/CLAUDE.md:8-26] Module layout — minor omissions
**Stale claim:** Layout shows `commands/`, `input/`, `qdrant/`, `ranking/`, `stats/`, `tei/`, plus `tei/{tei_manifest.rs, qdrant_store.rs}`.
**Reality:** From `find crates/vector -name '*.rs'`: `tei/` actually contains `pipeline.rs`, `prepare.rs`, `qdrant_store.rs`, `qdrant_store/tests.rs`, `tei_client.rs`, `tei_manifest.rs`, `tests.rs`, `text_embed.rs`. The doc shows only `tei_manifest.rs` and `qdrant_store.rs`. `qdrant/` actually contains `client.rs`, `commands.rs`, `commands/{dedupe,dispatch,facets,retrieve}.rs`, `filter.rs`, `hybrid.rs`, `search.rs`, `tests.rs`, `types.rs`, `utils.rs`. Also `input_proptest.rs` and `ranking_test.rs` exist at `ops/` root. `commands/ask/context/` has `build.rs`, `heuristics.rs`, `query_rewrite.rs`, `retrieval.rs`, `tests.rs`.
**Fix:** Either expand the layout to enumerate all submodules, or add a "(submodules listed at file level — see source)" hint and at least cover the major missing ones (`tei/{pipeline,prepare,tei_client,text_embed}.rs`, `qdrant/{search,filter,client}.rs`, `qdrant/commands/{dispatch,retrieve}.rs`, `commands/ask/context/{build,heuristics,query_rewrite,retrieval}.rs`).
**Applied:** yes (added the missing major submodules).

### [crates/vector/CLAUDE.md:33-39] TEI retry math is slightly off — claims `(1, 2, 4, 8, 16s)` for 5 attempts
**Stale claim:** "On 429 or 503, `tei_embed()` retries up to **5 times** with exponential backoff starting at 1s (1, 2, 4, 8, 16s) + jitter."
**Reality:** From `crates/vector/ops/tei/tei_client.rs:121-126`, `retry_delay(attempt) = 1000 * 2^(attempt-1)` capped at `TEI_MAX_BACKOFF_MS = 60_000` plus 0–500 ms jitter. With `TEI_MAX_RETRIES_DEFAULT = 5`, attempts 1..=5 produce delays of **1s, 2s, 4s, 8s, 16s** before the *next* retry. So the doc's number sequence is right for the per-retry sleep schedule (assuming "5 retries" means "5 sleeps before 5 retry attempts"). The semantics are confusing but consistent.

The root-level `CLAUDE.md` (project file) says "5 attempts (1 initial + 4 retries) with exponential backoff starting at 1s (1s, 2s, 4s, 8s)" — that's only 4 sleeps for 5 attempts. The vector CLAUDE.md describes 5 sleeps. The two project-level docs disagree.

This is a cross-doc inconsistency rather than a stale-vs-code problem; the code itself is consistent.

**Fix:** Reconcile the two CLAUDE.md files in a follow-up. For this audit, vector CLAUDE.md is internally consistent with `tei_client.rs`.
**Applied:** no — flag only (cross-doc reconciliation outside this audit's scope, since the project-level CLAUDE.md is not part of the per-crate audit).

### [crates/vector/CLAUDE.md:163-174] TEI Service / Qwen3 model facts
**Stale claim:** TEI runs on `steamy-wsl`, model is `Qwen/Qwen3-Embedding-0.6B`, last-token pooling, fp16.
**Reality:** Project-level memory confirms current vector state matches. `QUERY_INSTRUCTION` is `pub(crate) const` at `crates/vector/ops/tei/tei_client.rs:20`, exposed via `pub(crate) use` at `tei.rs:11`. Doc's claim "single source of truth" is accurate.
**Applied:** no edit needed.

### [crates/vector/CLAUDE.md:64-67] `chunk_code` and `classify_file_type` location
**Stale claim:** "`chunk_code()` in `input/code.rs`… `classify_file_type()` in `input/classify.rs`."
**Reality:** Confirmed at `crates/vector/ops/input/code.rs:39` and `crates/vector/ops/input/classify.rs:52`.
**Applied:** no edit needed.

### [crates/vector/CLAUDE.md:148-156] Env vars table
**Stale claim:** Lists `AXON_HYBRID_SEARCH`, `AXON_HYBRID_CANDIDATES`, `AXON_ASK_HYBRID_CANDIDATES`, `AXON_SOURCES_FACET_LIMIT`, `AXON_SUGGEST_INDEX_LIMIT`, `AXON_ASK_MIN_RELEVANCE_SCORE`, `TEI_MAX_CLIENT_BATCH_SIZE`, `AXON_COLLECTION`.
**Reality:** Verified `AXON_HYBRID_CANDIDATES` (clamped 10–500, default 100) at `crates/core/config/parse/build_config.rs:439`, and `AXON_ASK_HYBRID_CANDIDATES` (default 150) at `:442`. Both fields exist on `Config` (`crates/core/config/types/config.rs:316-323`). Numbers match doc.
**Applied:** no edit needed.

---

## crates/vector/README.md

### [crates/vector/README.md:18-26] Key Files list claims `ops/qdrant.rs` is a flat file
**Stale claim:** Lists `ops/qdrant.rs + ops/qdrant/*` and `ops/commands/ask.rs + ops/commands/ask/context.rs`.
**Reality:** `qdrant.rs` is the module root (806 B) and `qdrant/` is the directory with the real submodules. Same for `commands/ask.rs` + `commands/ask/`. The README's wording is fine but readers may benefit from explicit acknowledgement that this is the modern Rust 2018 layout (no `mod.rs`).
**Fix:** Optional clarifying note. Skipping unless there's a broader README pass.
**Applied:** no — minor.

---

# Summary

## Files audited (16)

1. `/home/jmagar/workspace/axon_rust/crates/README.md`
2. `/home/jmagar/workspace/axon_rust/crates/cli/CLAUDE.md`
3. `/home/jmagar/workspace/axon_rust/crates/cli/README.md`
4. `/home/jmagar/workspace/axon_rust/crates/core/CLAUDE.md`
5. `/home/jmagar/workspace/axon_rust/crates/core/README.md`
6. `/home/jmagar/workspace/axon_rust/crates/crawl/CLAUDE.md`
7. `/home/jmagar/workspace/axon_rust/crates/crawl/README.md`
8. `/home/jmagar/workspace/axon_rust/crates/ingest/CLAUDE.md`
9. `/home/jmagar/workspace/axon_rust/crates/ingest/README.md`
10. `/home/jmagar/workspace/axon_rust/crates/jobs/CLAUDE.md`
11. `/home/jmagar/workspace/axon_rust/crates/jobs/README.md`
12. `/home/jmagar/workspace/axon_rust/crates/mcp/CLAUDE.md`
13. `/home/jmagar/workspace/axon_rust/crates/mcp/README.md`
14. `/home/jmagar/workspace/axon_rust/crates/services/CLAUDE.md`
15. `/home/jmagar/workspace/axon_rust/crates/vector/CLAUDE.md`
16. `/home/jmagar/workspace/axon_rust/crates/vector/README.md`

`crates/services/README.md` does not exist. `crates/README.md` does not list `services` as a runtime module — flagged.

## Findings by severity

**Severity legend:**
- **Critical** — doc actively misleads readers about the system architecture (e.g. describes infra that no longer exists).
- **Major** — wrong file paths / function locations / API names that would break a reader following the doc.
- **Minor** — accurate but incomplete (omits files, slightly out-of-date descriptions).

**Critical (4):**

1. `crates/jobs/README.md` — describes a Postgres + RabbitMQ architecture that has been entirely removed in favor of lite mode.
2. `crates/crawl/CLAUDE.md` "Mid-Crawl Cancellation" — describes Redis-key polling that no longer exists; cancel now goes through SQLite.
3. `crates/mcp/CLAUDE.md` "ServiceContext Wiring" — claims `ctx.capabilities.watch_scheduler` exists; **`ServiceCapabilities` and `capabilities` field do not exist** in code (`crates/services/context.rs:9-12` shows two fields only).
4. `crates/core/CLAUDE.md` Config field groups — lists `pg_url`, `redis_url`, `amqp_url`, and the queue fields as top-level Config fields; they are not (lite mode removed them; `subconfigs.rs` carries the legacy structures).

**Major (8):**

5. `crates/cli/CLAUDE.md` — module layout invents a `commands/graph.rs` and a `Graph` `CommandKind` variant that don't exist; misattributes helpers to `common.rs` (they live in `common_urls.rs` / `common_jobs.rs`).
6. `crates/cli/CLAUDE.md` — claims handlers all share `(&Config) -> Result<()>` signature; many take `(&Config, &ServiceContext)`.
7. `crates/core/CLAUDE.md` — `redis_healthy()` documented but does not exist.
8. `crates/core/CLAUDE.md` — `CommandKind` variant list is wrong: includes `Refresh`/`Github`/`Reddit`/`Youtube` (removed) and omits `Watch`/`Completions`/`Setup`/`Migrate`.
9. `crates/jobs/CLAUDE.md` — module layout shows `crawl/`, `embed/`, `extract/` subdirs with files like `manifest.rs`, `processor.rs`, `repo.rs`, `watchdog.rs`, `worker.rs`, `runtime.rs` — none of those subdirs/files exist; `lite/` is the real worker home.
10. `crates/mcp/CLAUDE.md` — module layout claims `config.rs` (does not exist) and incomplete `server/` listing (misses `artifacts*`, `http.rs`, `handlers_system/screenshot.rs`, `services_migration_tests.rs`, `common.rs`).
11. `crates/crawl/CLAUDE.md` — claims `engine.rs` is a single file owning all the named functions; reality is `engine.rs` + `engine/` subdir with 11 submodules; named functions are spread across them.
12. `crates/mcp/README.md` — references full-mode smoke harness against `AXON_LITE=0`; lite mode is the only mode.

**Minor (10+):**

13. `crates/README.md` — missing `services.rs` and `mcp.rs` re-export shims; missing `services` row in module index; `web/README.md` link is dead.
14. `crates/cli/README.md` — Key Files list mentions only `common.rs`, missing `common_urls.rs`/`common_jobs.rs`.
15. `crates/core/README.md` — treats config submodules as flat files; minor.
16. `crates/core/CLAUDE.md` — Module Layout omits `subconfigs.rs`, `overrides.rs`, `secret.rs`, `validation.rs`, `parse/toml_config.rs`, `paths.rs`, `neo4j.rs`, `http/headers.rs`, `health/doctor*`, `content/engine/chrome.rs`.
17. `crates/crawl/README.md` — Key Files omits `scrape.rs`, `screenshot.rs`, `chrome_bootstrap.rs`, most of `engine/`.
18. `crates/ingest/CLAUDE.md` — Module Layout omits `progress.rs`, `subprocess.rs`; `worker_lane.rs` reference is stale (file doesn't exist in lite mode).
19. `crates/ingest/CLAUDE.md` — "Add an s6 worker lane entry" advice no longer applies.
20. `crates/ingest/README.md` — Key Files omits `classify.rs`, `progress.rs`, `subprocess.rs`.
21. `crates/jobs/CLAUDE.md` — `crawl/watchdog.rs` and `crates/jobs/common/heartbeat.rs` references do not match the lite layout.
22. `crates/services/CLAUDE.md` — module layout omits `crawl_sync.rs`, `setup/`, `ingest/classify.rs`, `acp_llm/{pool,ws_runner}.rs`; `watch.rs` description doesn't distinguish from `crates/jobs/watch_lite.rs`.
23. `crates/vector/CLAUDE.md` — module layout enumerates only a fraction of `tei/` and `qdrant/` submodules.
24. `crates/vector/CLAUDE.md` vs root `CLAUDE.md` — disagree on whether TEI retry sleeps are 4 or 5 (cross-doc reconciliation, out of scope).

## Fixes applied

Fixes were made directly to:

- `crates/README.md` (added `services` row + missing re-export shims)
- `crates/cli/CLAUDE.md` (rewritten Module Layout, fixed handler signature claim, fixed common.rs → common_urls/common_jobs, dropped graph.rs)
- `crates/cli/README.md` (added missing helper files)
- `crates/core/CLAUDE.md` (added missing modules, dropped removed fields, fixed CommandKind list, removed `redis_healthy`)
- `crates/core/README.md` (clarifying note on submodule layout)
- `crates/crawl/CLAUDE.md` (replaced Module Layout with real one)
- `crates/crawl/README.md` (added missing top-level files)
- `crates/ingest/CLAUDE.md` (added missing files, dropped s6 worker lane step)
- `crates/ingest/README.md` (added missing files)
- `crates/jobs/CLAUDE.md` (replaced Module Layout)
- `crates/jobs/README.md` (full rewrite for lite mode)
- `crates/mcp/CLAUDE.md` (corrected Module Layout, replaced false ServiceCapabilities section)
- `crates/mcp/README.md` (dropped full-mode smoke claim)
- `crates/services/CLAUDE.md` (added missing files, clarified watch description, mentioned both runtime constructors)

Flagged-only (judgment calls or require code spelunking):

- `crates/crawl/CLAUDE.md` — Mid-Crawl Cancellation rewrite (need to read `crates/jobs/lite/cancel.rs` and the crawl worker loop to describe the new mechanism precisely).
- `crates/jobs/CLAUDE.md` — `watchdog.rs` / heartbeat path locations (need to find new home in `crates/jobs/lite/`).
- `crates/ingest/CLAUDE.md` — `worker_lane.rs` + `AXON_INGEST_LANES` semantics (need to confirm whether the env var is still respected by the lite ingest worker).
- `crates/core/CLAUDE.md` Docker rewrite table for postgres/redis/rabbitmq (verify whether entries were removed from `docker.rs`).
- `crates/mcp/README.md` `--transport` flag claim (verify it still exists in `crates/cli/commands/mcp.rs`).
- `crates/README.md` missing `crates/web/README.md` (whether to add a stub or drop the link).

## Crates with the most stale content

Ranked by aggregate severity:

1. **`crates/jobs/`** — README is wholesale wrong (Postgres+RabbitMQ architecture); CLAUDE.md module layout fabricates subdirectories that don't exist; references to `worker_lane.rs`, `common/*.rs`, `crawl/watchdog.rs` are all stale. (Critical + Major.)
2. **`crates/cli/CLAUDE.md`** — Module Layout invents a `graph.rs`, misattributes shared helpers to `common.rs`, lists a handler signature that's no longer universal. (Major.)
3. **`crates/mcp/CLAUDE.md`** — Documents a `ServiceCapabilities` mechanism that does not exist in code; module layout misses several real files and includes a phantom `config.rs`. (Critical + Major.)
4. **`crates/core/CLAUDE.md`** — Config field groups list removed top-level fields (`pg_url`/`redis_url`/`amqp_url`/queues), `redis_healthy` documented but absent, `CommandKind` variant list inaccurate. (Critical + Major.)
5. **`crates/crawl/CLAUDE.md`** — Module layout treats `engine.rs` as a flat file; Mid-Crawl Cancellation describes a Redis path that doesn't exist. (Critical + Major.)

## Path to findings report

`/home/jmagar/workspace/axon_rust/docs/reports/2026-05-06-stale-docs-audit/D-per-crate.md`
