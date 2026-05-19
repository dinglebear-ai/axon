---
date: 2026-05-19 13:43:25 EST
repo: git@github.com:jmagar/axon.git
branch: main
head: 161001d9
working directory: /home/jmagar/workspace/axon_rust
worktree: /home/jmagar/workspace/axon_rust                                             161001d9 [main]
---

# Summarize Command and OAuth Scope Fix

## User Request

Add a new `axon summarize <url or urls>` command by reusing the existing `ask` command patterns, make it service-layer first so CLI/API/MCP can all call it, and update stale docs. After that, investigate OAuth scope failures where authenticated users were blocked from crawl/write operations, then fix OAuth access so allowed OAuth users can crawl.

## Session Overview

- Implemented a `summarize` service, CLI command, REST route, action API dispatch, server-mode dispatch, and MCP action.
- Updated current command/API/MCP architecture docs and added a dedicated command doc.
- Fixed OAuth scope behavior so Axon OAuth email allowlisting is the access boundary: new OAuth tokens default to both Axon scopes, and existing tokens with either Axon scope satisfy all Axon read/write checks.
- Added a shared `authz` scope predicate and focused tests for the new access model.

## Sequence of Events

1. Located the existing `ask` command path across CLI dispatch, service layer, MCP handlers, REST routes, and docs.
2. Added `summarize` as a service-layer operation that scrapes URL content, builds bounded untrusted context, and calls the configured LLM backend rather than hardcoding Gemini.
3. Wired the new operation through CLI, MCP, `/v1/actions`, `/v1/summarize`, server mode, result types, and current docs.
4. Swept stale docs and runtime-facing MCP help/scope surfaces, then corrected remaining `summarize` omissions.
5. Traced OAuth scopes from `src/mcp/auth.rs` through MCP, REST, and `/v1/actions` scope enforcement.
6. Changed OAuth defaults and scope checks so any allowed Axon OAuth user can perform the server's core actions, including crawl.
7. Verified formatting, binary compilation, and focused auth/scope tests.

## Key Findings

- `src/mcp/auth.rs` built OAuth config with supported scopes `axon:read` and `axon:write`, but the OAuth default was only `axon:read`, so clients that did not request `axon:write` received read-only tokens.
- `vendor/lab-auth/src/authorize.rs` falls back to `default_scope` when the OAuth request omits `scope`, and validates any space-separated combination against `scopes_supported`.
- `src/mcp/server.rs`, `src/web/actions.rs`, `src/web/server/routing.rs`, and `src/web/server/handlers/rest/auth.rs` each had local scope-satisfaction logic before this session.
- The worktree was already heavily dirty before these changes. The session did not attempt to clean or revert unrelated modified and untracked files.

## Technical Decisions

- `summarize` lives in `src/services/summarize.rs` so CLI, REST, action API, and MCP all share one implementation.
- Summary generation goes through `src/services/llm_backend/` using the configured completion backend. Gemini remains the current supported backend but is not directly hardcoded at the command boundary.
- Summary context is treated as untrusted scraped page content and bounded before being sent to the LLM.
- OAuth email allowlisting is now the practical authorization boundary for Axon. `axon:read` and `axon:write` remain as metadata/compatibility strings, but either Axon scope satisfies all Axon read/write checks.
- A shared `src/authz.rs` predicate replaced duplicated scope checks so MCP, REST, and `/v1/actions` do not drift again.

## Files Modified

### Summarize Implementation

- `src/services/summarize.rs` and `src/services/summarize_tests.rs` - new service-layer summary operation and focused tests.
- `src/services.rs` - exported the new service module.
- `src/services/types/service.rs` - added `SummarizeResult` and related typed result structs.
- `src/cli/commands/summarize.rs`, `src/cli/commands.rs`, `src/core/config/cli.rs`, `src/core/config/types/enums.rs`, `src/core/config/parse/build_config/command_dispatch.rs`, and `src/lib.rs` - added CLI parsing and dispatch.
- `src/cli/server_mode.rs`, `src/cli/server_mode/plan.rs`, and `src/cli/server_mode/render.rs` - added server-mode support.
- `src/mcp/schema.rs`, `src/mcp/server.rs`, and `src/mcp/server/handlers_query.rs` - added MCP schema, dispatch, handler, and tool instructions.
- `src/services/action_api.rs`, `src/services/action_api/commands.rs`, `src/services/action_api/commands/dispatchers.rs`, and `src/services/types/client_server.rs` - added `/v1/actions` dispatch and capabilities.
- `src/web/server/routing.rs`, `src/web/server/handlers/exploration.rs`, `src/web/server/openapi.rs`, `src/web/server/handlers/rest.rs`, `src/web/server/handlers/rest/types.rs`, and `src/web/server/handlers/rest/sync_post.rs` - added REST route and OpenAPI metadata.

### OAuth Scope Fix

- `src/authz.rs` and `src/authz_tests.rs` - new shared Axon scope constants and access predicate.
- `src/mcp/auth.rs` - changed OAuth default scope to `axon:read axon:write` and centralized Axon scope constants.
- `src/mcp/server.rs` - uses shared scope predicate for MCP tool calls.
- `src/web/actions.rs` - uses shared scope predicate for `/v1/actions`.
- `src/web/server/routing.rs` - uses shared scope predicate for first-party REST route groups.
- `src/web/server/handlers/rest/auth.rs` - uses shared scope predicate for REST scope guard middleware.
- `src/mcp/auth_tests.rs` and `src/web/server/handlers/rest_tests.rs` - updated tests to match the new access model.

### Docs

- `docs/commands/summarize.md` - new command reference.
- `README.md`, `CLAUDE.md`, `docs/API.md`, `docs/ARCHITECTURE.md`, `docs/MCP.md`, `docs/MCP-TOOL-SCHEMA.md`, `docs/README.md`, `docs/commands/README.md`, `docs/mcp/TOOLS.md`, `docs/mcp/DEV.md`, `src/cli/CLAUDE.md`, and `src/mcp/CLAUDE.md` - updated current docs for summarize and OAuth scope behavior.

## Commands Executed

- `rg` searches across `src`, `docs`, and command/MCP modules to locate `ask` patterns and stale docs.
- `cargo check --bin axon` - used repeatedly as the main compile verification.
- `cargo test summarize --lib` - verified summarize service tests during implementation.
- `cargo run --bin axon -- summarize --help` - verified CLI help surfaced the new command.
- `cargo fmt --check` and `cargo fmt` - checked and applied Rust formatting.
- `cargo test authz --lib` - verified the new shared scope predicate.
- `cargo test scope_check --lib` - verified REST scope predicate expectations.
- `cargo test oauth_metadata_base_keeps_mcp_as_canonical_resource_audience --lib` - verified OAuth metadata/resource behavior after changing the default scope.

## Errors Encountered

- Initial verification exposed existing compile drift around `src/core/config/cli/setup_args.rs` visibility. The fields/types referenced by `src/core/config/cli.rs` were made visible enough for the current build.
- Several focused test runs failed while other Cargo processes held package/build locks; rerunning after the lock cleared succeeded.
- Older test fixtures still referenced removed `ask_graph` and `AskTiming.graph` fields. The relevant fixtures were adjusted so focused auth/scope tests could compile.
- A transient patch application did not match the current file state because the worktree already had overlapping edits. The files were re-read and patched against the current content.

## Behavior Changes (Before/After)

| Area | Before | After |
| --- | --- | --- |
| CLI | No `axon summarize` command | `axon summarize <url>...` scrapes URLs and summarizes with configured LLM backend |
| Services | No shared summarize entry point | `services::summarize::summarize` is callable from CLI/API/MCP |
| REST | No `/v1/summarize` | `POST /v1/summarize` accepts `url` or `urls` |
| MCP | No `action=summarize` | `action=summarize` is handled through the consolidated `axon` tool |
| OAuth default | Tokens could default to only `axon:read` | New OAuth tokens default to `axon:read axon:write` |
| Scope checks | Some authenticated OAuth users with `axon:read` could be blocked from crawl/write routes | Any token with either Axon scope satisfies all Axon read/write routes |

## Verification Evidence

| Command | Expected | Actual | Status |
| --- | --- | --- | --- |
| `cargo check --bin axon` | Binary compiles | Finished successfully | PASS |
| `cargo test summarize --lib` | Summarize tests pass | Passed during implementation | PASS |
| `cargo run --bin axon -- summarize --help` | CLI help prints summarize command usage | Help command completed during implementation | PASS |
| `cargo fmt --check` | Rust formatting clean | Completed successfully | PASS |
| `cargo test authz --lib` | New authz predicate tests pass | 4 passed | PASS |
| `cargo test scope_check --lib` | REST scope expectation tests pass | 3 passed | PASS |
| `cargo test oauth_metadata_base_keeps_mcp_as_canonical_resource_audience --lib` | OAuth metadata/resource test passes | 1 passed | PASS |

## Risks and Rollback

- OAuth authorization is now intentionally coarser for Axon: email allowlisting grants full server access. Roll back by reverting `src/authz.rs`, restoring exact read/write predicate checks in MCP/REST/action paths, and changing `src/mcp/auth.rs` default scope back to `axon:read`.
- Existing OAuth clients may cache old tokens. With the compatibility predicate, tokens containing either `axon:read` or `axon:write` should continue working after the server is restarted.
- The worktree contains broad unrelated modifications and untracked files. A rollback should be scoped to the files listed in this note, not a blanket reset.

## Decisions Not Taken

- Did not modify historical docs under `docs/plans`, `docs/sessions`, or `docs/reports`; those are snapshots rather than current product docs.
- Did not remove scope strings from OAuth metadata entirely. They remain for client compatibility and for non-Axon exact-match behavior in the shared predicate.
- Did not hardcode Gemini in the summarize command. The command uses the configured LLM backend path.

## Open Questions

- The live `axon serve` / MCP HTTP process must be restarted before the OAuth behavior change takes effect in runtime.
- Full repo test execution was not attempted because the worktree is already very broad and dirty; focused checks passed for the changed surfaces.
- There are many pre-existing dirty and untracked files unrelated to this session, including large docs/session/report inventories and SQLite job-runtime refactor files.

## Next Steps

### Started But Not Completed

- Restart the live Axon server/MCP HTTP process and verify an OAuth-authenticated crawl request succeeds with an existing `axon:read` token.

### Follow-On Tasks

- If this branch is being prepared for commit, review `git status --short` carefully and stage only the intended session changes or intentionally stage the full dirty tree.
- Consider adding an integration test that mints/uses an OAuth JWT with only `axon:read` and proves `crawl start` reaches the handler.
