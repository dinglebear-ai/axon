---
date: 2026-05-13 19:15:51 EST
repo: git@github.com:jmagar/axon.git
branch: main
head: 1ee42eeceb92f335e879505447b9d093bb913074
agent: Codex
session id: unknown
transcript: unknown
working directory: /home/jmagar/workspace/axon_rust
worktree: /home/jmagar/workspace/axon_rust 1ee42eec [main]
pr: #84 "fix(mcp): serve web and mcp on one port" https://github.com/jmagar/axon/pull/84
---

# Unified Web and MCP Port Merge

## User Request

The user asked for Axon's web server and MCP HTTP server to run on the same port, specifically port `8001`, then requested a quick push, PR creation, PR comment handling, merge back to `main`, cleanup, and this session save.

## Session Overview

- Routed HTTP MCP startup through the unified Axum server so the web panel, first-party HTTP APIs, OAuth routes, and `/mcp` share one listener.
- Kept plain `axon mcp` as stdio-only while documenting that only HTTP selectors start the unified HTTP listener.
- Addressed PR review comments around OAuth resource metadata, stdio wording, default bind examples, and dead HTTP server code.
- Merged PR #84 into `main`, deleted the feature branch locally and remotely, and pushed Beads/Dolt state.

## Sequence of Events

1. Inspected MCP and serve startup paths and confirmed `axon serve` already used `run_unified_server`.
2. Updated `axon mcp --transport http` and the HTTP side of `--transport both` to call the unified server.
3. Quick-pushed branch `fix/unify-web-mcp-port-8001`, created PR #84, fetched review comments, and resolved the actionable feedback.
4. Removed the unused MCP-only `run_http_server` path and re-export.
5. Clarified docs and changelog around transport behavior, default bind address, and OAuth metadata versus canonical `/mcp` token audience.
6. Verified with focused Rust tests, cargo checks, and the MCP HTTP xtask guard.
7. Rebased the feature branch onto current `origin/main`, fast-forwarded `main`, pushed `main`, and cleaned up the branch.

## Key Findings

- `src/cli/commands/mcp.rs:8` keeps default `stdio` behavior for `axon mcp`.
- `src/cli/commands/mcp.rs:10` starts `run_unified_server` for HTTP MCP.
- `src/cli/commands/mcp.rs:17` starts `run_unified_server` alongside stdio for `--transport both`.
- `src/mcp/auth.rs:136` documents that OAuth metadata advertises the public origin while the canonical protected resource audience remains `/mcp`.
- `src/mcp/auth.rs:254` still configures lab-auth with `resource_path("/mcp")`.
- `docs/mcp/TRANSPORT.md:13` clarifies that plain `axon mcp` does not open an HTTP listener.
- `docs/commands/mcp.md:43` documents the default endpoint as `http://127.0.0.1:8001/mcp`.

## Technical Decisions

- Use one unified HTTP listener for every HTTP MCP selector instead of maintaining separate web and MCP-only HTTP server paths.
- Preserve stdio-only semantics for plain `axon mcp`, because local MCP clients expect process-stdio behavior and no network bind.
- Keep OAuth protected-resource metadata mounted at the public origin, while retaining `/mcp` as the canonical resource audience to avoid changing token semantics.
- Remove `run_http_server` instead of documenting it, because all in-tree HTTP entrypoints now use `run_unified_server`.

## Files Modified

- `src/cli/commands/mcp.rs` - HTTP MCP transport now starts the unified server.
- `src/mcp/server/http.rs` - removed the unused `run_http_server` entrypoint.
- `src/mcp/server.rs` and `src/mcp.rs` - removed the stale `run_http_server` re-export.
- `src/mcp/auth.rs` - clarified OAuth metadata base behavior and added regression coverage for `/mcp` audience preservation.
- `docs/mcp/TRANSPORT.md` - clarified stdio-only default and unified HTTP selectors.
- `docs/commands/mcp.md` - fixed default bind wording to `127.0.0.1:8001`.
- `docs/auth/MCP-AUTH.md` - documented metadata URL and `/mcp` resource value.
- `docs/MCP.md`, `docs/commands/serve.md`, `docs/mcp/CONNECT.md`, and `docs/mcp/DEV.md` - aligned HTTP transport docs with the unified listener model.
- `src/core/config/cli.rs` and `src/core/config/help.rs` - updated CLI help text.
- `xtask/src/checks/mcp_http.rs` - updated the guard to require `run_unified_server(`.
- `Cargo.toml`, `Cargo.lock`, `CHANGELOG.md`, and `bin/axon` - included in the quick-push commit.
- `.beagle/research/2026-05-12-rag-prompt-depth-injection-skills/findings/*.md` - three pre-existing deleted files were included by the quick-push flow.

## Commands Executed

- `cargo check --bin axon` - passed.
- `cargo test parse_mcp --lib` - passed.
- `cargo test parse_serve_mcp --lib` - passed.
- `cargo test oauth_resource_url --lib` - passed.
- `cargo test oauth_metadata_base_keeps_mcp --lib` - passed.
- `cargo run --package xtask -- check-mcp-http` - passed after guard update.
- `git push -u origin fix/unify-web-mcp-port-8001` - pushed PR branch.
- `gh pr view 84 --json state,mergedAt,mergeCommit,url` - confirmed PR #84 merged.
- `git merge --ff-only origin/fix/unify-web-mcp-port-8001` - fast-forwarded `main`.
- `git push origin main` - pushed merged main.
- `git push origin --delete fix/unify-web-mcp-port-8001` - deleted remote feature branch.
- `git branch -d fix/unify-web-mcp-port-8001` - deleted local feature branch.
- `bd dolt push` - completed.

## Errors Encountered

- The first commit attempt failed on the `mcp-http-only` xtask guard because it still looked for `run_http_server(`. The guard was updated to check for `run_unified_server(`.
- `git switch main` initially failed earlier because `main` was checked out in a separate worktree. By final cleanup, only the root worktree remained and switching/merging on `main` succeeded.
- A container runtime check showed the local `axon` app container was not running latest code: the app container was only `Created`, nothing listened on `8001`, and the local `ghcr.io/jmagar/axon:latest` image still reported `1.11.0` while the branch was `1.11.1`.

## Behavior Changes (Before/After)

| Before | After |
|---|---|
| `axon mcp --transport http` used an MCP-only HTTP path. | `axon mcp --transport http` starts the unified web + MCP HTTP server. |
| `axon mcp --transport both` paired stdio with the MCP-only HTTP server. | `axon mcp --transport both` pairs stdio with the unified HTTP server. |
| Plain `axon mcp` docs could be read as starting HTTP. | Plain `axon mcp` is documented as stdio-only. |
| Docs included an example implying default bind `0.0.0.0:8001`. | Docs state the default bind as `127.0.0.1:8001`. |
| `run_http_server` remained as dead code after unification. | Dead MCP-only HTTP entrypoint and re-export were removed. |

## Verification Evidence

| command | expected | actual | status |
|---|---|---|---|
| `cargo check --bin axon` | binary type-checks | passed | PASS |
| `cargo test parse_mcp --lib` | MCP parser tests pass | passed | PASS |
| `cargo test parse_serve_mcp --lib` | serve MCP parser test passes | passed | PASS |
| `cargo test oauth_resource_url --lib` | OAuth resource URL tests pass | passed | PASS |
| `cargo test oauth_metadata_base_keeps_mcp --lib` | OAuth metadata/audience regression passes | passed | PASS |
| `cargo run --package xtask -- check-mcp-http` | MCP HTTP guard passes | passed | PASS |
| `gh pr view 84 --json state,mergedAt,mergeCommit,url` | PR is merged at expected commit | merged at `1ee42eeceb92f335e879505447b9d093bb913074` | PASS |
| `git ls-remote origin refs/heads/main refs/heads/fix/unify-web-mcp-port-8001` | only `main` remains | only `refs/heads/main` at `1ee42eec` returned | PASS |
| `git status --short --branch` | clean main tracking origin | `## main...origin/main` | PASS |
| `bd dolt push` | tracker state pushed | `Push complete.` | PASS |

## Risks and Rollback

- Risk: HTTP MCP startup now initializes the unified web stack, so any future web-server startup regression could affect HTTP MCP mode.
- Risk: the quick-push commit included pre-existing dirty-tree changes, including `bin/axon` and `.beagle` deletions.
- Rollback: revert commit `1ee42eeceb92f335e879505447b9d093bb913074` from `main`, then rebuild/redeploy the previous container image if runtime rollback is needed.

## Decisions Not Taken

- Did not reintroduce or preserve the MCP-only HTTP server path; all in-tree HTTP paths now use the unified server.
- Did not change the OAuth canonical audience from `/mcp`; only the metadata discovery base was clarified.
- Did not start a side-port runtime workaround after the container check showed stale/non-running app state.

## References

- PR #84: https://github.com/jmagar/axon/pull/84
- `docs/sessions/2026-05-13-unified-web-mcp-port-quick-push.md`

## Open Questions

- The app container still needs a rebuild/redeploy before runtime will reflect `1.11.1` and the merged unified-port behavior.
- The quick-push-included `.beagle` deletions and `bin/axon` LFS pointer update were preserved because they were already part of the dirty tree.

## Next Steps

- Started but not completed: rebuild/redeploy the Axon app container from `main` so the container runtime runs commit `1ee42eec` or a newer image containing it.
- Follow-on: verify `http://127.0.0.1:8001/healthz` and `http://127.0.0.1:8001/mcp` against the rebuilt container.
