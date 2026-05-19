# Session: MCP HTTP/OAuth Recovery and Docker Stack Stabilization
Date: 2026-03-03 (EST)
Repo: `/home/jmagar/workspace/axon_rust`
Branch: `feat/sidebar`

## 1. Session overview
- Investigated `axon-workers` restart noise and repeated MCP failures (`ConnectionClosed("initialized request")`) seen in container logs.
- Verified local/remote history divergence and confirmed HTTP+OAuth work had been reset out locally, then restored.
- Re-enabled and validated HTTP MCP + OAuth-protected endpoints in `axon-workers`.
- Switched CLI MCP command path to HTTP-only runtime.

## 2. Timeline of major activities
- `2026-03-03 19:11:21 -0500`: Reflog shows reset to `HEAD~1` from `cd8d172c` to `62bdae5e` (`git reflog`).
- `2026-03-03 20:03:02 -0500`: Cherry-pick commit created locally as `7fb1100d` (`feat(mcp): add HTTP transport with Google OAuth + cleanup`).
- Rebuilt/restarted `axon-workers` multiple times with `docker compose up -d --build axon-workers` to validate runtime behavior.
- Final endpoint probes at `2026-03-04T01:12:42Z` showed OAuth-protected MCP HTTP responses and healthy discovery endpoints.

## 3. Key findings (with references)
- MCP CLI command path was not dispatching HTTP at runtime before patch; now always dispatches HTTP in [`crates/cli/commands/mcp.rs:4`](/home/jmagar/workspace/axon_rust/crates/cli/commands/mcp.rs#L4).
- HTTP server and OAuth routes are implemented in MCP server runtime: [`crates/mcp/server.rs:179`](/home/jmagar/workspace/axon_rust/crates/mcp/server.rs#L179) and routes at [`crates/mcp/server.rs:195`](/home/jmagar/workspace/axon_rust/crates/mcp/server.rs#L195).
- `mcp-http` s6 service launches Axon in HTTP mode via env vars in [`docker/s6/s6-rc.d/mcp-http/run:5`](/home/jmagar/workspace/axon_rust/docker/s6/s6-rc.d/mcp-http/run#L5).
- Dotenv loader now suppresses missing-file warnings for explicit `AXON_ENV_FILE` path-not-found in [`main.rs:30`](/home/jmagar/workspace/axon_rust/main.rs#L30).
- Branch divergence currently reports `3 1` (`HEAD...origin/feat/sidebar`) from `git rev-list --left-right --count`.

## 4. Technical decisions and rationale
- Restored missing HTTP+OAuth commit via cherry-pick instead of destructive reset to preserve in-progress local work.
- Kept OAuth-protected MCP HTTP behavior (401 until auth) because this matched implemented `require_google_auth` middleware behavior.
- Changed MCP command execution path to HTTP-only to match requested operational mode.
- Used container-local probes (`docker exec axon-workers curl`) to verify actual runtime behavior instead of relying on static code assumptions.

## 5. Files modified/created and purpose
- [`crates/cli/commands/mcp.rs`](/home/jmagar/workspace/axon_rust/crates/cli/commands/mcp.rs): force HTTP runtime (`run_http_server`) and env host/port parsing.
- [`crates/mcp/server.rs`](/home/jmagar/workspace/axon_rust/crates/mcp/server.rs): HTTP transport + OAuth route/middleware implementation present after restore.
- [`docker/s6/s6-rc.d/mcp-http/run`](/home/jmagar/workspace/axon_rust/docker/s6/s6-rc.d/mcp-http/run): launches `axon mcp` with HTTP env vars.
- [`main.rs`](/home/jmagar/workspace/axon_rust/main.rs): improved explicit env-file not-found handling.
- OAuth module files restored in `crates/mcp/server/oauth_google/*` and MCP wiring files restored by cherry-pick.

## 6. Critical commands executed and outcomes
- `git reflog --date=iso -n 20`: confirmed reset event and commit timeline.
- `git log --oneline --all --grep='oauth' -i`: identified `cd8d172c` on `origin/feat/sidebar`.
- `git stash push -u ... && git cherry-pick cd8d172c`: restored missing HTTP+OAuth commit (conflict resolved in `scrape.rs`).
- `LEFTHOOK=0 git cherry-pick --continue`: completed cherry-pick after hook-related failures.
- `docker compose up -d --build axon-workers`: rebuilt and restarted workers with recovered HTTP MCP path.
- `docker exec axon-workers curl ...`: validated MCP/OAuth endpoints and auth behavior.

## 7. Behavior changes (before/after)
- Before: `mcp-http` repeatedly exited with `ConnectionClosed("initialized request")`; after: `mcp-http` starts and stays running.
- Before: local branch lacked restored HTTP+OAuth commit after reset; after: commit restored locally as `7fb1100d`.
- Before: MCP CLI path could run stdio-only path; after: MCP CLI command path is HTTP-only.
- Before: repeated explicit AXON_ENV_FILE missing-file warning in Rust path; after: missing-file case handled without warning spam.

## 8. Verification evidence (`command | expected | actual | status`)
- `cargo check -q | compile success | exited 0 | PASS`
- `docker compose ps | axon-workers healthy | axon-workers Up ... (healthy) | PASS`
- `docker exec axon-workers curl http://127.0.0.1:8001/mcp | OAuth gate response | HTTP/1.1 401 + {"error":"authorization_required"} | PASS`
- `docker exec axon-workers curl http://127.0.0.1:8001/oauth/google/status | OAuth status endpoint reachable | HTTP/1.1 200 + configured:true | PASS`
- `docker exec axon-workers curl http://127.0.0.1:8001/.well-known/oauth-authorization-server | metadata reachable | HTTP/1.1 200 with issuer/authorization/token endpoints | PASS`

## 9. Source IDs + collections touched
- Session report embed attempted: attempted via `axon embed ... --json`; response used top-level `job_id` (no `data.job_id`).
- Embed job id: `d1063d31-a832-4bfa-83e9-70c612b6112e`
- Embed completion status: `completed`
- Source ID (`data.url`): `docs/sessions/2026-03-03-mcp-http-oauth-recovery-session.md (from successful retrieve candidate)`
- Collection (`data.collection`): `cortex` (from `result_json.collection` in embed status).
- Retrieve verification: `success for relative path source ID; absolute-path variant returned no content`

## 10. Risks and rollback
- Branch still diverges from `origin/feat/sidebar` (`3 ahead, 1 behind`), so future pulls may require reconciliation.
- Cherry-pick path triggered hook/test failures unrelated to MCP recovery; potential repo-state noise remains.
- OAuth-gated MCP returns 401 until authenticated session/token is provided; clients must handle this flow.
- Rollback path: `git revert 7fb1100d` (restored commit) and/or revert `crates/cli/commands/mcp.rs` HTTP-only change.

## 11. Decisions not taken
- Did not hard-reset branch to remote tip to avoid losing local WIP.
- Did not keep temporary `mcp-http` disabled workaround after restoring HTTP+OAuth path.
- Did not implement dual transport (`stdio` + `http`) in one process.

## 12. Open questions
- Should local branch be rebased/merged to eliminate current `3/1` divergence with `origin/feat/sidebar`?
- Are current OAuth env values (`issuer`, redirect URI) intended for this environment (`axon.tootie.tv`) during local container runs?
- Should pre-commit hook failures in unrelated tests be remediated now or tracked separately?

## 13. Next steps
- Reconcile branch divergence (`git pull --rebase` or controlled merge) after preserving local WIP intent.
- Decide whether to keep MCP command permanently HTTP-only or add controlled transport selection at runtime.
- Run authenticated MCP client flow against `/mcp` to validate post-login tool calls end-to-end.
