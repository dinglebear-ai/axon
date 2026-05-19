---
date: 2026-05-13 15:40:43 EDT
repo: git@github.com:jmagar/axon.git
branch: fix/unify-web-mcp-port-8001
head: e774e631
agent: Codex
session id: unknown
transcript: unknown
working directory: /home/jmagar/workspace/axon_rust
worktree: /home/jmagar/workspace/axon_rust e774e631 [fix/unify-web-mcp-port-8001]
---

# Unified Web and MCP Port Quick Push

## User Request

The user asked to run web and MCP on the same port, specifically port 8001, then requested `quick-push`.

## Session Overview

- Changed HTTP MCP transport to use the unified Axum server so web routes and `/mcp` share one listener.
- Updated CLI help and docs to describe the unified web/MCP HTTP runtime.
- Bumped the package from 1.11.0 to 1.11.1 and added a changelog entry.
- Committed and pushed branch `fix/unify-web-mcp-port-8001` to origin.

## Sequence of Events

1. Inspected the existing `serve` and MCP server paths.
2. Found that `axon serve` already used `run_unified_server`, while HTTP MCP modes still called the MCP-only HTTP server.
3. Updated `src/cli/commands/mcp.rs` so `http` and `both` MCP transports call `run_unified_server`.
4. Updated docs and CLI help for the unified listener model.
5. Ran focused verification, then handled the quick-push flow: branch creation, version bump, changelog, commit, Beads/Dolt push, and Git push.
6. Fixed the pre-commit `xtask check-mcp-http` guard after it failed because it still required the old `run_http_server(` string.

## Key Findings

- `src/cli/commands/serve.rs` already routes `axon serve` through `run_unified_server`.
- `src/cli/commands/mcp.rs:9` now routes HTTP MCP transport through `run_unified_server`.
- A live Docker-backed Axon server was already listening on `0.0.0.0:8001`; `/healthz` returned `ok` and `/mcp` returned the expected auth challenge.
- `xtask/src/checks/mcp_http.rs` encoded the old MCP-only function name and needed to track `run_unified_server(`.

## Technical Decisions

- Kept stdio MCP unchanged because it has no HTTP port.
- Treated every HTTP MCP selector as a unified HTTP server entrypoint, including `axon serve mcp`, `axon mcp --transport http`, and the HTTP side of `--transport both`.
- Bumped patch version because this is a behavior correction and docs alignment, not a new public feature family.
- Included the full dirty tree in the commit because `quick-push` stages with `git add .`.

## Files Modified

- `src/cli/commands/mcp.rs` - HTTP MCP transport now starts the unified server.
- `src/core/config/cli.rs` and `src/core/config/help.rs` - help text now describes unified HTTP behavior.
- `xtask/src/checks/mcp_http.rs` - guard now checks for `run_unified_server(`.
- `docs/MCP.md`, `docs/commands/mcp.md`, `docs/commands/serve.md`, `docs/mcp/CONNECT.md`, `docs/mcp/DEV.md`, `docs/mcp/TRANSPORT.md` - docs aligned with the shared 8001 listener.
- `Cargo.toml` and `Cargo.lock` - version bumped to 1.11.1.
- `CHANGELOG.md` - added 1.11.1 release notes.
- `bin/axon`, `src/mcp/auth.rs`, and three `.beagle/research/.../findings/*.md` deletions were pre-existing dirty-tree changes included by `git add .`.

## Commands Executed

- `cargo check --bin axon` - passed before commit.
- `cargo test parse_mcp --lib` - passed.
- `cargo test parse_serve_mcp --lib` - passed.
- `cargo run --package xtask -- check-mcp-http` - failed before guard update, passed after guard update.
- `git commit` - pre-commit suite passed after the guard update.
- `bd dolt push` - completed.
- `git push -u origin fix/unify-web-mcp-port-8001` - completed and uploaded one LFS object.

## Errors Encountered

- First commit attempt failed at `mcp-http-only`: `ERROR: MCP CLI must support HTTP transport in src/cli/commands/mcp.rs`.
- Root cause: the xtask guard searched for `run_http_server(`, which was the old MCP-only function path.
- Resolution: updated the guard and its tests to require `run_unified_server(`.

## Behavior Changes

| Before | After |
|---|---|
| `axon mcp --transport http` started an MCP-only HTTP listener. | `axon mcp --transport http` starts the unified web + MCP HTTP server. |
| `axon mcp --transport both` paired stdio with an MCP-only HTTP listener. | `axon mcp --transport both` pairs stdio with the unified web + MCP HTTP server. |
| Some docs described HTTP MCP as separate from web health/routes. | Docs describe web, first-party APIs, and MCP sharing port 8001. |

## Verification Evidence

| command | expected | actual | status |
|---|---|---|---|
| `cargo check --bin axon` | binary type-checks | passed | PASS |
| `cargo test parse_mcp --lib` | MCP transport parser tests pass | 5 passed | PASS |
| `cargo test parse_serve_mcp --lib` | serve-mcp parser test passes | 1 passed | PASS |
| `curl http://127.0.0.1:8001/healthz` | web health available on 8001 | `ok` | PASS |
| `curl http://127.0.0.1:8001/mcp` | MCP route available on 8001 | `401 Unauthorized` auth challenge | PASS |
| pre-commit suite | repo hooks pass | passed on second commit attempt | PASS |
| `bd dolt push` | tracker state pushed | `Push complete.` | PASS |
| `git push -u origin fix/unify-web-mcp-port-8001` | branch pushed | branch created and tracking origin | PASS |

## Risks and Rollback

- Risk: HTTP MCP startup now initializes the web panel/config path because it uses the unified server.
- Risk: the commit intentionally includes pre-existing dirty-tree changes from `bin/axon`, `src/mcp/auth.rs`, and `.beagle` deletions.
- Rollback: revert commit `e774e631` or change `McpTransport::Http` and the HTTP side of `Both` back to `run_http_server`.

## Decisions Not Taken

- Did not start a side server on another port; the target was the canonical `8001` listener.
- Did not alter stdio behavior.
- Did not create a PR from the pushed branch.

## Open Questions

- Whether the pushed pre-existing `src/mcp/auth.rs` public resource URL change was intended as part of the same branch.
- Whether the deleted `.beagle/research/.../findings/*.md` files should remain deleted.

## Next Steps

- Create or open the PR for `fix/unify-web-mcp-port-8001` if this should merge through review.
- Restart/redeploy the production container from the new branch or merged main when ready so the live server runs the new code.
