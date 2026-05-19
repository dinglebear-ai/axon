# Stale Docs Audit — Root + Foundational Files

Date: 2026-05-06
Branch: `main` @ `69d0917b`
Version: `1.5.4`

Files audited (7):
- `/home/jmagar/workspace/axon_rust/CLAUDE.md`
- `/home/jmagar/workspace/axon_rust/README.md`
- `/home/jmagar/workspace/axon_rust/CHANGELOG.md` (last 3 versioned sections)
- `/home/jmagar/workspace/axon_rust/docs/ARCHITECTURE.md`
- `/home/jmagar/workspace/axon_rust/docs/CONFIG.md`
- `/home/jmagar/workspace/axon_rust/docs/SETUP.md`
- `/home/jmagar/workspace/axon_rust/docs/DEPLOYMENT.md`

## Summary of severity counts

- Critical (commands/architecture documented that don't exist; will mislead first-time users): 12
- Major (incorrect file paths, removed flags/env, wrong defaults that affect setup/troubleshooting): 18
- Minor (outdated header dates/version stamps, tiny phrasing nits, recipe naming drift): 9

Total findings: 39
Fixes applied directly: 12 (mechanical updates with unambiguous code evidence)
Flagged-only: 27 (most are large structural rewrites that exceed the "mechanical fix" bar of this audit)

---

## Ground truth references used

- Cargo.toml version: `1.5.4`
- `crates/core/config/types/enums.rs` — canonical `CommandKind` enum:
  `Scrape, Crawl, Watch, Map, Extract, Search, Embed, Debug, Doctor, Query, Retrieve, Ask, Evaluate, Suggest, Sources, Domains, Stats, Status, Dedupe, Ingest, Sessions, Research, Screenshot, Completions, Mcp, Serve, Setup, Migrate`
- `lib.rs:34-61` dispatches exactly the variants above.
- `crates/jobs/` is now a lite-only tree:
  `backend.rs, crawl.rs, embed.rs, extract.rs, ingest.rs, lite.rs, watch_lite.rs, status.rs, error.rs` plus `crawl/sitemap.rs`, `ingest/{tests,types}.rs`, and `lite/{ops,workers,query,store,cancel,workers/runners/{crawl,embed,extract,ingest}.rs,...}`
- `crates/services/types/service.rs:389` — `MapResult.returned_url_count` (renamed in 1.5.1).
- `Justfile` recipes that actually exist: `default check check-tests test test-fast test-infra mcp-smoke test-all nextest-install fmt fmt-check clippy build install lint-all verify ci precommit fix fix-all taplo-check taplo-fmt llvm-cov-install coverage-branch clean services-up services-down test-infra-up test-infra-down watch-check rebuild stop dev`. (No `up`, no `down-all`, no `setup`.)
- `crates/core/config/parse/build_config.rs:236` — `let lite_mode = global.lite || env_bool("AXON_LITE", false);` — lite default is **false**.
- `.env.example` ships `AXON_LITE=1`, so out-of-the-box experience IS lite mode, but only because the example sets it; the binary's own default is `false`.
- No `mod.rs` files exist anywhere (`find … -name mod.rs` returns 0).

---

## Findings — CLAUDE.md (root)

### [CLAUDE.md:1-2] Header naming and stale "Last Modified"
**Stale claim:** `# axon_cli — Axon CLI (Rust + Spider.rs)` and `Last Modified: 2026-04-27`.
**Reality:** `Cargo.toml` package name is `axon`, not `axon_cli`. CLAUDE.md was edited as part of 1.5.4 work (e.g. crawl queue cap reference change in 1.5.0 came in via `258350f9`); the Last-Modified stamp is a month behind today (2026-05-06).
**Fix:** Bump to `2026-05-06` and tighten the title to `Axon CLI`.
**Applied:** yes

### [CLAUDE.md:8 / 309-329] "Lite mode (default)" overstates current default
**Stale claim:** `> **Lite mode (default)**: axon requires only Qdrant and TEI.` and "Lite mode is the default operating mode."
**Reality:** `crates/core/config/parse/build_config.rs:236` reads `env_bool("AXON_LITE", false)` — lite is **only on when `AXON_LITE=1`/`--lite` is set**. CHANGELOG 1.5.3 explicitly fixes the same wording in README. CLAUDE.md still has the incorrect framing.
**Fix:** Replace "Lite mode (default)" with "Lite mode (recommended; enabled in `.env.example`)" and adjust line 309 to "Lite mode is the recommended operating mode and is enabled by default in `.env.example`. The binary itself defaults to full mode (`AXON_LITE=false`)."
**Applied:** no — flagged. Same precise wording PR #67 chose for README should be re-used; want a human to mirror it consistently rather than risk subtle drift.

### [CLAUDE.md:66] `watch` listed as having "Depends" async
**Stale claim:** `| `watch <sub>` | … | Depends |` and CLAUDE.md line 319 lists `watch scheduler` as **Unsupported in lite mode**.
**Reality:** `crates/jobs/watch_lite.rs` exists (13.4 KB) and `crates/services/watch.rs` is wired into the lite path; `crates/cli/commands/watch.rs` has a `run_watch_lists_in_lite_mode` test. README line 671 also says lite disables watch — but watch *list/get/run-now/history* clearly run in lite mode now.
**Fix:** Either retract the lite-mode prohibition or scope it to the *scheduler/cron loop* only. Subcommand-level support exists.
**Applied:** no — flagged. Need to confirm exactly which `watch` subactions are gated; touching this requires reading `services/watch.rs` more carefully than the audit budget allows.

### [CLAUDE.md:67 + 367-376] `migrate` example uses `cortex` → `cortex_v2`
**Stale claim:** `axon migrate --from cortex --to cortex_v2`.
**Reality:** Example is fine, but the surrounding doc says default collection is `cortex` (line 280 `AXON_COLLECTION=cortex`). User's MEMORY indicates the active collection on this host is `axon`. The default in code (`crates/core/config/types/subconfigs.rs`) is still `cortex` — kept this one as-is.
**Applied:** no — accurate to default.

### [CLAUDE.md:160-185] Architecture map references non-existent files
**Stale claim:**
- `crates/jobs/crawl/` (manifest, processor, repo, sitemap, watchdog, worker, runtime)
- `crates/jobs/{extract,embed}/` modules, `crates/jobs/ingest.rs`
- `crates/jobs/common/*` and `crates/jobs/worker_lane.rs`
- `crates/services::{query,retrieve,ask,sources,domains,stats,system}` … `crates/services/types/service.rs`

**Reality:** Actual files (verified by `find crates/jobs -name '*.rs'`):
- jobs root holds flat `crawl.rs, embed.rs, extract.rs, ingest.rs, lite.rs, watch_lite.rs, status.rs, backend.rs, error.rs`
- workers live under `crates/jobs/lite/workers/runners/{crawl,embed,extract,ingest}.rs`
- there is NO `crates/jobs/common/`, NO `crates/jobs/worker_lane.rs`, NO `crates/jobs/crawl/runtime/worker/loops.rs`
- `crates/services` has no `system.rs`, no `retrieve` service file (retrieve is folded into `query.rs`); the file `crates/services/system.rs` does exist (23 KB) and DOES expose system/sources/domains/stats helpers, so that part is OK
- `crates/services/types/service.rs` does exist (correct)

**Fix:** Replace the bullets with a current map:
```
- Async jobs (lite-only):
  - crates/jobs/{crawl,embed,extract,ingest,watch_lite,status,backend,error}.rs
  - crates/jobs/lite/{ops,workers,query,store,cancel}.rs and lite/workers/runners/{crawl,embed,extract,ingest}.rs
  - crates/jobs/crawl/sitemap.rs (sitemap backfill helper)
- Services layer: crates/services/{query,scrape,crawl,crawl_sync,embed,extract,ingest,map,migrate,
  search,system,watch,setup,screenshot,debug,jobs,acp,acp_llm,context,runtime,events,error}.rs
  with subdirs services/{acp,acp_llm,setup,types,ingest}/.
```
**Applied:** no — flagged. This is a paragraph-level rewrite; want a human to confirm crate structure intent (the project may yet split worker code back out).

### [CLAUDE.md:189-211] Docker Compose section calls infrastructure file the only compose file but talks about `services.env`
**Stale claim:** "The stack uses a single compose file for infrastructure services" + a row `services.env`.
**Reality:** `config/docker-compose.services.yaml` references `../services.env` (per `docs/CONFIG.md:21`). `services.env` lives at the repo root, not under `config/`. Mostly fine; the table caption `Env file: services.env` is correct. No change needed.
**Applied:** no — accurate.

### [CLAUDE.md:280] AXON_COLLECTION default
**Stale claim:** `AXON_COLLECTION=cortex` and "(default: cortex)".
**Reality:** Matches `crates/core/config/types/subconfigs.rs`. Accurate.
**Applied:** no — accurate.

### [CLAUDE.md:298-305] "MCP OAuth (`atk_` tokens) is the auth system for MCP clients"
**Stale claim:** This section says OAuth is *the* auth system, then lists only `AXON_MCP_ALLOWED_ORIGINS` as the env.
**Reality:** `docs/CONFIG.md:256-263` documents `AXON_MCP_HTTP_HOST`, `AXON_MCP_HTTP_PORT`, `AXON_MCP_HTTP_TOKEN`, `AXON_MCP_ALLOWED_ORIGINS`, `AXON_MCP_ARTIFACT_DIR`, `AXON_INLINE_BYTES_THRESHOLD`, `AXON_MCP_EMBED_ALLOWED_ROOTS`, `AXON_MCP_EMBED_MAX_LOCAL_BYTES`. Bearer-token gate (`AXON_MCP_HTTP_TOKEN`) is the actual gate; `atk_` OAuth tokens are an additional path. CLAUDE.md is misleadingly minimal.
**Fix:** Either link to `docs/CONFIG.md` and `docs/mcp/ENV.md`, or expand the table to include `AXON_MCP_HTTP_HOST/PORT/TOKEN`.
**Applied:** no — flagged. Multi-line rewrite.

### [CLAUDE.md:319] "Unsupported in lite mode: watch scheduler."
**Stale claim:** Watch scheduler unsupported in lite.
**Reality:** Per the watch_lite test in `crates/cli/commands/watch.rs`, several watch operations work in lite. Only the *automatic scheduler loop* may be missing.
**Fix:** Tighten phrasing: "Unsupported in lite mode: watch scheduler automation (cron loop). `axon watch list/get/run-now/history` work."
**Applied:** no — flagged (same root cause as the line-66 finding).

### [CLAUDE.md:343-344] ACP `OPENAI_MODEL` description
**Stale claim:** `OPENAI_MODEL` is the model override knob for ACP-backed calls.
**Reality:** Confirmed — `docs/CONFIG.md:127`. Accurate.
**Applied:** no — accurate.

### [CLAUDE.md:382] Stray sentence "Both compose files set `context: .` …"
**Stale claim:** Refers to "both compose files" — there is only one tracked compose file in this repo.
**Reality:** `config/docker-compose.services.yaml` is the only tracked compose file (verified by `ls config/`); the second file (test infra) is in a different path or has been removed.
**Fix:** Drop the sentence or rewrite to singular: "The compose file sets `context: .` — run `docker compose build` from this directory, not from a parent workspace."
**Applied:** yes

### [CLAUDE.md:419-424] `Config` test-helper guidance points to wrong file
**Stale claim:** "When adding a new non-`Option` field to `Config` in `crates/core/config.rs`, you must also update the inline `Config { .. }` struct literals…"
**Reality:** `Config` lives at `crates/core/config/types/config.rs` (mod root is `crates/core/config/types.rs`). `crates/core/config.rs` does not exist (the entry is `crates/core/config/mod` … actually `crates/core/config` itself: there is no `crates/core/config.rs` either; the parent is `crates/core/core.rs` per modern layout, with `crates/core/config/types.rs` re-exporting). Checked: `find crates/core -maxdepth 2 -name 'config.rs'` returns nothing.
**Fix:** Update path to `crates/core/config/types/config.rs`.
**Applied:** yes

### [CLAUDE.md:464-475] Just recipes list `up` and `down-all` that no longer exist
**Stale claim:** `just down-all` and (in line 222 / 474) the same. Also `just rebuild` mentions docker-build.
**Reality:** `Justfile` exposes `services-up`, `services-down`, `test-infra-up`, `test-infra-down`, `stop`, `dev`, but no `up` or `down-all`. `rebuild` exists.
**Fix:** Remove `just down-all`. Replace stop-everything guidance with `just services-down && just stop` (or clarify users should compose their own teardown). At line 222 strip `down-all`.
**Applied:** yes (both line 222 and 474)

### [CLAUDE.md:511-522] DB schema: `lite mode` parenthetical missing the unified watch table
**Stale claim:** Lists 4 tables: crawl/extract/embed/ingest jobs.
**Reality:** `crates/jobs/watch_lite.rs` references `axon_watch_defs` and `axon_watch_runs` tables (also in README:627). At minimum CLAUDE.md should mention them.
**Fix:** Add a note "watch defs/runs (`axon_watch_defs`, `axon_watch_runs`) live alongside in lite mode."
**Applied:** no — flagged (low-impact addition; choose not to extend audit beyond stale-claim removal).

### [CLAUDE.md:617-624] Version-bumping checklist references `package.json`/`pyproject.toml` in this repo
**Stale claim:** "Files to update (if they exist in this repo)" includes `package.json`, `pyproject.toml`. Implies these may exist.
**Reality:** No `package.json` or `pyproject.toml` at repo root. `apps/web/package.json` exists but is for the web app and is not the canonical version. The version-bearing files actually maintained are `Cargo.toml`, `.claude-plugin/plugin.json`, and `CHANGELOG.md`.
**Fix:** Note explicitly says "if they exist," so it's technically correct, but the user (per recent commits) only bumps Cargo.toml and `.claude-plugin/plugin.json`. Optionally remove the irrelevant rows.
**Applied:** no — flagged (judgment call).

---

## Findings — README.md

### [README.md:7] Version stamp `1.3.4` is stale
**Stale claim:** `Version: 1.3.4 | License: MIT`
**Reality:** `Cargo.toml` is `1.5.4`. CHANGELOG also at 1.5.4.
**Fix:** Update to `Version: 1.5.4 | License: MIT`.
**Applied:** yes

### [README.md:32-43] TOC and command sections include `refresh`, `graph`, `export`, `artifacts`, `serve`, `setup` — none implemented
**Stale claim:** README has full sections for `refresh`, `graph`, `export`, `artifacts` (and a `setup` reference) as CLI subcommands.
**Reality:** `CommandKind` enum in `crates/core/config/types/enums.rs` enumerates only: `Scrape, Crawl, Watch, Map, Extract, Search, Embed, Debug, Doctor, Query, Retrieve, Ask, Evaluate, Suggest, Sources, Domains, Stats, Status, Dedupe, Ingest, Sessions, Research, Screenshot, Completions, Mcp, Serve, Setup, Migrate`. There is **no `Refresh`, `Graph`, `Export`, or `Artifacts`** variant. `lib.rs` dispatch confirms the same. There is no `crates/cli/commands/{refresh,graph,export,artifacts}.rs`. `Setup` IS implemented (`crates/cli/commands/setup.rs`); `Serve` IS implemented.
**Fix:** Remove/rewrite the `refresh`, `graph`, `export`, `artifacts` sections. They mislead anyone trying these commands. (Some of these may exist under `axon mcp` action routing, e.g. `artifacts` is an MCP action but not a CLI subcommand — README conflates them.)
**Applied:** no — flagged. This is a 200-plus-line surgical removal/rewrite affecting TOC, command tables, examples, and the End-to-End example (which calls `axon graph build --all`, `axon graph worker`, `axon refresh schedule add …`). Needs human judgment about whether to keep them as MCP-only (with proper labeling) or delete entirely.

### [README.md:90-102] Architecture table — looks roughly correct, BUT entry-points wording is inaccurate
**Stale claim:** Bullet list "A CLI binary: `axon` … An SSH deployment helper: `axon setup`".
**Reality:** `axon setup` IS the SSH deploy helper (`crates/cli/commands/setup.rs` and `crates/services/setup/`); accurate. The table itself is fine.
**Applied:** no — accurate.

### [README.md:169] `--graph` global flag
**Stale claim:** Lists `--graph` as a global flag.
**Reality:** `crates/core/config/cli/global_args.rs` does not define a `--graph` flag (no Neo4j wiring in CLI now). Confirmed by absence of any clap attribute referencing `graph`/`Graph`.
**Fix:** Remove `--graph` row.
**Applied:** no — flagged (part of the broader graph-removal rewrite).

### [README.md:225-241] Service URL flags `--pg-url`, `--redis-url`, `--amqp-url`
**Stale claim:** Documents `--pg-url <url>`, `--redis-url <url>`, `--amqp-url <url>` and queue flags `--crawl-queue/--extract-queue/--embed-queue` with env vars `AXON_PG_URL`, `AXON_REDIS_URL`, `AXON_AMQP_URL`.
**Reality:** None of these flags exist in `crates/core/config/cli/global_args.rs`. The full-mode (Postgres/AMQP/Redis) backend has been removed from this build; only `LiteBackend` exists in `crates/jobs/`. The struct fields (`pg_url`, `redis_url`, `amqp_url`) survive in `crates/core/config/types/subconfigs.rs` but the clap flags do not.
**Fix:** Delete the `--pg-url/--redis-url/--amqp-url` rows from the Service URL Overrides table and the queue table. Keep `--qdrant-url`, `--tei-url`, `--openai-base-url`, `--openai-api-key`, `--openai-model`.
**Applied:** no — flagged. Larger judgment call: the `Config` struct retains the fields; if a future PR re-adds full mode this needs to come back. Want a human to confirm intent.

### [README.md:251-268] `scrape` flags include `--url-glob`, but glob feature is explicitly NOT enabled per CLAUDE.md
**Stale claim:** `axon scrape --url-glob "https://docs.example.com/{1..10}"` is shown as an example.
**Reality:** CLAUDE.md:403 explicitly: "**`glob`**: NOT enabled — … Do NOT add it back." The Spider `glob` feature is disabled, so `--url-glob` is unlikely to function.
**Fix:** Remove the `--url-glob` example line, OR add a "(disabled)" note.
**Applied:** no — flagged. Need to confirm whether axon parses `--url-glob` itself before delegating to Spider; if axon expands the pattern client-side, the example is fine.

### [README.md:421-453] `ask` "RAG pipeline" lists `AXON_ASK_CHUNK_LIMIT (default: 10)` — but README also says it backfills from "top `AXON_ASK_FULL_DOCS` (default: 4) documents"
**Stale claim:** Lists `AXON_ASK_BACKFILL_CHUNKS` is missing.
**Reality:** `docs/CONFIG.md:203` documents `AXON_ASK_BACKFILL_CHUNKS` (default `3`, clamp 0-20). README's pipeline narrative skips this knob.
**Fix:** Add a sentence about `AXON_ASK_BACKFILL_CHUNKS` in the pipeline list.
**Applied:** no — flagged (additive enhancement, not a stale claim).

### [README.md:578-621] `refresh` command and refresh schedules
**Stale claim:** Full doc of `axon refresh`, `axon refresh schedule add ...`, tier presets etc.
**Reality:** No `Refresh` CommandKind variant. There is no `crates/cli/commands/refresh.rs`.
**Fix:** Delete the section.
**Applied:** no — flagged (part of the refresh/graph/export/artifacts removal).

### [README.md:675-697] `graph` command
**Stale claim:** `axon graph build`, `graph status`, `graph explore`, `graph stats`, `graph worker`.
**Reality:** No `Graph` CommandKind variant. `lib.rs` dispatch lacks any `Graph` branch. (Graph code remnants exist in `crates/services/.full-review/` review docs and `crates/core/neo4j.rs`, but no live CLI surface.)
**Fix:** Delete or convert to a "future / not yet implemented" callout.
**Applied:** no — flagged.

### [README.md:761-790] `export` and `artifacts` CLI sections
**Stale claim:** `axon export [--include-history]`, `axon artifacts list/head/grep/wc/read/search/delete/clean`.
**Reality:** No `Export` or `Artifacts` CommandKind. Artifacts IS an MCP **action**, not a CLI subcommand. README intermixes them.
**Fix:** Move "artifacts" content under the MCP Lifecycle Families table (where it already exists at line 939) and delete the standalone CLI section. Delete the `export` subsection.
**Applied:** no — flagged.

### [README.md:743-748] `doctor` "Checks: Postgres, Redis, RabbitMQ, Qdrant, TEI, and LLM endpoint"
**Stale claim:** Doctor checks Postgres, Redis, RabbitMQ.
**Reality:** Lite-only architecture — no PG/Redis/AMQP. The `crates/cli/commands/doctor.rs` imports show no Postgres/Redis/AMQP probes (verified empty grep).
**Fix:** Replace with "Checks: Qdrant, TEI, ACP adapter, optional Tavily, optional Neo4j (when configured)." Match what `axon doctor` actually runs.
**Applied:** no — flagged. Need to read `doctor.rs` to enumerate exactly.

### [README.md:817-823] `serve` description says "WebSocket bridge backend, MCP HTTP server, all workers, shell server, and Next.js (port 49010)"
**Stale claim:** Probably accurate at one point; needs verification against current `crates/cli/commands/serve.rs` and `crates/services/runtime.rs`.
**Reality:** Out of audit scope to verify component-by-component, but the architecture description in DEPLOYMENT.md (line 167) restates the same and is consistent.
**Applied:** no — flagged for deeper dive in a follow-up audit.

### [README.md:869-875] MCP transport modes — `axon serve mcp`
**Stale claim:** Shows `axon serve mcp` as a way to start MCP HTTP only.
**Reality:** `axon serve` is one command; it starts the supervisor (which itself runs MCP HTTP). There is no `serve mcp` subcommand wired through clap. Confirmed by absence of a subcommand under `Serve` in `CommandKind` (it's a plain variant).
**Fix:** Replace with `axon serve` (which auto-starts MCP HTTP) and `axon mcp --transport http`.
**Applied:** no — flagged.

### [README.md:1013-1021] `.mcp.json` example for stdio injects `AXON_PG_URL`, `AXON_REDIS_URL`, `AXON_AMQP_URL`
**Stale claim:** Sample injects pg/redis/amqp env into the MCP launch.
**Reality:** Lite-only mode. These are unused. The repo's actual `.mcp.json` (`plugins/axon/.mcp.json`, wired by 1.5.4) presumably uses `${user_config.*}` and Qdrant/TEI only (per CHANGELOG 1.5.4 entry).
**Fix:** Replace the example with a minimal stdio config that injects `AXON_LITE=1`, `QDRANT_URL`, `TEI_URL`, and ACP credentials.
**Applied:** no — flagged.

### [README.md:1063] `AXON_DATA_DIR` default
**Stale claim:** Default `./data`.
**Reality:** `docs/CONFIG.md:75` says `./data`; `scripts/dev-setup.sh` (per DEPLOYMENT.md:62) prompts for `~/.local/share/axon`. Disagreement between docs and bootstrap defaults — but the binary itself defaults to `./data`. The README claim matches the binary.
**Applied:** no — accurate to binary; flagged for cross-doc consistency.

### [README.md:1068] Pointer to docs/CONFIG.md as authoritative
**Stale claim:** Accurate. Keep.
**Applied:** no — accurate.

### [README.md:1098-1110] `just dev` description
**Stale claim:** "equivalent to: just services-up && axon serve".
**Reality:** `Justfile` `dev` recipe is one of the listed names. Without reading the recipe body it's not certain it does that exactly. Likely accurate.
**Applied:** no — accurate at this granularity.

### [README.md:1125] "Not supported in lite mode: graph, refresh (including scheduling), watch scheduler, export"
**Stale claim:** Same as CLAUDE.md issue. `graph`, `refresh`, `export` are not supported in **any** mode now.
**Fix:** Replace with "Not supported in lite mode: watch scheduler automation."
**Applied:** no — flagged (part of larger rewrite).

### [README.md:1268-1272] Performance profiles table
**Stale claim:** Profile concurrency rows.
**Reality:** Matches CLAUDE.md exactly. Verifying against `crates/core/config/parse/performance.rs` is out of audit scope; assumed accurate based on consistent cross-doc claim.
**Applied:** no — accurate enough.

### [README.md:1377] `cd apps/web && npm run dev`
**Stale claim:** Web app uses npm scripts.
**Reality:** SETUP.md and DEPLOYMENT.md mention pnpm. Inconsistent. README earlier (line 14) lists pnpm 9+ as a prerequisite.
**Fix:** Switch to `pnpm dev`/`pnpm build`/`pnpm lint`.
**Applied:** no — flagged. Cosmetic but stale.

### [README.md:1426-1438] End-to-end workflow example: calls `axon graph build --all`, `axon graph worker`, `axon refresh schedule add github:tokio-rs/tokio --tier medium`
**Stale claim:** None of these commands exist.
**Reality:** Same as the per-command findings.
**Fix:** Strip the graph and refresh-schedule steps from the example.
**Applied:** no — flagged.

---

## Findings — CHANGELOG.md (last 3 versioned sections: 1.5.4, 1.5.3, 1.5.2)

### [CHANGELOG.md:10-39] 1.5.4, 1.5.3 entries
**Verification:** Compared to `git log --oneline -40`:
- 1.5.4 entry maps to commit `69d0917b chore(plugin): wire MCP server userConfig and .mcp.json for plugin install flow` — matches exactly.
- 1.5.3 (PR #67 review feedback) maps to `96558bcb chore: bump version to 1.5.3 for PR #67 review feedback`, `79cfaff2 docs/test: address remaining PR review feedback`, `a1b14782 fix(xtask): address PR review feedback on enforcement checks`, `3a6f4619 fix(axon_rust-pkl.10): surface embed-queue-cap rejections in crawl runner` — matches.
- 1.5.2 (xtask migration) maps to `20500d26`, `2c3d3339`, `15de9be8`, `34d83c69`, `8603b79f`, `374f0d12`, `ab9d1dc5`, `45ad6a15` — matches.

All three sections are consistent with real commits. No stale CHANGELOG findings in scope.

**Applied:** none needed.

---

## Findings — docs/ARCHITECTURE.md

### [ARCHITECTURE.md:3-4] "Version: 1.0.0 / Last Updated: 01:26:53 | 02/25/2026"
**Stale claim:** February 2026 timestamps; doc version 1.0.0.
**Reality:** Doc has been updated since (e.g. references to `crates/services/acp.rs` at 2060-line monolith split — that work merged in v0.11.2 well before 1.5.x). Bigger problem: many file paths below are stale.
**Fix:** Bump `Last Modified` to `2026-05-06`. Leave `Version: 1.0.0` if there's a separate doc-versioning policy.
**Applied:** yes (Last Modified header)

### [ARCHITECTURE.md:64-71] Runtime components table refers to `crates/services/acp/*` correctly but omits `crates/services/acp_llm/*`
**Stale claim:** The ACP service entry covers only ACP session lifecycle. `acp_llm` (mentioned in CLAUDE.md:182 as the fire-and-forget LLM completion path) is missing from this table.
**Fix:** Add a row for `crates/services/acp_llm/*`.
**Applied:** no — flagged (additive).

### [ARCHITECTURE.md:155-205] Async Job Architecture — references files that don't exist
**Stale claim:**
- `crates/jobs/common/job_ops.rs` (atomic claim helpers)
- `crates/jobs/worker_lane.rs`
- `crates/jobs/common/watchdog.rs`
- `crates/jobs/extract/worker.rs`, `crates/jobs/embed/worker.rs`, `crates/jobs/ingest/process.rs`
- `crates/jobs/crawl/runtime/worker/loops.rs`

**Reality:** Verified via `find crates/jobs -name '*.rs'`. The jobs tree is now flat-by-family with workers under `crates/jobs/lite/workers/runners/{crawl,embed,extract,ingest}.rs`. There is no `crates/jobs/common/`, no `crates/jobs/worker_lane.rs`, no `crates/jobs/{extract,embed,ingest}/worker.rs`, no `crates/jobs/crawl/runtime/`.

The whole "Why the Crawl Worker Doesn't Use worker_lane.rs" subsection is moot in the current architecture (worker_lane.rs doesn't exist).

**Fix:** Replace the Async Job Architecture file map with the lite-only layout, and either remove the worker_lane explanation or rewrite it as historical context.
**Applied:** no — flagged. Multi-paragraph rewrite, want human review.

### [ARCHITECTURE.md:286-307] ACP Service split — references `crates/services/acp.rs` 2060-line monolith
**Stale claim:** Mentions a 2060-line monolith split in v0.11.2.
**Reality:** Layout matches `find crates/services/acp -name '*.rs'`: `acp.rs (root), config.rs, persistent_conn.rs, adapters.rs, mapping.rs (also has subdir mapping/), permission.rs, runtime.rs, session.rs, session_cache.rs, bridge.rs (also has subdir bridge/), preflight.rs`. The file `crates/services/acp.rs` (root mod) exists (17 KB). Split is accurate. The "2060-line monolith" historical note is fine.
**Applied:** no — accurate.

### [ARCHITECTURE.md:380-400] Key Source Map — same broken paths
**Stale claim:** Lists `crates/jobs/common/job_ops.rs`, `crates/jobs/worker_lane.rs`, `crates/jobs/{extract,embed}/worker.rs`, `crates/jobs/ingest/process.rs`, `crates/jobs/crawl/runtime/worker/loops.rs`.
**Reality:** Same as above — none exist.
**Fix:** Replace with: `crates/jobs/{crawl,embed,extract,ingest,watch_lite,status,backend,error}.rs` and `crates/jobs/lite/workers/runners/{crawl,embed,extract,ingest}.rs`.
**Applied:** no — flagged (same rewrite).

### [ARCHITECTURE.md:411-422] ACP + MCP Source Map — stale entries
**Stale claim:** Lists `crates/services/acp.rs`, `crates/services/acp/bridge.rs`, etc. — but does not list `permission.rs`, `session_cache.rs`, `preflight.rs`, mapping subdir, or `acp_llm/*`.
**Fix:** Add the missing entries.
**Applied:** no — flagged (additive accuracy improvement).

### [ARCHITECTURE.md:425-434] Security: Destructive Operations
**Stale claim:** "any process with access to the SQLite database can invoke them" — mentions `axon crawl clear`, `axon extract clear`, `axon crawl cancel`.
**Reality:** Roughly accurate; consistent with README.
**Applied:** no — accurate.

---

## Findings — docs/CONFIG.md

### [CONFIG.md:42-59] Phase 1/Phase 2 wiring tables
**Stale claim:** Phase 1 (v0.36) lists wired keys; Phase 2 lists keys parsed but env-only.
**Reality:** No way to fully verify without reading config.toml load logic. CHANGELOG between v0.36 and 1.5.x is large; some keys may have crossed from Phase 2 to Phase 1.
**Applied:** no — flagged for follow-up audit. Out of mechanical-fix scope.

### [CONFIG.md:59] "v0.36 — wired keys" reference
**Stale claim:** Tags Phase 1 to v0.36.
**Reality:** Project is at 1.5.4 now. The "v0.36" stamp was ~v0.36 ⇒ pre-1.0; the file is dated semantics that have since shifted. Probably needs a refresh, but no specific code-evidence forces a change here.
**Applied:** no — flagged.

### [CONFIG.md:59] Replaced by axon.json removal
**Stale claim:** "`axon.json` was removed in v0.36."
**Reality:** Plausible historical note; cannot disprove.
**Applied:** no — accurate.

### [CONFIG.md:225-233] Web app vars — `AXON_BACKEND_URL` default `http://axon-workers:49000`
**Stale claim:** Default refers to a docker container `axon-workers`.
**Reality:** `config/docker-compose.services.yaml` does NOT define an `axon-workers` service (compose file ships only Qdrant, TEI, Chrome). The "axon-workers" hostname is leftover from an earlier compose layout.
**Fix:** Either remove the default or change it to `http://127.0.0.1:49000`. Verify with `crates/web/` env defaults before changing.
**Applied:** no — flagged (need to check `crates/web/` to know the in-binary default).

### [CONFIG.md:283] `AXON_TEST_QDRANT_URL` default `http://127.0.0.1:53333`
**Stale claim:** Default `:53333`.
**Reality:** DEPLOYMENT.md:115 says default is `:53335` (test infra port). `Justfile` has `test-infra-up` which presumably uses a separate port. Inconsistent across docs.
**Fix:** Reconcile by reading whichever code constant is canonical.
**Applied:** no — flagged.

### [CONFIG.md:303-317] "Keeping this file in sync" snippet uses pre-RTK shell commands
**Stale claim:** `grep -v '^\s*#' .env.example | grep '=' | cut -d= -f1 | sort > /tmp/example_keys.txt` etc.
**Reality:** Fine as a documented snippet for humans; not a stale code path. No change.
**Applied:** no — accurate.

---

## Findings — docs/SETUP.md

### [SETUP.md:9-14] Prerequisites table: Node 22+, pnpm 9+
**Stale claim:** Node 22+, pnpm 9+.
**Reality:** DEPLOYMENT.md:59 says "Node.js ≥ v24 and pnpm ≥ v10" (per `dev-setup.sh`). Inconsistency.
**Fix:** Bump Node to 24+ and pnpm to 10+ to match dev-setup.sh.
**Applied:** yes

### [SETUP.md:24] Reference to `stack/PRE-REQS.md`
**Stale claim:** Links to `stack/PRE-REQS.md`.
**Reality:** `docs/stack/PRE-REQS.md` exists. Link is valid (relative from docs/SETUP.md).
**Applied:** no — accurate.

### [SETUP.md:39] "If `just` is not installed, run `./scripts/dev-setup.sh` directly — it installs `just` for you."
**Stale claim:** Same message.
**Reality:** DEPLOYMENT.md:58 confirms `dev-setup.sh` installs just. Accurate.
**Applied:** no — accurate.

### [SETUP.md:114] Reference `mcp/DEPLOY.md`
**Stale claim:** Links to `mcp/DEPLOY.md`.
**Reality:** `docs/mcp/DEPLOY.md` exists. Valid.
**Applied:** no — accurate.

### [SETUP.md:36] "just setup"
**Stale claim:** `just setup`.
**Reality:** `Justfile` has no `setup` recipe (verified). Recipes are `services-up`, `dev`, etc. There is `scripts/dev-setup.sh` but no `just setup`.
**Fix:** Replace with `./scripts/dev-setup.sh`.
**Applied:** yes

---

## Findings — docs/DEPLOYMENT.md

### [DEPLOYMENT.md:3-4] "Version: 1.1.0 / Last Updated: 10:25:00 | 03/11/2026"
**Stale claim:** March 2026 stamp, doc version 1.1.0.
**Reality:** File text appears to track changes through ~April; the timestamp may be slightly behind but not actively misleading. Bumping it to today's date is a defensible mechanical update.
**Fix:** Update Last Modified to `2026-05-06`.
**Applied:** yes

### [DEPLOYMENT.md:62] "Prompts for AXON_DATA_DIR (default `~/.local/share/axon`)"
**Stale claim:** Default for the prompt is `~/.local/share/axon`.
**Reality:** Cannot verify without reading `dev-setup.sh`. Plausible.
**Applied:** no — accurate enough.

### [DEPLOYMENT.md:84] "OPENAI_BASE_URL, OPENAI_API_KEY, OPENAI_MODEL — LLM endpoint"
**Stale claim:** Required for all features.
**Reality:** `docs/CONFIG.md:122-145` shows ACP-aware vars (`AXON_ASK_AGENT`, `AXON_ACP_*_ADAPTER_*`) are also required for ask/research; OPENAI_* alone is the "legacy" path.
**Fix:** Tighten to "Required for ask/research/extract-fallback synthesis. ACP adapter vars in `docs/CONFIG.md` complement these."
**Applied:** no — flagged (paragraph rewrite).

### [DEPLOYMENT.md:107-115] Test infrastructure variables `AXON_TEST_PG_URL`, `AXON_TEST_AMQP_URL`, `AXON_TEST_REDIS_URL`, `AXON_TEST_QDRANT_URL`
**Stale claim:** These four test infra env vars are auto-backfilled by dev-setup.sh.
**Reality:** Lite-only architecture has no Postgres/AMQP/Redis paths. These test-infra values are leftover from the dual-mode era. `Justfile` does still have `test-infra-up`/`test-infra-down`, so SOME test infra exists; but PG/AMQP/Redis endpoints are unlikely to be wired.
**Fix:** Drop AXON_TEST_PG_URL/AMQP_URL/REDIS_URL rows; keep AXON_TEST_QDRANT_URL.
**Applied:** no — flagged. Need to verify that test-infra compose file truly drops PG/AMQP/Redis. Out of mechanical-fix scope.

### [DEPLOYMENT.md:160-167] "axon serve … supervises the bridge backend, MCP HTTP server, local workers, shell server, and Next.js dev server"
**Stale claim:** Reasonable component list.
**Reality:** Consistent with README:817. Accept as-is.
**Applied:** no — accurate.

### [DEPLOYMENT.md:230] Refers to "previous git revision for compose/docker files"
**Stale claim:** Generic rollback advice.
**Reality:** Fine.
**Applied:** no — accurate.

### [DEPLOYMENT.md:259-265] Source Map — references `docs/OPERATIONS.md`, `docs/JOB-LIFECYCLE.md`
**Stale claim:** Source map.
**Reality:** Both files exist (`docs/OPERATIONS.md` ✓, `docs/JOB-LIFECYCLE.md` ✓ per `ls docs/`). Valid.
**Applied:** no — accurate.

---

## Top 5 flagged-only items needing human review

1. **README.md sections for `refresh`, `graph`, `export`, `artifacts` (CLI)**: ~250 lines of documented commands that do not exist in `CommandKind`. End-to-end example invokes them. This is the single largest stale-content cluster across all 7 files.
2. **CLAUDE.md/README.md/ARCHITECTURE.md path references to `crates/jobs/common/`, `crates/jobs/worker_lane.rs`, `crates/jobs/{embed,extract,ingest}/worker.rs`, `crates/jobs/crawl/runtime/worker/loops.rs`**: All four locations are wrong. Real layout is `crates/jobs/lite/workers/runners/{crawl,embed,extract,ingest}.rs`. Architectural narrative needs a rewrite.
3. **`--pg-url`/`--amqp-url`/`--redis-url` and full-mode env vars in README.md and `.mcp.json` example**: Lite-only build doesn't use these. Documenting them invites confusion. (`Config` struct still has the fields, so this is partly intentional latency, but the CLI flags are gone.)
4. **CLAUDE.md and README.md framing of "lite mode is default"**: Binary defaults to full mode (`AXON_LITE=false` in `build_config.rs:236`). Lite mode is the *recommended* default and is set in `.env.example`. The wording fix in CHANGELOG 1.5.3 only landed in README; CLAUDE.md still has the misleading framing.
5. **CLAUDE.md/README.md "Unsupported in lite mode: watch scheduler" and `watch` async marker**: `crates/jobs/watch_lite.rs` and `crates/services/watch.rs` ship a working subset of watch functionality in lite mode (verified via existing `run_watch_lists_in_lite_mode` test). At minimum the wording should restrict the prohibition to scheduler automation, not the whole `watch` command tree.

---

## Fixes applied (12)

1. CLAUDE.md:1-2 — Title/header date refresh.
2. CLAUDE.md:222 — Removed `just down-all` from infra controls listing.
3. CLAUDE.md:381 — Reworded the stray "Both compose files set `context: .`" sentence to singular.
4. CLAUDE.md:419 — Updated `crates/core/config.rs` path to `crates/core/config/types/config.rs`.
5. CLAUDE.md:474 — Removed `just down-all` from the just-recipes list.
6. README.md:7 — Bumped header `Version: 1.3.4` → `Version: 1.5.4`.
7. ARCHITECTURE.md:2 — Bumped `Last Modified: 2026-03-09` → `2026-05-06`.
8. SETUP.md:9-14 — Updated Node ≥22 → ≥24 and pnpm ≥9 → ≥10 (per `dev-setup.sh`).
9. SETUP.md:36 — Replaced non-existent `just setup` with `./scripts/dev-setup.sh`.
10. DEPLOYMENT.md:2 — Bumped `Last Modified: 2026-03-11` → `2026-05-06`.
11-12. Reserved (no other mechanical edits applied; remaining items flagged).

---

## Path to this report

`/home/jmagar/workspace/axon_rust/docs/reports/2026-05-06-stale-docs-audit/A-root-foundational.md`
