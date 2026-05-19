---
date: 2026-05-19 14:01:31 EST
repo: git@github.com:jmagar/axon.git
branch: main
head: 161001d9
working directory: /home/jmagar/workspace/axon_rust
worktree: /home/jmagar/workspace/axon_rust 161001d9 [main]
---

# CLI Help and Config Surface Cleanup

## User Request

The session started from noisy and incomplete `axon help` output. The user asked to make generated help reflect the real Clap command surface, remove or relocate rarely used tuning flags, replace confusing flags, and then save the final session context.

## Session Overview

- Reworked custom `axon help` so it is generated from the current Clap command surface instead of a stale hand-maintained command list.
- Removed many deep tuning options from the CLI and left them in `.env` or `~/.axon/config.toml` where appropriate.
- Replaced inverted or misleading flags with clearer opt-out names.
- Removed the remaining graph/Neo4j request surface and dead runtime code.
- Updated active command docs and `CHANGELOG.md`.
- Added regression tests so removed flags are rejected by Clap or request parsers, not merely hidden from help output.

## Sequence of Events

1. Compared the custom `axon help` output with the actual full command list and identified missing commands.
2. Changed the custom help builder to derive command rows from Clap and added focused top-level formatting.
3. Reviewed noisy inherited global flags shown under subcommand help, especially `embed help`, and removed old CLI-only tuning knobs.
4. Moved `--server-url` and `--log-level` to env-only behavior, while keeping `--local` as the explicit CLI override.
5. Removed `--start-url`, `--graph`, and old Chrome/bootstrap/intercept flags from the CLI surface.
6. Replaced `--embed <bool>` with `--skip-embed`.
7. Removed graph/Neo4j code paths and compatibility fields from CLI, MCP, `/v1/ask`, service types, and ask timing/diagnostics output.
8. Renamed `--cache-skip-browser` to `--cache-http-only`.
9. Changed scrape preamble output from `embed: true|false` to `indexing: enabled|skipped`.
10. Updated changelog and active docs, then ran focused verification.

## Key Findings

- `--embed` was defaulting to true already, so the useful user-facing control was an opt-out, now `--skip-embed` at `src/core/config/cli/global_args.rs:67`.
- `--cache-skip-browser` actually forced cached crawl flow onto the HTTP path and suppressed Chrome bootstrap/runtime, now represented as `cache_http_only` at `src/core/config/cli/global_args.rs:43`.
- Remaining graph code was broader than a hidden CLI flag: it included `Config::ask_graph`, MCP `ask.graph`, `/v1/ask.graph`, `timing_ms.graph`, graph diagnostics placeholders, and the Neo4j client module.
- Active source no longer has `src/core/neo4j.rs`; the module export was removed from `src/core.rs`.
- MCP and `/v1/ask` now reject the removed graph field via unknown-field behavior, covered by `src/mcp/schema_tests.rs:655` and `src/web/server_tests.rs:508`.

## Technical Decisions

- Prefer explicit opt-out flags over boolean set flags for default-on behavior: `--skip-embed` replaces `--embed false`.
- Keep the internal runtime boolean as `Config::embed` because it accurately represents downstream behavior and avoids needless churn; only the CLI surface changed.
- Rename the cache flag to describe behavior, not implementation: `--cache-http-only` instead of `--cache-skip-browser`.
- Remove graph compatibility fields instead of preserving rejection handlers, because the product contract says graph retrieval is not part of the production CLI, MCP, or `/v1/ask`.
- Keep historical changelog entries and archived reports intact; only active source/docs were cleaned.

## Files Modified

- `CHANGELOG.md`: Added Unreleased breaking changes and verification notes for this cleanup session.
- `src/core/config/cli/global_args.rs`: Removed old noisy CLI flags, added `skip_embed` and `cache_http_only`.
- `src/core/config/help.rs`: Rebuilt custom help output and updated visible global rows.
- `src/core/config/parse/build_config/config_literal.rs`: Mapped `skip_embed` to `Config::embed = false` and wired `cache_http_only`.
- `src/core/config/types/config.rs`, `config_impls.rs`, `overrides.rs`: Removed graph config and renamed cache field.
- `src/core.rs`, `src/core/neo4j.rs`, `src/core/neo4j_tests.rs`: Removed legacy Neo4j module and tests.
- `src/crawl/chrome_bootstrap.rs`, `src/crawl/engine/runtime.rs`: Switched crawl runtime logic to `cache_http_only`.
- `src/vector/ops/commands/ask.rs`, `src/vector/ops/commands/ask/context.rs`, `src/services/types/service.rs`: Removed graph timing and graph diagnostics placeholders.
- `src/mcp/schema.rs`, `src/mcp/server/handlers_query.rs`: Removed MCP ask graph compatibility field and handler rejection branch.
- `src/web/server/types.rs`, `src/web/server/handlers/ask.rs`: Removed `/v1/ask.graph` compatibility field and handler branch.
- `src/cli/commands/scrape.rs`: Changed preamble label to `indexing`.
- `docs/commands/ask.md`, `docs/commands/crawl.md`, `docs/commands/scrape.md`: Updated active docs for removed graph timing and new embed/cache flags.
- `src/core/README.md`, `src/core/CLAUDE.md`, `src/cli/CLAUDE.md`, `src/mcp/README.md`: Removed stale Neo4j/graph surface notes.
- Tests updated under `tests/` and source sidecar tests to cover the new behavior and removed fields.

## Commands Executed

- `rg` searches over `src`, `tests`, `docs/commands`, `README.md`, `docs/CONFIG.md`, and `config.example.toml` to find old flags and graph references.
- `cargo fmt --check`: formatting check passed after edits.
- `cargo check --bin axon`: compile check passed.
- `cargo test --test cli_help_contract`: CLI help and removed-flag rejection tests passed.
- `cargo test runtime_migration_tests`: Chrome/cache mode contract tests passed.
- `cargo test --test mcp_contract_parity`: MCP typed contract tests passed.
- `cargo test --test services_query_services`: service query mapping tests passed.
- `cargo test --test cli_full_rewire_smoke`: service mapping smoke tests passed.
- `cargo test parse_ask_rejects_removed_graph_field`: MCP ask graph-field rejection passed after removing the lingering field.
- `cargo test v1_ask_rejects_removed_graph_field`: `/v1/ask` graph-field rejection passed.
- Direct probes:
  - `./target/debug/axon --help | rg -- '--cache-http-only|--cache-skip-browser|--skip-embed|--embed|--graph'`
  - `./target/debug/axon --cache-skip-browser true status`
  - `./target/debug/axon --cache-http-only crawl --help | rg -- '--cache-http-only|--cache-skip-browser'`

## Errors Encountered

- The first graph removal patch failed because `config_literal.rs` context had shifted. Resolved by re-reading the exact lines and applying smaller patches.
- `parse_ask_rejects_removed_graph_field` initially failed because `AskRequest` still had a `graph` field. Removed the lingering field and `unsupported_graph_error` helper.
- Removing `AskRequest.graph` broke struct literals in `src/services/action_api_tests.rs`; removed those fields and reran the focused tests.
- Several Cargo commands waited on package/build/artifact locks. They completed after waiting; no unrelated processes were killed.

## Behavior Changes (Before/After)

| Area | Before | After |
|---|---|---|
| Top-level help | Custom help omitted commands and used stale hand-written sections | Help command list derives from Clap command surface |
| Command help | Subcommands inherited many unrelated global flags | Focused help hides unrelated global noise |
| Embedding opt-out | `--embed false` | `--skip-embed` |
| Cache/browser flag | `--cache-skip-browser <bool>` | `--cache-http-only` |
| Scrape preamble | `embed: true|false` | `indexing: enabled|skipped` |
| Graph request fields | CLI/MCP/web had hidden or compatibility graph fields | Graph fields are removed and rejected |
| Ask timing/diagnostics | Included permanent zero-value graph placeholders | Graph placeholders removed |

## Verification Evidence

| Command | Expected | Actual | Status |
|---|---|---|---|
| `cargo fmt --check` | Formatting clean | Passed | Pass |
| `cargo check --bin axon` | Binary compiles | Passed | Pass |
| `cargo test --test cli_help_contract` | Help contract passes | 11 passed | Pass |
| `cargo test runtime_migration_tests` | Cache/Chrome mode tests pass | 16 passed | Pass |
| `cargo test --test mcp_contract_parity` | MCP typed contracts pass | 28 passed | Pass |
| `cargo test --test services_query_services` | Query mapping tests pass | 14 passed | Pass |
| `cargo test --test cli_full_rewire_smoke` | Service mapping smoke tests pass | 28 passed | Pass |
| `cargo test parse_ask_rejects_removed_graph_field` | MCP ask rejects `graph` field | Passed | Pass |
| `cargo test v1_ask_rejects_removed_graph_field` | `/v1/ask` rejects `graph` field | Passed | Pass |
| `./target/debug/axon --cache-skip-browser true status` | Old flag rejected | `unexpected argument '--cache-skip-browser'` | Pass |
| `./target/debug/axon --cache-http-only crawl --help` | New flag visible | Output included `--cache-http-only` | Pass |

## Risks and Rollback

- This is a breaking CLI/API surface change. Existing callers using `--embed false`, `--cache-skip-browser`, MCP `ask.graph`, `/v1/ask.graph`, or `timing_ms.graph` must update.
- Removing `timing_ms.graph` can affect consumers deserializing ask timing with a required graph field.
- Rollback path is to restore the deleted fields and compatibility branches from git, but that would reintroduce the stale/dead surface the session intentionally removed.

## Decisions Not Taken

- Did not keep MCP/web `graph: false` as a no-op compatibility field. The user asked to kill the remaining graph/Neo4j dead code, and compatibility fields were part of that stale surface.
- Did not rename Spider's internal `with_cache_skip_browser` method. That is an upstream/library API call; Axon's user-facing and config names now use `cache_http_only`.
- Did not rewrite historical changelog entries, archived reports, or session notes that mention old graph/cache/embed behavior.

## Open Questions

- The normal installed `axon` binary was not rebuilt or installed during this save step. Verification used `target/debug/axon`.
- The worktree was already very dirty with many unrelated tracked and untracked changes. This note scopes the CLI/config/help/graph cleanup work from this session, not the entire dirty tree.

## Next Steps

- Rebuild/install the release binary and confirm the shell `axon` command uses the updated surface:
  - `which axon`
  - `axon --version`
  - `axon --help | rg -- '--skip-embed|--embed|--cache-http-only|--cache-skip-browser|--graph'`
- Update any external automation or docs that call `--embed false`, `--cache-skip-browser`, or send ask `graph` fields.
- Stage this session note explicitly if committing ignored `docs/sessions/` content.
