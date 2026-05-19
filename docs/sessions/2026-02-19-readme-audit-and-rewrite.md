# Session: README Audit and Full Rewrite
**Date:** 2026-02-19
**Branch:** `chore/housekeeping`
**Duration:** ~25 minutes

---

## Session Overview

Deployed a 4-agent haiku explorer team (`arch-explorer`, `cli-explorer`, `docker-explorer`, `schema-explorer`) to audit the existing README.md against the actual codebase in parallel. Each agent covered an independent domain. Synthesized all findings into a complete README rewrite that is now factually accurate.

---

## Timeline

| Time | Activity |
|------|----------|
| Start | Invoked `superpowers:dispatching-parallel-agents` skill |
| +2m | Created `readme-audit` team; created 4 TaskCreate entries |
| +3m | Dispatched all 4 haiku agents in parallel |
| +8m | `cli-explorer` reported (flags and commands) |
| +10m | `docker-explorer` reported (services and infra) |
| +12m | `arch-explorer` reported (crate layout and binary name) |
| +14m | `schema-explorer` reported (schema, env vars, gotchas) |
| +16m | Verified key findings directly (Cargo.toml, docker-compose.yaml, .env.example, Glob) |
| +20m | Wrote full README rewrite |
| +22m | Shut down team and deleted team resources |

---

## Key Findings

### Critical (user-facing bugs in the old README)

1. **Binary name wrong**: README claimed `cortex` (primary) and `axon` (alias). Actual `Cargo.toml:9-11` has only one binary: `name = "axon"`. No `cortex` binary exists.
2. **`--collection` default wrong**: README said `spider_rust`. Actual code (`config.rs:347`) and `.env.example:46` default to `cortex`.
3. **`--max-pages` default wrong**: README said `200`. Actual code (`config.rs:258`) defaults to `0` (uncapped).
4. **AMQP fallback credentials wrong**: README showed `guest:guest`. Actual code fallback is `axon:axonrabbit` (consistent with `docker-compose.yaml:63-64`).
5. **Docker network name wrong**: README claimed `cortex`. Actual `docker-compose.yaml:157` uses `axon`.
6. **Redis image version wrong**: README said `redis:7.4-alpine`. Actual `docker-compose.yaml:44` uses `redis:8.2-alpine`.

### Missing Structures (entirely absent from old README)

7. **`axon-webdriver` service**: `selenium/standalone-chrome:4.34.0`, ports `127.0.0.1:4444:4444` (WebDriver) and `127.0.0.1:7900:7900` (VNC). A full Docker service not mentioned at all.
8. **`crates/jobs/crawl_jobs/`**: 8-file subdirectory (`config.rs`, `manifest.rs`, `processor.rs`, `repo.rs`, `sitemap.rs`, `watchdog.rs`, `worker.rs`, `mod.rs`) — v2 crawl pipeline.
9. **`crates/vector/ops/`**: 7-file subdirectory (`commands.rs`, `input.rs`, `mod.rs`, `qdrant.rs`, `ranking.rs`, `stats.rs`, `tei.rs`) — v2 vector ops.
10. **`crates/jobs/common.rs`**: Shared AMQP infrastructure (`make_pool`, `open_amqp_channel`, `claim_next_pending`, `mark_job_failed`, `enqueue_job`).
11. **`crates/jobs/crawl_jobs_dispatch.rs`**: Dispatcher routing v1/v2 crawl pipeline.
12. **`docker/s6/s6-rc.d/`**: The actual s6-rc service tree (not `services.d` as documented).
13. **21 undocumented CLI flags**: Chrome browser options (`--chrome-*`), cache flags, cron scheduling, watchdog tuning, `--url-glob`, `--exclude-path-prefix`, `--start-url`.

### Wrong Claims Removed

14. **`run_embed_and_save()` in `common.rs`**: Does not exist. Actual functions in `common.rs` are URL parsing utilities (`parse_urls`, `expand_url_glob_seed`, etc.).
15. **"2 lanes for higher throughput"**: Not reflected in `docker-compose.yaml` or worker config.
16. **Performance profiles table**: Missing `Backfill concurrency` column — three concurrency axes exist (crawl, sitemap, backfill).
17. **`debug` command**: Existed in code (`crates/cli/commands/debug.rs`) but absent from README commands table.
18. **`recover` subcommand**: Existed as `JobSubcommand::Recover` but absent from README job subcommands list.

### Accurate (verified, no changes needed)

- All 4 database schemas (`axon_crawl_jobs`, `axon_batch_jobs`, `axon_extract_jobs`, `axon_embed_jobs`) matched README exactly.
- All gotchas (chunk size 2000/200 overlap, TEI batch 64/128, `ensure_collection` uses PUT, auto-switch 60% threshold, ask URL pattern) were correct.
- All 12 core env vars present in `.env.example` as documented.
- `crawl/engine.rs` function list was accurate.
- `vector/ops.rs` function list was accurate.

---

## Technical Decisions

1. **4 independent agents vs 1**: Architecture, CLI, Docker, and Schema are genuinely orthogonal domains — no agent needed another's output. Pure parallel exploration.
2. **Haiku model for explorers**: Read-only investigation tasks; haiku is fast and sufficient for Glob/Grep/Read exploration. No code generation needed.
3. **Verified findings directly**: After agents reported, re-read `Cargo.toml`, `docker-compose.yaml`, `.env.example`, and ran Glob queries to confirm agent findings before writing. Facts only in the README.
4. **Kept README structure**: Preserved the existing section ordering and table style — changed content, not format.
5. **Documented all 21 missing flags**: Rather than a selective subset, added new grouped sections (Browser/WebDriver, Cache, Cron, Watchdog) to the Global Flags Reference. Completeness matters for a reference document.

---

## Files Modified

| File | Purpose |
|------|---------|
| `README.md` | Complete rewrite based on audit findings |

No source code was modified. This was a documentation-only session.

---

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| Binary name | `cortex` (wrong) | `axon` (correct) |
| Default collection | `spider_rust` (wrong) | `cortex` (correct) |
| Default max-pages | `200` (wrong) | `0` / uncapped (correct) |
| AMQP fallback creds | `guest:guest` (wrong) | `axon:axonrabbit` (correct) |
| Docker network | `cortex` (wrong) | `axon` (correct) |
| Redis image | `redis:7.4-alpine` (wrong) | `redis:8.2-alpine` (correct) |
| axon-webdriver service | Not documented | Documented with ports/image |
| `debug` command | Not in table | Added to commands table |
| `recover` subcommand | Not listed | Added to job subcommands |
| v2 crate subdirs | Not documented | `crawl_jobs/` and `ops/` documented |
| Performance profiles | Missing backfill column | Full 3-column concurrency table |
| Chrome/browser flags | 0 documented | 11 flags documented |
| Watchdog/cron flags | 0 documented | 4 flags documented |
| Env var reference | Partial | Complete with grouping and defaults |

---

## Verification Evidence

| Claim | Source | Status |
|-------|--------|--------|
| Only `axon` binary | `Cargo.toml:9-11` | Confirmed |
| `--collection` default `cortex` | `config.rs:347`, `.env.example:46` | Confirmed |
| `--max-pages` default `0` | `config.rs:258` | Confirmed |
| AMQP creds `axon:axonrabbit` | `docker-compose.yaml:63-64` | Confirmed |
| Network name `axon` | `docker-compose.yaml:157` | Confirmed |
| Redis `8.2-alpine` | `docker-compose.yaml:44` | Confirmed |
| `axon-webdriver` service exists | `docker-compose.yaml:99-118` | Confirmed |
| `crawl_jobs/` dir exists | Glob `crates/jobs/**/*.rs` | Confirmed (15 files) |
| `ops/` dir exists | Glob `crates/vector/**/*.rs` | Confirmed (10 files) |
| `common.rs` is URL utilities | Agent read `commands/common.rs` functions | Confirmed |
| All 4 DB schemas accurate | Agent read `*_jobs.rs` CREATE TABLE stmts | Confirmed |
| All gotchas accurate | Agent read `ops.rs:386`, `engine.rs:251`, etc. | Confirmed |

---

## Source IDs + Collections Touched

Axon services not running at session start (startup hook reported: "Axon services not running"). Embed/retrieve attempted after session file written — see results below.

---

## Risks and Rollback

- **Risk**: README is now accurate to the codebase as of `chore/housekeeping` branch. If `CLAUDE.md` (project instructions) is not updated separately, it still contains stale `cortex` binary references and the old `--collection spider_rust` default. These are documentation-only risks.
- **Rollback**: `git checkout HEAD -- README.md` restores the previous (inaccurate) README.

---

## Decisions Not Taken

| Alternative | Reason Rejected |
|------------|----------------|
| Update `CLAUDE.md` in same session | Scope was README only; CLAUDE.md update should be a separate reviewed change |
| Skip documenting all 21 missing flags | Completeness is the goal — a reference doc with known omissions is a known liability |
| Single sequential agent | Would take 4× longer for genuinely independent domains |

---

## Open Questions

1. **CLAUDE.md references `cortex` binary** in multiple places (`cargo build --release --bin cortex`, Quick Start, etc.). Needs a follow-up PR to align.
2. **`crates/cli/commands/probe.rs`** is a utility module, not a command. Should it be documented in the architecture section? Left out of commands table (correct) but not in crate layout tree.
3. **`axon-workers` resource limits** (4 CPU / 4 GB) — not mentioned in README Worker Model section. Could be worth documenting for capacity planning.
4. **`crawl_jobs/` vs `crawl_jobs.rs`** — dispatcher routes between them based on what logic? The dispatch criteria isn't documented anywhere in the README.
5. **`axon main.rs` vs `axon_main.rs`** — arch-explorer noted an `axon_main.rs` at root. Is this a duplicate/alternate entry point? Not in Cargo.toml, so probably dead code or an artifact.

---

## Next Steps

1. Follow-up PR: update `CLAUDE.md` to remove all `cortex` binary references, fix `--collection spider_rust` default.
2. Investigate `axon_main.rs` at repo root — determine if it's dead code.
3. Document `crawl_jobs/` dispatch criteria (what triggers v2 vs v1 pipeline) in README or a separate architecture doc.
4. Consider adding `axon-workers` CPU/memory limits to the Worker Model section.
