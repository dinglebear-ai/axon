# Agent D — vector/RAG + ops/setup command docs report

Domain: `docs/commands/` for the vector/RAG commands, the ops/setup commands, and
the commands index README. Verified against the v4.16.0 CLI help dumps in
`ground-truth/`, the clap defs in `src/cli/commands/*` + `src/core/config/cli.rs` +
`src/core/config/cli/global_args.rs`, and behavior in `src/vector/`, `src/services/`.

## Files reviewed

| File | Verdict |
|------|---------|
| docs/commands/embed.md | minor fixes |
| docs/commands/query.md | minor fixes |
| docs/commands/retrieve.md | minor fixes |
| docs/commands/ask.md | minor fixes |
| docs/commands/evaluate.md | minor fixes |
| docs/commands/summarize.md | accurate (verified; no content change) |
| docs/commands/suggest.md | minor fixes |
| docs/commands/sources.md | minor fixes |
| docs/commands/domains.md | minor fixes |
| docs/commands/stats.md | minor fixes |
| docs/commands/dedupe.md | minor fixes |
| docs/commands/migrate.md | accurate (date only) |
| docs/commands/status.md | minor fixes |
| docs/commands/doctor.md | minor fixes |
| docs/commands/debug.md | accurate (date only) |
| docs/commands/serve.md | accurate (already rewritten since prior audit) |
| docs/commands/setup.md | minor fixes |
| docs/commands/completions.md | accurate (date only) |
| docs/commands/README.md | major fixes (index incomplete) |

## Fixes made

**Default collection `cortex` → `axon` (biggest accuracy problem).** Every
`--collection` row across these docs claimed default `cortex`. The real default is
`axon` — verified in `src/core/config/types/config_impls.rs:55` (`collection:
"axon"`), `src/core/config/cli/global_args.rs:69-77` (`default_value = "axon"`), and
`build_config.rs:71` + tests asserting `cfg.collection == "axon"`. The `cortex` seen
in the help dumps comes only from the stale `help.rs` banner string
(`help.rs:222,231`), NOT the resolved clap default. **Do not re-correct this back to
`cortex`.** Fixed in: embed.md, query.md, retrieve.md, ask.md, evaluate.md,
suggest.md, sources.md, domains.md, stats.md, dedupe.md (incl. the JSON example
`"collection": "axon"`). Added "Also settable via `AXON_COLLECTION`" where useful.

**Stale `Version:` / `Last Updated:` header lines removed** from query.md,
retrieve.md, ask.md, evaluate.md, suggest.md, sources.md, domains.md, stats.md
(boilerplate `Version: 1.0.0 / 20:30:18 | 03/03/2026 EST`). All `Last Modified`
dates bumped to 2026-06-01.

**Missing `--no-hybrid-search` flag added** to query.md and ask.md (global flag,
`global_args.rs:205-208`; present in the help dumps).

**embed.md** — `[INPUT]` arg description corrected: input can also be raw text, and
the omitted-input default is `<output-dir>/markdown` (verified
`src/services/embed.rs:143-148` joins `cfg.output_dir` + `markdown`). The prior
`.cache/axon-rust/output/markdown` literal is correct (that is the default
`output_dir`), so kept it as an example.

**sources.md** — added `--by-schema-version` global flag row (per-schema-version
chunk-count breakdown via full collection scroll; `global_args.rs:188-191`,
`src/cli/commands/sources.rs:20`). Env defaults verified against
`facets.rs` / `sources.rs` (FACET_LIMIT 100000, DOMAIN_LIMIT 10000).

**status.md** — added `--watch` live-update flag row (`global_args.rs:99-101`;
honored by status at `src/cli/commands/status.rs:34`). 4 job families + `totals` and
the 20-per-family limit (`src/services/system/status.rs:84-99`) confirmed accurate.

**doctor.md** — added the `diagnose` subcommand (`axon doctor diagnose`, shown in the
help dump). Described as equivalent to `axon debug` (inferred from the help text
"Print doctor output plus LLM diagnosis"; not independently verified).

**ask.md / evaluate.md** — added the `--since` / `--before` temporal-filter rows for
parity with query.md (global flags `global_args.rs:178-186`, whose comment names
"query/ask results"; present in both help dumps). suggest.md correctly omits them —
it is facet/prompt-context based, not semantic retrieval, so they are no-ops there.

**setup.md** — added the `setup check` subcommand (alias of `preflight`; verified
`src/cli/commands/setup.rs:38` `Some("preflight" | "check")`). All `setup init`
options verified present in `setup.rs:175-191` incl. `--auth-admin-email`.

**suggest.md** — replaced the `"refresh scheduler internals"` example (refresh
command removed) with `"watch scheduler internals"`. Tuning env defaults (250 / 500 /
50000) and the 1..100 clamp verified in `src/vector/ops/commands/suggest.rs`.

**dedupe.md** — verified the `(url, chunk_index)` dedup key and "keep newest by
`scraped_at`" against `src/vector/ops/qdrant/commands/dedupe.rs:64-94`; accurate.

**README.md (major).** The index was missing real commands and only linked the
files that exist. Kept the existing Core/Redirects/Shell structure (reorg is Phase 2).
Added a "Setup & Ops" pointer (preflight/compose/smoke are documented inside
setup.md) and a "Commands without a dedicated doc yet" section listing `brand`,
`diff`, `config`, `train`, `monitor`, `sync` with one-line descriptions taken
verbatim from their `--help` first lines. No dead links created.

## Gaps / missing docs (for Phase 2)

These subcommands exist in the v4.16.0 binary but have NO `docs/commands/<name>.md`:

- **brand** — extract a URL's brand identity (colors, fonts, logos, favicon). Should
  cover synopsis, output shape, render-mode/Chrome dependency.
- **diff** — diff two URLs, show content/metadata/link changes. Should cover the two
  positional URL args, `--json` shape, and how it relates to `crawl diff`.
- **config** — read/write `~/.axon/.env` + `~/.axon/config.toml`. Subcommands
  `list`/`get`/`set`/`unset`/`path`; auto-routing by key shape (UPPER_SNAKE → .env,
  dotted lowercase → toml); `--env`/`--toml`/`--reveal`. (Documented in root CLAUDE.md
  but no command page.)
- **train** — collect human preference votes for retrieved RAG candidates. Net-new
  RAG-eval surface; needs its own page.
- **monitor** — stream job-lifecycle events as a line-oriented feed (start/completion/
  failure/cancel). Pairs with `status`.
- **sync** — reconcile locally produced server-mode artifacts. Belongs near the
  server-mode / `AXON_SERVER_URL` docs.
- **endpoints** — has a doc file (out of my edit scope) and is linked; not a gap.

## Reorg observations (for Phase 2)

- The README "Core" list mixes web/extraction, vector/RAG, jobs, and runtime/setup.
  The binary's `--help` groups these (Web And Extraction / Vector And RAG / Jobs And
  Imports / Runtime And Setup). Phase 2 could mirror that grouping in the README.
- `setup.md` is a 4-command page (setup/preflight/compose/smoke). preflight, compose,
  and smoke are real top-level commands; consider whether they deserve their own pages
  or a clearer "ops" subsection. README now points to setup.md for them.
- `doctor`/`debug` overlap: `axon doctor diagnose` == `axon debug`. Worth a single
  cross-link rather than two near-duplicate pages.
- github.md / reddit.md / youtube.md are redirect stubs to ingest.md — fine as-is, but
  Phase 2 may fold them into an ingest-sources index.

## Cross-reference notes

- Links FROM my docs: README.md → ask/crawl/...//setup/ingest/github/reddit/youtube/
  completions/endpoints (all intra-`commands/`). retrieve.md & summarize.md reference
  MCP action behavior (no file link). ask.md references `axon research` / `axon
  evaluate` by command name (no link).
- Code→doc references noticed: `src/cli/commands/sources.rs` &
  `src/services/types/service.rs` mention `axon sources --by-schema-version` in
  comments (now documented). No code path references these doc files by path.
- Stale-help note for the orchestrator: `src/core/config/help.rs` still prints
  `--collection ... (default cortex)` and the `query "..." --collection cortex`
  example banner. That is a **code** bug (help text out of sync with the real `axon`
  default), out of scope for this doc pass — flag for a follow-up code edit.
