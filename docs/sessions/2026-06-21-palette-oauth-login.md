---
date: 2026-06-21 02:31:13 EST
repo: git@github.com:jmagar/axon.git
branch: claude/zealous-agnesi-ffb8e8
head: 86db17da
plan: docs/superpowers/plans/2026-06-21-palette-oauth-login.md
working directory: /home/jmagar/workspace/axon/.claude/worktrees/zealous-agnesi-ffb8e8
worktree: /home/jmagar/workspace/axon/.claude/worktrees/zealous-agnesi-ffb8e8
pr: 248 — feat(palette): OAuth 2.0 Sign in with Google (PKCE + loopback DCR, dual-mode) — https://github.com/jmagar/axon/pull/248
beads: axon_rust-061y (feature, claimed, in progress), axon_rust-gr3k (follow-up, created)
---

# Palette OAuth login (PR #248)

## User Request
"Use the writing-plans skill to create a plan to implement OAuth support in the palette, then run lavra-eng-review and update the plan to address all issues, then execute work-it" — in the existing isolated worktree (no new worktree).

## Session Overview
Added full browser-based OAuth 2.0 "Sign in with Google" login to the Axon Palette Tauri desktop app (`apps/palette-tauri`): Authorization Code + PKCE (S256) with a loopback redirect and RFC 7591 dynamic client registration, run entirely in the Rust shell, coexisting with the existing static bearer token. Delivered via the requested pipeline: `superpowers:writing-plans` → `lavra:lavra-eng-review` (4 agents) → revise plan → `vibin:work-it` (dispatch implementation agent, open PR, post-implementation review waves, fixes). Palette bumped 5.10.4 → 5.11.0. PR #248 open and green on all local gates.

## Sequence of Events
1. **Explore** — parallel agents mapped the palette app (Tauri v2 + React, Rust HTTP bridge, static-token auth, `~/.axon` persistence) and the server OAuth contract (vendored `lab-auth`: discovery, DCR, PKCE S256, form-encoded `/token`, loopback redirects always allowed, conditional refresh tokens). Read key files directly to confirm.
2. **Clarify scope** — asked the user: full browser login (chosen) and OAuth + static token coexisting (chosen).
3. **writing-plans** — wrote a 9-task TDD plan; created tracking bead `axon_rust-061y`.
4. **lavra-eng-review** — 4 agents (architecture/simplicity/security/performance) verified claims against real source. Key must-fixes: single-flight refresh, persist the discovered `token_endpoint`, redacted `Debug`, HTTPS-or-loopback guard, optional `registration_endpoint`. Rewrote the plan (rev 2) folding these in; logged decisions on the bead.
5. **work-it** — committed the plan; prebuilt `target/debug/xtask` (+ `AXON_ALLOW_FALLBACK_WEB_ASSETS=1`) so the pre-commit hook's `xtask-check` stays under its 60s budget; dispatched one implementation agent that executed all 9 tasks via `superpowers:executing-plans` (TDD, one commit per task), green.
6. **PR** — re-verified gates, rebased onto `origin/main`, pushed, opened PR #248.
7. **Post-implementation review** — 5 agents (correctness/silent-failure/security/simplicity/type-design) reviewed the real code; applied the actionable subset in 2 commits; deferred the rest to `axon_rust-gr3k`.
8. **Close-out** — re-ran gates green, pushed, filed follow-up bead, wrote this note.

## Key Findings
- `/authorize` in `vendor/lab-auth/src/authorize.rs:185-211` rejects unknown `client_id`s and pins `redirect_uri` to the registered client → **dynamic client registration is mandatory**; the loopback listener must bind first so the exact post-bind `redirect_uri` is registered.
- Loopback redirects are unconditionally allowed (`authorize.rs:501-513`), scheme must be `http`; the desktop RFC 8252 path needs no Tauri capability/CSP change (browser launch via the `open` crate + a Rust `TcpListener` live outside the webview sandbox).
- The palette webview CSP locks `connect-src` to `'self' ipc:` (`tauri.conf.json`) and all HTTP already flows through the Rust bridge (`axon_bridge.rs:132-139`) — so the OAuth flow had to be Rust-side.
- `/token` is form-encoded (`token.rs:22-25`); refresh tokens are only issued when the upstream IdP returned one (`token.rs:105-143`) — the client handles their absence gracefully.

## Technical Decisions
- **Single-flight refresh via Tauri-managed `OauthState`** (`oauth.rs`): a `tokio::sync::Mutex<CredCache>` caches credentials and is held across the refresh await, so N concurrent expired requests collapse to one `/token` call + one write; a separate `Mutex<()>` serializes interactive logins.
- **Persist the discovered `token_endpoint`** in `StoredCredentials` rather than reconstructing `{server_url}/token` — correct behind reverse proxies where the server's `public_url` differs from the dialed URL.
- **Hand-written redacted `Debug`** on `StoredCredentials` / `TokenResponse` (mirrors `lab-auth`'s `UpstreamOauthCredentialRow`) so tokens never reach logs.
- **`require_secure_url` guard** (https, or http only on loopback) on the server URL and every server-supplied endpoint before use, and on the refresh path — OAuth secrets never cross cleartext to a non-loopback host.
- **Hand-rolled** the loopback listener, PKCE, and the discovery/register/token calls instead of pulling `oauth2`/`tauri-plugin-oauth` — tighter control over the exact `lab-auth` wire contract, minimal new deps (`sha2`, `open`, `tokio` `net/io-util/time`, `uuid` `v4`).
- **Dual-mode precedence**: `resolve_auth_token` prefers a valid OAuth token for the active server, else the static bearer token (`pick_token`).

## Files Changed
| status | path | purpose | evidence |
|---|---|---|---|
| created | apps/palette-tauri/src-tauri/src/oauth.rs | command surface, `OauthState`, single-flight token resolution | cargo test 58 pass |
| created | apps/palette-tauri/src-tauri/src/oauth/pkce.rs (+ _tests) | PKCE verifier/challenge/state | RFC 7636 vector test |
| created | apps/palette-tauri/src-tauri/src/oauth/store.rs (+ _tests) | credential store (0o600, redacted Debug) | store tests pass |
| created | apps/palette-tauri/src-tauri/src/oauth/flow.rs (+ _tests) | discovery/DCR/token + URL validation | flow tests pass |
| created | apps/palette-tauri/src-tauri/src/oauth/callback_server.rs (+ _tests) | loopback redirect capture | callback tests pass |
| created | apps/palette-tauri/src-tauri/src/oauth_tests.rs | precedence + status tests | oauth_tests pass |
| modified | apps/palette-tauri/src-tauri/src/lib.rs | register 3 commands + `.manage(OauthState)` | builds |
| modified | apps/palette-tauri/src-tauri/src/persistence.rs | `atomic_write` → `pub(crate)` | reused by store |
| modified | apps/palette-tauri/src-tauri/src/axon_bridge.rs, stream.rs | resolve OAuth token per request | builds |
| created | apps/palette-tauri/src/lib/oauthClient.ts (+ .test) | frontend wrappers + status formatter | 232 frontend tests pass |
| modified | apps/palette-tauri/src/lib/invoke.ts | browser-dev stubs | typecheck clean |
| modified | apps/palette-tauri/src/components/palette/SettingsPanel.tsx (+ .test) | Authentication block | render test pass |
| modified | apps/palette-tauri/src/styles.css | auth-status styles | lint clean |
| modified | tauri.conf.json, package.json, src-tauri/Cargo.toml, Cargo.lock | 5.10.4 → 5.11.0 | release-version check |
| modified | apps/palette-tauri/README.md | document OAuth login | n/a |
| created | docs/superpowers/plans/2026-06-21-palette-oauth-login.md | the implementation plan (rev 2) | committed 1524ca4b |

## Beads Activity
- **axon_rust-061y** (feature) — created, claimed (in_progress), commented twice (eng-review decisions; completion status). Implementation complete + PR #248 open; left open pending merge.
- **axon_rust-gr3k** (task, P3) — created as follow-up: surface refresh-failure/expired auth to the UI, reactive 401-refresh in the bridge, optional `Secret` newtype for structural token redaction, and bound the `stream.rs` SSE error-body read.

## Repository Maintenance
- **Plans**: the plan lives under `docs/superpowers/plans/` (superpowers convention), not `docs/plans/`; left in place (not moved to `docs/plans/complete/` — different tree, and the work is still in review).
- **Beads**: updated as above. `axon_rust-061y` left open because the PR is not yet merged.
- **Worktrees/branches**: `git worktree list` shows many sibling worktrees owned by other agents/branches (e.g. `agent-*`, `epic-archimedes`, `marketplace-no-mcp`); none touched — unknown ownership / unmerged / protected. This session's worktree and branch are active for PR #248.
- **Stale docs**: none contradicted by this session; the palette README was updated as part of the change.

## Tools and Skills Used
- **Skills**: `superpowers:writing-plans`, `lavra:lavra-eng-review`, `vibin:work-it`, `superpowers:executing-plans` (via the dispatched implementation agent), `vibin:save-to-md`.
- **Subagents**: 2 `Explore` (palette + lab-auth); 4 lavra review agents (plan); 1 `general-purpose` implementation agent; 5 review agents post-implementation (pr-review-toolkit code-reviewer/silent-failure-hunter/code-simplifier/type-design-analyzer + lavra security-sentinel).
- **Shell**: git, cargo (build/test/clippy/fmt), pnpm (test/typecheck/lint), `bd`, `gh`. **File tools**: Read/Write/Edit.
- **Issues**: pre-commit hook `xtask-check` timed out at 60s until `target/debug/xtask` was prebuilt with `AXON_ALLOW_FALLBACK_WEB_ASSETS=1` (known palette gotcha); external review bots (CodeRabbit/Codex/cubic) were rate-limited / out of credits, producing 0 actionable comments.

## Commands Executed
| command | result |
|---|---|
| `AXON_ALLOW_FALLBACK_WEB_ASSETS=1 cargo build -p xtask` | built `target/debug/xtask` (hook fast-path) |
| `cargo test --manifest-path apps/palette-tauri/src-tauri/Cargo.toml` | 58 passed |
| `cargo clippy … --all-targets -- -D warnings` | clean |
| `pnpm test` / `pnpm typecheck` / `pnpm lint` | 232 passed / clean / exit 0 |
| `cargo xtask check-release-versions --mode pr` | palette 5.11.0 only changed component |
| `gh pr create … ` | https://github.com/jmagar/axon/pull/248 |

## Errors Encountered
- **Pre-commit `xtask-check` 60s timeout** — root cause: `target/debug/xtask` absent, so the hook fell back to compiling `cargo xtask` from scratch; `axon` `build.rs` also required built web assets. Resolved by prebuilding xtask with `AXON_ALLOW_FALLBACK_WEB_ASSETS=1` and exporting that flag on every commit/push.

## Behavior Changes (Before/After)
- **Auth**: before — only a static bearer token. After — "Sign in with Google" in Settings → Connection runs a browser OAuth flow; the bridge uses the OAuth token when present (auto-refreshed), else the static token.
- **Settings UI**: new Authentication block showing signed-in / session-expired / different-server / not-signed-in states with a context-appropriate Sign in / Sign out button.

## Verification Evidence
| command | expected | actual | status |
|---|---|---|---|
| `cargo test` (palette) | all pass | 58 passed | pass |
| `cargo clippy -D warnings` | no warnings | clean | pass |
| `pnpm test` | all pass | 232 passed | pass |
| `pnpm typecheck` | clean | clean | pass |
| `cargo xtask check-release-versions --mode pr` | palette-only bump | palette 5.11.0 only | pass |

## Risks and Rollback
- Risk: each login dynamically registers a fresh server-side client (ephemeral-port loopback redirects preclude reuse); rate-limited and operator-bounded server-side. Documented in the plan/README and `axon_rust-gr3k`.
- Rollback: revert the palette commits on `claude/zealous-agnesi-ffb8e8` / close PR #248; no schema or data migration involved; static-token auth is untouched.

## Decisions Not Taken
- Constant-time `state` comparison (negligible local-only timing risk; would add a `subtle` dep to a separate workspace).
- `client_id` reuse across logins (incompatible with ephemeral-port loopback redirects).
- Strict same-host endpoint check (would break legitimate reverse-proxy / `public_url` divergence; scheme is still enforced).
- Folding `pkce.rs` into `flow.rs` (kept as a cohesive, independently-testable unit).

## References
- Plan: docs/superpowers/plans/2026-06-21-palette-oauth-login.md
- PR: https://github.com/jmagar/axon/pull/248
- Server OAuth: vendor/lab-auth/src/{authorize,token,types,routes}.rs

## Open Questions
- External review bots were quota-limited; if the user wants a CodeRabbit pass, it can be retriggered once the org's review limit resets (or via the billing add-on).

## Next Steps
1. Merge PR #248 once reviewed (CI jobs are path-skipped for a palette-only change; local gates are green).
2. After merge, `auto-tag` cuts `palette-v5.11.0` and the palette release workflow builds the artifacts.
3. Optional manual smoke against a live `AXON_MCP_AUTH_MODE=oauth` server (sign in, run an action without a static token, confirm the "Different server" state on URL change).
4. Pick up `axon_rust-gr3k` for the deferred auth-failure UI surfacing / reactive-401 work.
