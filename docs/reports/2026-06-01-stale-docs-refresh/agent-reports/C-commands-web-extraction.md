# Agent C — web & extraction command docs report

**Date:** 2026-06-01
**Lane:** `docs/commands/{scrape,crawl,map,endpoints,search,research,extract,screenshot}.md`
**Ground truth:** `docs/reports/2026-06-01-stale-docs-refresh/ground-truth/axon-<cmd>--help.txt` + clap in `src/core/config/cli.rs`, `src/core/config/cli/global_args.rs`, plus handler/service source.

## Method note (important for other agents)

The per-command `--help` dumps are **terse** — they list only the flags declared directly on each subcommand's `Args` struct. Nearly every flag the docs document (`--max-depth`, `--output-dir`, `--format`, `--collection`, `--json`, `--wait`, `--research-depth`, `--search-time-range`, `--viewport`, `--screenshot-full-page`, `--url-glob`, etc.) is an `#[arg(global = true)]` flag in `global_args.rs` and is valid on every subcommand even though it does **not** appear in the per-command help. I verified "missing" flags against `global_args.rs` before assuming removal — none were actually removed. Do not delete documented global flags just because the per-command help omits them.

## Files reviewed

- `docs/commands/scrape.md` — accurate (no edits)
- `docs/commands/crawl.md` — accurate (no edits; `--max-depth 10` and `audit`/`diff` subcommands are correct)
- `docs/commands/map.md` — accurate (no edits)
- `docs/commands/endpoints.md` — minor fixes (added missing flags table + RPC-probe layer)
- `docs/commands/search.md` — major fixes (behavior changed: now auto-enqueues crawl jobs)
- `docs/commands/research.md` — minor fix (`--research-depth` semantics were wrong)
- `docs/commands/extract.md` — minor fixes (`--query` is not parser-enforced)
- `docs/commands/screenshot.md` — accurate (no edits)

## Fixes made

1. **search.md — behavioral regression (biggest accuracy problem).** The doc claimed "`search` is synchronous and does not enqueue a background job" and "`search` does not enqueue crawl jobs." This is now **false**: `run_search` (`src/cli/commands/search.rs:43`) calls `search_and_crawl()` (`src/services/search_crawl.rs`), which "enqueue[s] one bounded crawl job per result URL," emits `crawl_jobs`/`crawl_jobs_rejected`/`auto_crawl_status`, and **errors if results were found but none could be queued** (`search.rs:66-76`). Rewrote the summary line, the Output section (now lists the real `--json` payload keys: `query`, `limit`, `offset`, `search_time_range`, `results`, `auto_crawl_status`, `crawl_jobs`, `crawl_jobs_rejected`), and the Behavior Notes to describe auto-enqueue + async crawl lifecycle. (Note: the 2026-05-06 audit declared search.md "verified clean" on this exact point — the code changed since then.)

2. **research.md — wrong flag semantics.** `--research-depth` was documented as "Crawl depth limit for the research pass." Per `src/cli/commands/research.rs:92-95` and `global_args.rs:168-172`, it is the **number of sources the LLM synthesizes over**; it overrides `--limit` when set and falls back to `--limit` (default 10) when unset, capped with `--offset` at 100 (Tavily window). Corrected the table cell. Left the "does not enqueue jobs / does not auto-embed" line intact — `research.rs` does **not** call `search_and_crawl`, so research genuinely differs from search and that line is still correct.

3. **endpoints.md — missing flags.** The doc had no flags table and omitted `--probe-rpc` and `--probe-rpc-subdomains` (real per ground-truth help, clap `EndpointArgs` lines 355-360, and commit 45ade5f0). Added a full flags table with verified defaults (`--include-bundles=true`, `--first-party-only=false`, `--unique-only=true`, `--max-scripts=40`, `--max-scan-bytes=8388608`, `--verify`, `--capture-network`, `--probe-rpc`, `--probe-rpc-subdomains`), a "Layer 4" protocol-probing description, two example invocations, and the `rpc_probe` / `mcp_candidates` output fields (`src/cli/commands/endpoints.rs:103-128`).

4. **extract.md — overstated requirement.** Doc said "`--query <prompt>` is required for both async and sync extraction." The binary does **not** enforce it: `extract.rs:70` uses `cfg.query.clone().unwrap_or_default()` and an empty prompt becomes `None`, falling back to deterministic parsers only (lines 206-209). Only the URL is enforced (`extract.rs:59-61`). Softened the Required Inputs section and the flag-table cell to "recommended" with the empty-prompt behavior spelled out.

## Gaps / missing docs (for Phase 2)

- **`axon diff` — no command doc.** Real command (`DiffArgs` in `cli.rs:211-219`, ground-truth `axon-diff--help.txt`). Takes two positional URLs `<URL_A> <URL_B>`, shows content/metadata/link changes. Needs a `docs/commands/diff.md` (synopsis, args, `--json`, behavior). NOT created per brief.
- **`axon brand` — no command doc.** Real command (`Brand(ScrapeArgs)` in `cli.rs:48-49`, ground-truth `axon-brand--help.txt`). Extracts brand identity (colors, fonts, logos, favicon) from a URL. Needs a `docs/commands/brand.md`. NOT created per brief.
- Both are listed under "Web And Extraction" in `axon --help` and fall squarely in this lane's domain.

## Reorg observations (for Phase 2)

- The four "fetch" docs (scrape, crawl, extract, screenshot) carry a stale `Version: 1.0.0 / Last Updated: 20:29:46 | 03/03/2026 EST` header block that is meaningless and inconsistent with the rest of the tree (most docs use only `Last Modified:`). Recommend dropping the `Version`/`Last Updated` lines repo-wide during reorg.
- `endpoints.md` uses `# endpoints` as its title while every sibling uses `# axon <cmd>`. Normalize.
- `map.md` synopsis duplicates the same line twice (lines 9-10 are identical) — trivial cleanup.

## Cross-reference notes (links + code→doc references)

- research.md and extract.md both link to `../CONFIG.md` (relative). Valid today; reorg should preserve.
- **Stale claims OUTSIDE my lane (report only, did not edit):**
  - Root `CLAUDE.md` "Commands" table and `src/core/CLAUDE.md` say `--max-depth` default is `5`. The real default is **`10`** (`global_args.rs:14`). crawl.md correctly says 10 — do not change it down.
  - Root `CLAUDE.md` documents a `--embed <bool>` global flag (default `true`). The real flag is **`--skip-embed`** (`global_args.rs:65-67`); there is no `--embed`. My docs correctly use `--skip-embed`.
  - The `axon --help` **banner** (`src/core/config/help.rs`, reflected in ground-truth `axon--help.txt` lines 11/16) says `--collection` "(default cortex)" and a Quick-Start example uses `--collection cortex`. The actual clap default is **`axon`** (`global_args.rs:69-77`, `AXON_COLLECTION`). The help banner prose is stale. None of my 8 docs assert a collection default, so no edit was needed in-lane; flagging for the agent who owns root `CLAUDE.md` / help-banner.
  - Root `CLAUDE.md` line ~38 still says `search` "auto-queues crawl jobs for results" — this is now **correct** (it was the stale party in the 2026-05-06 audit, but the code caught up). search.md is the doc that was stale and is now fixed.
