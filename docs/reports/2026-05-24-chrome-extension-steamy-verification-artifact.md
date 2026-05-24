# Chrome Extension Steamy Verification Artifact

Generated: 2026-05-24

Source session: `docs/sessions/2026-05-24-chrome-extension-steamy-verification.md`

## Conclusion

The Axon Chrome extension failure was traced to SWAG/nginx rejecting Chrome extension origins before requests reached Axon. The proxy was updated to allow valid Chrome extension origins, the extension now probes bearer-token acceptance instead of only `/healthz`, and an automated Chromium extension smoke test successfully sent an Axon question and received a sourced answer.

The already-open Steamy Chrome side panel was not physically driven. That final live-browser proof remains blocked until Steamy Chrome exposes CDP or Windows-MCP tools are available.

## Verified

- Settings `Check API` now requires both `/healthz` reachability and bearer-token acceptance.
- Side-panel API checks now include a token-acceptance probe through `/v1/scrape`.
- Failed auto-scrapes no longer count as successful cooldown events.
- The exact Steamy extension origin `chrome-extension://kllododgecpcdimlgiliginoicbgmeif` reached Axon auth after the SWAG change.
- An automated Chromium session loaded the extension, passed the settings probe, submitted `ask what is Axon?`, and received a sourced answer.

## Not Verified

- The already-open personal Steamy Chrome side panel was not directly exercised.
- It is not proven that the currently visible Steamy Chrome instance has reloaded the deployed unpacked-extension files.

## Evidence

| check | result |
| --- | --- |
| Exact extension-origin curl to `/v1/scrape` with empty body | Returned `400 {"kind":"bad_request","message":"url or urls is required"}`, which proves auth succeeded before request validation. |
| Extension UI smoke with env-loaded token | Settings probe returned `ok`; side panel returned an Axon answer with sources. |
| Steamy CDP probes on ports `9222` and `9223` | Connection refused. |
| Steamy WSL Windows interop probe via `cmd.exe` | Failed with `exec format error`. |

## Changed Surfaces

- Repo files:
  - `apps/chrome-extension/options.js`
  - `apps/chrome-extension/popup-api.js`
  - `apps/chrome-extension/background.js`
- External host config:
  - `/mnt/appdata/swag/nginx/mcp-server.conf`
- Deployed unpacked extension path:
  - `C:\Users\jmaga\axon-chrome-extension`

## Rollback

- Restore `/mnt/appdata/swag/nginx/mcp-server.conf.bak-20260523231024`, then reload SWAG.
- Redeploy a known-good extension directory to `C:\Users\jmaga\axon-chrome-extension`, or check out the previous Axon commit and rerun the extension `rsync`.

## Next Proof Step

Launch or expose a controllable Steamy Chrome session, then drive the installed side panel in that exact browser profile and submit an Axon question.

Acceptable control paths:

- Start Steamy Chrome with remote debugging enabled and connect to CDP.
- Expose Windows-MCP tools in the next Codex/Claude session.
- Manually reload the unpacked extension from `chrome://extensions`, then run a visible side-panel ask test.

Pass condition: the installed Steamy side panel sends an ask request and displays a sourced Axon answer without `401` or `403` errors.
