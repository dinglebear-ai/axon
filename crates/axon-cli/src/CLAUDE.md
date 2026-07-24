# axon-cli — Agent Guide

`axon-cli` owns the human command-line **transport**: clap parsing, help text,
progress display, human/JSON rendering, and process exit codes. It converts argv
into `axon-api` request DTOs, calls `axon-services`, and renders the result —
nothing more. Full contract (owns / API / deps / tests):
[../../../docs/pipeline-unification/crates/axon-cli/README.md](../../../docs/pipeline-unification/crates/axon-cli/README.md)
· surface spec:
[../../../docs/pipeline-unification/surfaces/command-contract.md](../../../docs/pipeline-unification/surfaces/command-contract.md)
· help text:
[../../../docs/pipeline-unification/surfaces/axon-help.md](../../../docs/pipeline-unification/surfaces/axon-help.md).

## Status — live crate, Phase 10 surface cutover already applied
`embed`, `ingest`, `crawl`, `code-search`, `code-search-watch`, `purge`,
`dedupe`, `refresh`, and `fresh` are already **removed** from the clap tree
(not aliased) — verified against the live binary, they do not appear in the
`Command` enum in `crates/axon-core/src/config/cli.rs`. `scrape` is retained as
a canonical one-page SourceRequest projection. The target `axon <source>` grammar is
implemented: the parser (`route_bare_source` in
`crates/axon-core/src/config/source_routing.rs`) routes any first positional
that is not a canonical/removed command or global flag to the `source`
subcommand (a `SourceRequest`). Do not add back-compat aliases for removed
commands.

## Module map
Current groups from `crates/axon-cli/src/`:
| Area | Owns |
|---|---|
| `lib.rs` | `run` / `run_once` entrypoints + top-level dispatch |
| `commands.rs` + `commands/` | per-command handlers (argv → request DTO → service call) |
| clap args / render / progress / json / exit | parser tree, human renderers, progress, `--json` envelope, exit-code mapping (arg/render/progress/json helpers live under `commands/` and `axon-core`; `app.rs`/`args.rs`/`exit.rs`/`help.rs` are not yet split out) |
| `*_tests.rs` sidecars | CLI tests (e.g. `json_tests.rs`) — `_tests.rs` sidecar convention, no dedicated `testing.rs` |

## Boundary — keep OUT of this crate
- Source pipeline logic, provider/store/domain internals — always go through `axon-services`.
- Duplicate DTOs or an alternate job/status model — reuse `axon-api`.
- MCP/REST compatibility aliases; ad-hoc stdout emitted from lower crates.

## Dependencies
- **Allowed:** `axon-api`, `axon-error`, `axon-core`, `axon-authz`, `axon-observe`, `axon-services`, clap + terminal rendering crates; optionally `axon-web`/`axon-mcp` only to bootstrap the `serve`/`mcp` subcommands.
- **Forbidden:** domain crate internals bypassing services, direct provider/store clients, compat aliases for removed commands. Enforced by `cargo xtask check-layering`.

## Invariants (review checklist)
- `axon <source>` is the default pipeline command; `ask`/`query`/`retrieve`/`search` keep clear, non-overlapping semantics.
- Every command maps to exactly one service request/result path — the CLI is a transport, not the pipeline owner.
- `--json` emits the shared `axon-api` envelope; human progress renders from shared `axon-observe` progress events.
- No removed command (`embed`/`ingest`/`crawl`/`code-search`/`code-search-watch`/`purge`/`dedupe`/`refresh`/`fresh`) survives in help, completions, or the parser after the clean break.

## DTO ownership
Wire DTOs (`SourceRequest`/`SourceResult`, `AskRequest`, `QueryResult`, the
shared JSON envelope, …) live in **`axon-api`**; this crate constructs and
renders them. Transports call `axon-services`/`axon-api`, never a domain crate's
`::ops::*` or internals.

## Keep in sync when shapes change
`README.md` (crate contract) · `surfaces/command-contract.md` ·
`surfaces/axon-help.md` · `schemas/cli-schema.md` · the request/result and
envelope DTOs in `axon-api`.
