# Remove ACP and Standardize Gemini Headless Merge

Date: 2026-05-08

## Request

Merge the ACP removal branch back into `main`, clean up its worktree, pull the latest into the local main checkout, and save a markdown handoff note.

## Merge

- PR: https://github.com/jmagar/axon/pull/75
- Branch: `bd-work/remove-acp-gemini-headless`
- Base: `main`
- Merge commit: `5786c972a44f32998521873a6d30ba31b737edc6`
- Merged at: 2026-05-08 02:26:02 UTC

## Implementation Summary

- Removed ACP protocol code, ACP MCP handlers, ACP service/session/cache/permission modules, and ACP-specific tests/docs.
- Standardized LLM completion paths on Gemini headless via `src/services/llm_backend/`.
- Kept `OPENAI_MODEL` only as compatibility input for Gemini model values; added explicit Gemini/headless env knobs.
- Updated ask, research, evaluate, suggest, debug, and extract fallback paths to use the headless backend.
- Removed ACP from generated MCP schema docs and server help text.
- Preserved MCP stdio/http/both support and cache hardening in `mcp`/`serve`.
- Added logging fallback behavior so denied log directories do not panic CLI startup.

## Verification Evidence

- `python3 scripts/generate_mcp_schema_doc.py --check` passed.
- `cargo fmt --check` passed.
- `cargo check --tests` passed.
- `cargo clippy --tests -- -D warnings` passed.
- `cargo build --release --bin axon` passed on rebased head `bc49c643479d7766a14a7166520e9253a9e07706`.
- `./target/release/axon --version` returned `axon 1.8.4`.
- Release denied-log-dir smoke passed with exit code 0:
  - `AXON_DATA_DIR=<0500 tempdir> AXON_SQLITE_PATH=<tempdir>/jobs.db ./target/release/axon --json doctor`
  - stderr reported `warn: failed to create axon log file appender; continuing with stderr logging`
  - stdout returned a JSON doctor report beginning with `"all_ok": true`.
- Earlier branch verification before the final merge included:
  - release build of `axon 1.8.1`
  - CLI help/parse matrix across 28 top-level commands and lifecycle subcommands
  - live smoke coverage for `scrape`, `crawl`, `map`, `extract`, `screenshot`, `debug`, `serve`, and `mcp --transport http`
  - `cargo test cli -- --test-threads=1` with 190 passed, 0 failed

## Cleanup

- Local main checkout `/home/jmagar/workspace/axon_rust` was fast-forwarded to `5786c972a44f32998521873a6d30ba31b737edc6`.
- Removed feature worktree: `/home/jmagar/workspace/axon_rust/.worktrees/remove-acp-gemini-headless`.
- Deleted local branch `bd-work/remove-acp-gemini-headless`.
- Deleted remote branch `origin/bd-work/remove-acp-gemini-headless`.

## Open Questions

- No remaining merge or worktree cleanup tasks are known.
- The external Claude review check was still in progress when the merge was attempted, but GitHub reported the PR mergeable and accepted the merge.
