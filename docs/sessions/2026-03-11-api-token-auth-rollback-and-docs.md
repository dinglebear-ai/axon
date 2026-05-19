# Session: API-Token Auth Rollback + Docs Consistency
**Date:** 2026-03-11  
**Repo:** `/home/jmagar/workspace/axon_rust`

## 1. Session overview
- Rolled back web-surface auth complexity to API-token-only for current development.
- Removed Tailscale and SSH-key auth code paths from active Rust/Next auth checks.
- Rewrote auth documentation to match runtime behavior and removed obsolete auth docs.
- Verified build health and documentation reference consistency.

## 2. Timeline of major activities
- Audited auth docs, session logs, and runtime auth code to identify rollback scope.
- Simplified Rust auth wiring in `crates/web.rs` and download auth in `crates/web/download.rs`.
- Replaced `crates/web/tailscale_auth.rs` with token-only auth implementation.
- Simplified Next middleware auth logic in `apps/web/proxy.ts` to token-only checks.
- Updated root/crate docs and env templates; deleted obsolete `docs/auth/TAILSCALE.md` and `docs/auth/SSH-KEY.md`.

## 3. Key findings (with references)
- WS/download/output auth state no longer carries Tailscale/SSH fields in runtime state structs: `crates/web.rs:42`, `crates/web.rs:56`.
- Server startup now logs token mode only and no longer initializes Tailscale/SSH auth config: `crates/web.rs:80`, `crates/web.rs:84`.
- `/auth/ssh-challenge` route is no longer registered; active routes are `/ws`, `/ws/shell`, `/output/*`, and download routes: `crates/web.rs:111`.
- Download endpoints now authenticate via shared token from `Authorization`, `x-api-key`, or `?token=` only: `crates/web/download.rs:33`, `crates/web/download.rs:42`.
- Next middleware authorization no longer uses `tailscale-user-login`; it requires token match unless insecure localhost dev bypass is enabled: `apps/web/proxy.ts:143`, `apps/web/proxy.ts:147`.

## 4. Technical decisions and rationale
- Kept auth simple and explicit: single shared API token for web surfaces to reduce moving parts and local-dev friction.
- Preserved shell websocket server auth model (`AXON_SHELL_WS_TOKEN` fallback to shared token) because it is already token-based and independent.
- Deleted obsolete auth docs instead of keeping “historical” method docs in active auth reference docs to avoid operator confusion.
- Kept session logs untouched as historical records; updated only authoritative docs and runtime-guidance docs.

## 5. Files modified/created and purpose
- `crates/web.rs` — removed Tailscale/SSH auth wiring and unified on token auth.
- `crates/web/download.rs` — removed SSH-first branch; token auth only.
- `crates/web/tailscale_auth.rs` — replaced with token-only auth outcome/check module.
- `apps/web/proxy.ts` — removed dual-auth/Tailscale header logic; token-only middleware auth.
- `.env.example` — removed `AXON_REQUIRE_DUAL_AUTH` and `AXON_SSH_AUTHORIZED_KEYS` guidance.
- `README.md` — updated auth docs summary to API token + shell token + MCP OAuth.
- `CLAUDE.md` — removed dual-auth and SSH auth env guidance.
- `crates/web/CLAUDE.md` — updated web auth stack documentation to token-only model.
- `docs/auth/README.md` — rewritten overview for current auth model.
- `docs/auth/API-TOKEN.md` — rewritten token auth guide for web surfaces.
- `docs/auth/MCP-OAUTH.md` — updated `/ws` troubleshooting text to token-only guidance.
- `docs/auth/TAILSCALE.md` — deleted (obsolete for current auth model).
- `docs/auth/SSH-KEY.md` — deleted (obsolete for current auth model).
- `docs/sessions/2026-03-11-api-token-auth-rollback-and-docs.md` — this session log.

## 6. Critical commands executed and outcomes
- `rg` sweeps over docs/code for Tailscale/SSH/dual-auth markers — identified remaining references for cleanup.
- `cargo fmt --all` — completed successfully.
- `cargo check` — completed successfully (`Finished dev profile` observed).
- Post-cleanup `rg` sweep for removed docs/env/auth markers — no matches in active docs/code scope.
- `axon status --json` — preflight executed successfully (returned queue state JSON).
- `axon embed "docs/sessions/2026-03-11-api-token-auth-rollback-and-docs.md" --json` — returned `job_id` `9bbe8bb3-64c5-4f08-9631-648b5416a21e` with pending status.
- `axon embed status "9bbe8bb3-64c5-4f08-9631-648b5416a21e" --json` — completed; `result_json.collection`=`cortex`, `result_json.source`=`rust`, `chunks_embedded`=4.
- `axon retrieve "rust" --collection "cortex"` — returned `No content found for URL: rust`.
- `axon retrieve "docs/sessions/2026-03-11-api-token-auth-rollback-and-docs.md" --collection "cortex"` — returned 4 chunks.

## 7. Behavior changes (before/after)
- Before: WS/download/output/API middleware included Tailscale and/or SSH-key branches.  
  After: all web auth surfaces use shared API token checks.
- Before: auth docs presented five methods including Tailscale/SSH for current operations.  
  After: active docs present API token, shell token, and MCP OAuth only.
- Before: env templates included dual-auth/SSH-key auth variables.  
  After: those variables removed from active env guidance.

## 8. Verification evidence
| command | expected | actual | status |
|---|---|---|---|
| `cargo fmt --all` | formatting completes without error | command exited successfully | PASS |
| `cargo check` | project compiles | `Finished dev profile [unoptimized + debuginfo]` observed | PASS |
| `rg -n "TAILSCALE\\.md\|SSH-KEY\\.md\|AXON_REQUIRE_DUAL_AUTH\|AXON_SSH_AUTHORIZED_KEYS" ... -g '!docs/sessions/**'` | no active-doc/code references remain | no matches (exit code 1 from `rg`) | PASS |
| `axon embed ... --json` | returns queued/pending embed job with job id | `{"job_id":"9bbe8bb3-64c5-4f08-9631-648b5416a21e","status":"pending"}` | PASS |
| `axon embed status "9bbe8bb3-64c5-4f08-9631-648b5416a21e" --json` | status reaches completed with source metadata | `status=completed`, `collection=cortex`, `source=rust`, `chunks_embedded=4` | PASS |
| `axon retrieve "rust" --collection "cortex"` | retrieve indexed content using reported source id | `No content found for URL: rust` | FAIL |
| `axon retrieve "docs/sessions/2026-03-11-api-token-auth-rollback-and-docs.md" --collection "cortex"` | fallback retrieve by embedded path returns content | returned 4 chunks | PASS |

## 9. Source IDs + collections touched
- Embed job id: `9bbe8bb3-64c5-4f08-9631-648b5416a21e` (completed).
- Collection from status output: `cortex`.
- Source value from status output: `rust` (`result_json.source`).
- Retrieve outcome using status source: failed (`No content found for URL: rust`).
- Retrieve outcome using embedded path + collection: succeeded (4 chunks returned).

## 10. Risks and rollback
- Risk: removing Tailscale/SSH docs may surprise operators relying on those workflows.
- Risk: external automation expecting deleted docs/vars may need updates.
- Rollback path (manual, not executed in this session): reintroduce deleted docs and prior auth branches from repository history if needed.

## 11. Decisions not taken
- Did not edit `docs/sessions/*` historical entries; kept as factual historical records.
- Did not alter shell server token model beyond doc consistency checks.
- Did not run git history operations (`reset`, `revert`, `checkout`) during this work.

## 12. Open questions
- Do any external scripts or deployment docs outside this repo still reference `AXON_REQUIRE_DUAL_AUTH` or `AXON_SSH_AUTHORIZED_KEYS`?
- Should a compatibility note be added for one release cycle to signal that `docs/auth/TAILSCALE.md` and `docs/auth/SSH-KEY.md` were removed?

## 13. Next steps
- Investigate why embed status reports `result_json.source=rust` while retrieve succeeds only with the file path; align source-id semantics in CLI output.
- Keep this session indexed in Axon and linked in Neo4j for future auth rollback context.
