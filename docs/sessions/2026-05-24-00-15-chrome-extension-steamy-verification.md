---
date: 2026-05-24 00:15:33 EST
repo: git@github.com:jmagar/axon.git
branch: feat/palette-tauri-and-dev-to-body
head: 59f8d14b
working directory: /home/jmagar/workspace/axon_rust
worktree: /home/jmagar/workspace/axon_rust 59f8d14bbdc511e94c1ce688f9a975aa85197d5d [feat/palette-tauri-and-dev-to-body]
---

# Chrome Extension Steamy Verification

## User Request

Diagnose and resolve the Axon Chrome extension failure shown on Steamy, then verify that it actually works rather than relying on inferred backend checks.

## Session Overview

- Updated and deployed the Chrome extension auth-check behavior so settings and side panel probes prove bearer-token acceptance, not just `/healthz` reachability.
- Found and fixed the server-side proxy blocker: SWAG/nginx rejected `chrome-extension://...` origins before requests reached Axon.
- Updated the Steamy Chrome extension storage to use the current Axon token and verified the exact installed extension origin reaches Axon auth.
- Ran a real extension UI smoke test in Chromium: settings auth probe passed, side panel sent `ask what is Axon?`, and Axon returned an answer with sources.
- Could not physically drive the already-open Steamy Chrome side panel because CDP was not listening on Steamy, WSL interop was disabled, and Windows-MCP tools were not exposed in this Codex session.

## Sequence of Events

1. Inspected the reported side-panel failure: repeated `Forbidden by the Axon server or proxy` responses despite the options page saying the API was reachable.
2. Read the Steamy Chrome extension storage LevelDB for extension ID `kllododgecpcdimlgiliginoicbgmeif`, then updated `axonUrl`, `axonToken`, and `autoScrapeEnabled`.
3. Reproduced the distinction between token auth and proxy origin handling: token-only curl reached Axon, while extension-origin requests initially received nginx `403`.
4. Patched SWAG nginx on `squirts` to allow `chrome-extension://[a-p]{32}` origins, validated with `nginx -t`, and reloaded SWAG.
5. Deployed the unpacked extension to `C:\Users\jmaga\axon-chrome-extension` via `rsync`.
6. Verified the extension UI path using a loaded extension in Chromium, then re-ran the check with the token environment correctly loaded after catching a false auth failure in the smoke harness.
7. Checked Steamy and agent-os control surfaces after the user challenged the verification claim.

## Key Findings

- `apps/chrome-extension/options.js:45` now runs `/healthz` and an auth probe; `options.js:85` treats the expected empty-scrape `400` as proof the token was accepted.
- `apps/chrome-extension/popup-api.js:196` mirrors the side-panel API check, and `popup-api.js:211` probes `/v1/scrape` for token acceptance.
- `apps/chrome-extension/background.js:102` now skips cooldown only for successful auto-scrapes, so failed auto-scrapes do not suppress retries for 24 hours.
- SWAG/nginx, not Axon itself, caused the extension-origin `403`; allowing `chrome-extension://...` origins changed the exact Steamy extension origin from forbidden to an auth-passed `400` on the intentional empty scrape.
- Steamy automation remained blocked: `curl http://127.0.0.1:9222/json/version` and `:9223` failed, `/mnt/c/Windows/System32/cmd.exe /c ver` returned `exec format error`, and no `mcp__windows-mcp__*` tools were available.

## Technical Decisions

- Used `/v1/scrape` with an empty URL as the bearer-token probe because Axon returns a clear `400` only after auth succeeds; `/healthz` alone cannot prove token acceptance.
- Allowed extension origins at the proxy layer rather than weakening Axon auth, preserving token enforcement while making Chrome extension CORS/preflight viable.
- Kept the final claim scoped: backend/proxy/extension UI flow verified, but the already-open Steamy Chrome window not physically driven.

## Files Changed

| status | path | previous path | purpose | evidence |
| --- | --- | --- | --- | --- |
| modified | `apps/chrome-extension/options.js` | | Add token-acceptance probe to Settings `Check API` | `options.js:45`, `options.js:85` |
| modified | `apps/chrome-extension/popup-api.js` | | Add side-panel health plus auth probe and friendlier auth/forbidden errors | `popup-api.js:196`, `popup-api.js:211`, `popup-api.js:261` |
| modified | `apps/chrome-extension/background.js` | | Avoid treating failed auto-scrapes as successful cooldown hits | `background.js:102` |
| created | `docs/sessions/2026-05-24-chrome-extension-steamy-verification.md` | | Capture this verification and follow-up session | this file |
| modified | `/mnt/appdata/swag/nginx/mcp-server.conf` | | External host config: allow Chrome extension origins through SWAG | validated with `docker exec swag nginx -t` and reload |
| modified | `C:\Users\jmaga\axon-chrome-extension\*` | | Deployed unpacked extension files to Steamy | `rsync -a --delete -e ssh apps/chrome-extension/ steamy-wsl:/mnt/c/Users/jmaga/axon-chrome-extension/` |

## Beads Activity

No bead activity observed for this session. `bd list --all --sort updated --reverse --limit 20 --json` was read during saveout and returned historical closed issues, but none were directly tied to the Chrome extension verification work.

## Repository Maintenance

- Plans checked: `find docs/plans -maxdepth 1 -type f` showed open plan files, but none were clearly completed by this Chrome extension verification session, so no files were moved to `docs/plans/complete/`.
- Beads checked: recent Beads were read; no directly relevant bead was created, edited, or closed during this session.
- Worktrees checked: `git worktree list --porcelain` showed the main worktree plus `async-prepared-session-ingest`, `axon-status-trim`, and `rest-api-canonical-contracts`. No cleanup was performed because those worktrees map to active branches and were not proven stale.
- Branches checked: local and remote branch lists showed active remote-tracking branches. No branch cleanup was performed.
- Stale docs checked narrowly: the session changed extension behavior and external proxy config; no in-repo doc was identified as contradicted by the observed implementation during the saveout.

## Tools and Skills Used

- Skills: `save-to-md` for this session note; `agent-os`, `chrome`, and `screenshots` guidance was consulted during the verification challenge.
- Shell commands: SSH to `steamy-wsl`, SSH to `dookie`, curl probes, git state inspection, `bd list`, Node smoke scripts, and LevelDB reads/writes via `classic-level`.
- External CLIs/services: SWAG nginx inside Docker on `squirts`, Chromium with extension loading, Xvfb, Axon HTTP API at `https://axon.tootie.tv`.
- Browser tools: Chrome DevTools Protocol was available for the automated Chromium smoke test, but not for the already-open Steamy Chrome session.
- MCP tools: Windows-MCP was requested by the user but unavailable in this Codex tool session.

## Commands Executed

| command | result |
| --- | --- |
| `ssh steamy-wsl 'curl -fsS --max-time 2 http://127.0.0.1:9222/json/version \|\| true; curl ...:9223...'` | Both Steamy CDP checks failed to connect. |
| `ssh steamy-wsl '/mnt/c/Windows/System32/cmd.exe /c ver'` | Failed with `exec format error`; WSL interop was disabled. |
| `curl -X POST https://axon.tootie.tv/v1/scrape -H 'Origin: chrome-extension://kllododgecpcdimlgiliginoicbgmeif' ... --data '{}'` | Returned `400 {"kind":"bad_request","message":"url or urls is required"}`, proving auth reached Axon. |
| `set -a; source ~/.axon/.env; node /tmp/axon-extension-full-smoke.mjs` | Extension settings probe returned `ok`; side panel asked Axon and received a sourced answer. |
| `docker exec swag nginx -t` | SWAG nginx config syntax validated before reload. |
| `rsync -a --delete -e ssh apps/chrome-extension/ steamy-wsl:/mnt/c/Users/jmaga/axon-chrome-extension/` | Deployed current extension files to Steamy unpacked-extension directory. |

## Errors Encountered

- Initial options-page "reachable" signal was misleading because it only proved `/healthz` access, not bearer-token acceptance.
- Extension-origin requests initially failed with nginx `403`; root cause was missing `chrome-extension://...` origin allowance in SWAG config.
- A rerun of `/tmp/axon-extension-full-smoke.mjs` failed auth once because the shell had not sourced `~/.axon/.env`, leaving `process.env.AXON_MCP_HTTP_TOKEN` empty for the smoke harness. Re-running with the env loaded passed.
- Live Steamy UI automation was blocked by unavailable CDP, disabled WSL interop, and missing Windows-MCP tools.

## Behavior Changes (Before/After)

| before | after |
| --- | --- |
| Settings could report API reachable even with an invalid or missing token. | Settings reports success only after health and token-acceptance probes pass. |
| Side panel showed repeated forbidden/auth failures against `https://axon.tootie.tv`. | Extension UI smoke test successfully asked Axon and received a sourced answer. |
| Failed auto-scrape attempts could suppress retries during the cooldown window. | Only successful prior scrapes trigger the cooldown skip. |
| SWAG rejected Chrome extension origins. | SWAG allows valid Chrome extension origins to reach Axon auth. |

## Verification Evidence

| command | expected | actual | status |
| --- | --- | --- | --- |
| `node --check apps/chrome-extension/options.js apps/chrome-extension/background.js` and related deployed-file checks | JavaScript syntax valid | Passed during implementation/deploy verification | pass |
| Exact Steamy origin curl to `/v1/scrape` with empty body | `400` bad request after auth, not `401` or `403` | `400 {"kind":"bad_request","message":"url or urls is required"}` | pass |
| Extension UI smoke with env-loaded token | Settings probe `ok`, side panel gets Axon answer | Returned answer beginning "Axon is a trimodal application..." with sources | pass |
| Steamy CDP check on `9222`/`9223` | CDP endpoint available for live UI automation | Connection refused | blocked |
| Steamy Windows interop check via `cmd.exe` | Windows command runner usable from WSL | `exec format error` | blocked |

## Risks and Rollback

- SWAG origin regex is intentionally broad for Chrome extension IDs: `chrome-extension://[a-p]{32}`. Roll back by restoring `/mnt/appdata/swag/nginx/mcp-server.conf.bak-20260523231024` and reloading SWAG.
- Extension deployment to `C:\Users\jmaga\axon-chrome-extension` used `rsync --delete`; rollback by redeploying a known-good extension directory or checking out the previous commit and re-running the same rsync.
- The already-open Steamy Chrome profile may need an extension reload to pick up deployed unpacked-extension changes if Chrome has not reloaded the unpacked extension.

## Decisions Not Taken

- Did not claim the already-open Steamy side panel was physically verified, because no working control channel existed.
- Did not weaken Axon auth to work around proxy failures; the proxy was fixed to pass extension-origin requests through to normal bearer auth.
- Did not delete worktrees or branches during saveout because none were proven safe to remove.

## Open Questions

- Whether the already-open Steamy Chrome extension instance has reloaded the deployed unpacked-extension files.
- Whether the next Codex/Claude session can expose `mcp__windows-mcp__*` tools or Steamy Chrome CDP so the live personal browser can be driven directly.

## Next Steps

- For final live Steamy proof, launch Chrome with remote debugging on Steamy or expose Windows-MCP tools, then drive the installed side panel and send an `ask` message in that exact browser session.
- If the current visible Steamy Chrome still shows old behavior, reload the unpacked extension from `chrome://extensions` or restart the debug Chrome session and re-run the side-panel check.
- Keep the auth probe behavior in Settings and side panel; it catches the precise failure class that caused this session.
