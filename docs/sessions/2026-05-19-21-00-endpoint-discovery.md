# Endpoint Discovery Session

Date: 2026-05-19
Repository: `/home/jmagar/workspace/axon_rust`
Worktree: `/home/jmagar/workspace/axon_rust/.worktrees/endpoint-discovery`
Branch: `work/endpoint-discovery`
Base: `ad5f714c7848a2ab0f793ad17ced702b2996cd20` (`origin/main`, `Update web panel and runtime env`)
PR: <https://github.com/jmagar/axon/pull/114>

## Scope

Executed `docs/plans/2026-05-19-endpoint-discovery.md` in an isolated worktree. The root checkout was left untouched.

## Changes

- Added `axon endpoints <url>` with stable JSON output and human-readable output.
- Added the endpoint discovery service and typed report model.
- Added static discovery from HTML attributes, inline scripts, first-party script bundles, GraphQL paths, WebSocket URLs, and absolute URLs.
- Added optional verification using conservative unauthenticated HEAD probes with OPTIONS fallback.
- Added optional Chrome/CDP network capture through the existing Chrome remote URL plumbing.
- Exposed endpoint discovery through MCP `action=endpoints`, the REST action API, and `/v1/endpoints`.
- Updated MCP schema docs, command docs, OpenAPI docs, env/config boundary metadata, and CLI help snapshots.
- Added parser, service, MCP contract, action API, and CLI help regression tests.
- Addressed reviewer feedback after the initial PR: explicit boolean CLI parsing, unquoted script source parsing, case-insensitive GraphQL matching, shared scan-byte caps for HTML attributes, protocol-relative first-party classification, WebSocket capture merging, `first_party_only` capture filtering, bounded capture validation caching, no-redirect verification probes, REST error taxonomy, and RAII loopback test cleanup.
- Addressed the second review wave: canonical env-matrix surface labels, timestamped session filename, guarded CLI URL extraction, config-backed defaults for MCP/REST/action API endpoint requests, optional service progress events, CDP event preservation while awaiting command responses, stronger MCP parity assertions, serialized loopback tests, and a non-vacuous capture filtering assertion.

## Verification

Local verification run from the worktree:

- `RUSTC_WRAPPER= cargo test -q endpoints`
- `RUSTC_WRAPPER= cargo test -q action_api`
- `RUSTC_WRAPPER= cargo test -q --test mcp_contract_parity`
- `RUSTC_WRAPPER= cargo test -q --test cli_help_contract`
- `unset NO_COLOR; RUSTC_WRAPPER= cargo test -q --test cli_help_contract`
- `RUSTC_WRAPPER= cargo check --bin axon`
- `python3 scripts/generate_mcp_schema_doc.py --check`
- `python3 scripts/test_mcp_doc_renderer.py`
- `RUSTC_WRAPPER= just verify`

Manual smoke checks:

- `./target/debug/axon endpoints https://example.com --json`
- `./target/debug/axon endpoints https://example.com --verify --json`
- `./target/debug/axon endpoints https://example.com --capture-network --json`

The final local full gate passed with `2271 passed, 6 skipped`.

After the review-fix wave, the focused gates passed again:

- `RUSTC_WRAPPER= cargo test -q endpoints` (`14` matching lib tests passed, plus filtered endpoint integration hits)
- `RUSTC_WRAPPER= cargo test -q action_api`
- `RUSTC_WRAPPER= cargo test -q --test mcp_contract_parity`
- `RUSTC_WRAPPER= cargo test -q --test cli_help_contract`
- `python3 scripts/generate_mcp_schema_doc.py --check`
- `python3 scripts/test_mcp_doc_renderer.py`
- `RUSTC_WRAPPER= cargo check --bin axon`
- `RUSTC_WRAPPER= just verify`

The post-review local full gate passed with `2276 passed, 6 skipped`.

After the second review-fix wave, repeat verification was run before the final push; see the final response for the exact command results and GitHub check status.

After rebasing onto the updated `origin/main` head `52d4ff65`, another review wave was addressed:

- Added endpoint discovery defaults to `Config` debug output.
- Updated the CommandKind local contract note to include the new `Endpoints` command.
- Added `--unique-only` to CLI help contract assertions.
- Made endpoint HTML body reads truncate consistently instead of erroring solely because `Content-Length` exceeded the scan cap.
- Broke out of the CDP capture loop when the WebSocket stream closes before page load.
- Moved endpoint service result types into `src/services/types/endpoints.rs` and re-exported them from `src/services/types.rs`.

The MCP schema request-model split suggested by review was tested and reverted because `scripts/generate_mcp_schema_doc.py` intentionally parses request structs from `src/mcp/schema.rs`; splitting `EndpointsRequest` without first changing that generator breaks schema-doc sync.

## Review And CI

CodeRabbit completed on PR #114. The first CI run exposed MCP schema doc drift, which was fixed by wiring endpoint discovery into the schema doc generator. The next CI run exposed ANSI color drift in CLI help snapshots, which was fixed by forcing `NO_COLOR=1` in the help contract test harness.

The final-head `mcp-smoke` run then exposed MCP help/tool-description parity drift. `endpoints` had been added to the tool description and schema but not to the mcporter expected action lists or `action=help` response. The same parity audit also showed the pre-existing `summarize` description/help mismatch, so both direct actions were added to the help response and smoke expectations.

Named work-it review agents were not all available in this Codex session. Substitutions used were local full verification, GitHub CI, CodeRabbit, clippy, monolith checks, and direct PR comment inspection.

The GitHub review wave surfaced actionable Cubic, Copilot, and Codex comments after the first green CI run. Those comments were addressed in code and tests in a follow-up commit before the final push.

The final CodeRabbit wave added a mix of quick fixes and broader refactor suggestions. Quick fixes were applied. The MCP schema split was left as a generator-aware follow-up because the current docs generator contract requires request structs to remain in `src/mcp/schema.rs`.

## Open Questions

- None known at the time this note was written.
