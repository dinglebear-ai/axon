---
date: 2026-05-20 18:09:36 EDT
repo: git@github.com:jmagar/axon.git
branch: main
head: af13c72a
working directory: /home/jmagar/workspace/axon_rust
worktree: /home/jmagar/workspace/axon_rust af13c72a [main]
---

# Chrome Extension Ask Verification

## User Request

Install the Axon Chrome extension into the Windows-accessible Chrome session, test `ask`, and save the work to markdown.

## Session Overview

- Installed the unpacked Chrome extension into the Windows MCP-visible debug Chrome profile.
- Verified Chrome DevTools Protocol on `127.0.0.1:9222`.
- Tested the extension's `ask` flow against the real Axon `/v1/ask` endpoint.
- Fixed markdown inline emphasis rendering for assistant answers.

## Sequence of Events

1. Used Windows MCP to load `C:\Users\Docker\axon-chrome-extension` through `chrome://extensions` with Developer Mode enabled.
2. Confirmed Chrome assigned extension ID `olhlogeonhmnpjkjkkjbkmojjafgfgpp`.
3. Opened `chrome-extension://olhlogeonhmnpjkjkkjbkmojjafgfgpp/sidepanel.html` through CDP and verified the `Axon Chat` UI rendered.
4. Sent `ask what is Axon?` through the UI; first run failed with a missing bearer token error.
5. Configured extension storage with `axonUrl=http://100.88.16.79:8001` and the local `AXON_MCP_HTTP_TOKEN`, then reran the ask.
6. Verified `/v1/ask` returned a real RAG answer with sources and rendered in the chat.
7. Patched markdown rendering for bold and italic inline content, redeployed the unpacked extension folder, reloaded Chrome, and confirmed the answer rendered without literal `**...**`.

## Key Findings

- The Windows MCP-visible Chrome profile lives under `C:\Users\Docker`; it is not the same filesystem view as `steamy-wsl:/mnt/c/Users/jmaga`.
- Chrome 148 exposed CDP successfully from the cloned debug profile on `127.0.0.1:9222`.
- The installed extension ID was `olhlogeonhmnpjkjkkjbkmojjafgfgpp`.
- Auto-scrape cooldown is implemented in `apps/chrome-extension/background.js:1` through `apps/chrome-extension/background.js:41`.
- Inline markdown emphasis was missing from `renderInline`; the fix is in `apps/chrome-extension/popup.js:1743`.

## Technical Decisions

- Used the unpacked extension install path because the task was to test current local extension code, not a Web Store package.
- Used Chrome extension storage for the Axon URL/token to match the extension's runtime configuration path.
- Kept the markdown renderer fix local to the lightweight custom renderer instead of adding a dependency.

## Files Modified

- `apps/chrome-extension/popup.js`: Added inline code-preserving bold and italic markdown rendering.
- `apps/chrome-extension/popup.css`: Added strong/em styling using existing Aurora tokens.
- `docs/sessions/2026-05-20-chrome-extension-ask-verification.md`: Captured this session note.

## Commands Executed

- `node --check apps/chrome-extension/popup.js && node --check apps/chrome-extension/background.js && node --check apps/chrome-extension/options.js`: JavaScript syntax checks passed.
- `python3 -m http.server 8766 --bind 0.0.0.0 --directory /tmp`: Temporarily served the extension ZIP to Windows.
- Windows PowerShell `Invoke-WebRequest` + `Expand-Archive`: copied the refreshed extension into `C:\Users\Docker\axon-chrome-extension`.
- Windows PowerShell CDP calls against `http://127.0.0.1:9222/json`: verified extension targets and opened the side panel.

## Errors Encountered

- Initial `ask what is Axon?` failed with: `Auth failed. The extension needs the Axon bearer token for this server. Run auth or open Settings.`
  - Root cause: the Windows debug Chrome profile had no `axonToken` stored.
  - Resolution: configured `axonUrl` and `axonToken` in `chrome.storage.local`, then reran the ask successfully.
- First redeploy ZIP extracted files directly under `C:\Users\Docker` instead of under `C:\Users\Docker\axon-chrome-extension`.
  - Root cause: ZIP archive did not include the top-level folder.
  - Resolution: rebuilt the ZIP from `/tmp/axon-chrome-extension` with the folder included and redeployed.

## Behavior Changes

### Before

- `ask` failed in the installed extension profile without a bearer token.
- Assistant answers rendered bold markdown literally, for example `**CLI:**`.

### After

- `ask what is Axon?` returned a real `/v1/ask` answer with RAG sources.
- Bold and italic markdown render as formatted text in assistant messages.

## Verification Evidence

| command | expected | actual | status |
| --- | --- | --- | --- |
| Windows MCP Snapshot after install | Axon extension visible in `chrome://extensions` | `Axon Page Scraper` 0.1.0 visible with ID `olhlogeonhmnpjkjkkjbkmojjafgfgpp` | pass |
| CDP `/json` target query | Axon extension runtime target exists | `chrome-extension://olhlogeonhmnpjkjkkjbkmojjafgfgpp/sidepanel.html` and service worker target observed | pass |
| Extension UI `ask what is Axon?` before token | Auth failure if token missing | Rendered bearer-token auth error | pass |
| Extension UI `ask what is Axon?` after token | Real Axon answer with sources | Returned trimodal Axon/RAG answer with `[S1]`, `[S2]`, `[S3]` sources | pass |
| `node --check` on extension scripts | No syntax errors | `popup.js`, `background.js`, and `options.js` passed | pass |

## Risks and Rollback

- Risk: the installed extension is in the Windows MCP-visible debug Chrome profile, not necessarily the user's normal Chrome profile.
- Risk: the bearer token was configured only in that Chrome profile's extension storage.
- Rollback: revert `apps/chrome-extension/popup.js` and `apps/chrome-extension/popup.css`, then reload the unpacked extension.

## Open Questions

- Whether the user's normal Steamy Chrome profile should also receive the refreshed unpacked extension install.
- Whether extension settings should be seeded automatically from Axon config rather than manually entered or injected per browser profile.

## Next Steps

- Started but not completed: none.
- Follow-on: test an `ask` command from an actual `http://` or `https://` target tab so the target-card page context is populated.
- Follow-on: decide whether to package the extension or keep using the unpacked developer install workflow.
