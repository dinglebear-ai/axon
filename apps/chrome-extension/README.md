# Axon Chrome Extension

> Current pre-#298 client docs. The future extension contract is
> `docs/pipeline-unification/surfaces/chrome-extension-contract.md`; after the
> source-pipeline cutover this extension should consume shared REST/API
> contracts rather than owning ingestion semantics.

An unpacked **Manifest V3** Chrome extension that brings Axon to the page you're
on. The **side panel** is an Aurora-styled launcher: browse the full Axon action
surface (scrape, crawl, extract, search, query, ask, …), run an action against the
current tab, and read the result inline. Right-click **context menus** ("Scrape
with Axon (copy markdown)", "Crawl this page with Axon", "Ask Axon about
*selection*") pre-fill and run an action straight from the page.

The toolbar **popup** keeps the older command-line chat surface for quick CLI-style
commands; both talk to the same Axon `/v1/*` HTTP API.

## Load It

1. Open `chrome://extensions`.
2. Enable **Developer mode**.
3. Click **Load unpacked** and select this `apps/chrome-extension` directory.
4. Open the extension **Options** to set the Axon server URL + bearer token.
5. Click the toolbar icon (or press **Ctrl/⌘+Shift+Space**) to open the side panel,
   or pick *Axon* from Chrome's side-panel menu.

No build step — the side panel is plain HTML/CSS/JS on the Aurora design tokens
(`aurora.css`), so the unpacked directory loads as-is. (MV3 forbids remote code,
so everything ships in the package.)

## Package It

For a distributable ZIP (sharing or Chrome Web Store upload):

```bash
./package.sh   # -> dist/axon-<version>.zip
# or: just package-extension
```

The version is read from `manifest.json`. Runtime assets live under this
extension directory as real files so release-please sees Chrome release changes
inside one package path. `package.sh` ships the runtime files and copies the
referenced assets into the staging directory, omitting dev-only files
(`README.md`, `package.sh`).

## Release It

The extension is released independently of the main axon `v*` releases, on its
own tag. Bump `version` in `manifest.json`, then push a matching tag:

```bash
git tag chrome-ext-v0.2.1   # must match manifest.json's "version"
git push origin chrome-ext-v0.2.1
```

The `chrome-extension-release` workflow builds the zip, checksums it, and
publishes a GitHub Release with `axon-<version>.zip` +
`.sha256` attached. The tag version must match `manifest.json` or the workflow
fails. A manual **Run workflow** (workflow_dispatch) builds the zip as a run
artifact without creating a release (dry-run).

## Side panel — the launcher

Recreated from the Aurora design handoff (`Axon Extension.html`):

- **Brand strip** — Axon mark + a server status dot (green online / red offline)
  + the configured host + a Settings button.
- **This page** card — the current tab URL with quick actions: **Scrape**,
  **Crawl**, **Extract**.
- **Action browse** — every action grouped by family (Fetch & Read · Crawl &
  Ingest · Search & Discover · Reason · System), color-coded by tone
  (cyan = read, orange = async jobs, rose = LLM). `ASYNC` badges mark lifecycle
  jobs.
- **Run → result** — tapping an action opens a result view with an editable
  arg bar (URL prefilled from the active tab, or a query field) and renders the
  real response: ranked query hits, web results, the sources library → **doc
  viewer**, stats, doctor, status, accepted-job cards, brand palettes, endpoint
  lists, diffs, screenshots, and a generic JSON fallback for anything else.

Server URL + bearer token are read from `chrome.storage` (set on the Options
page), shared with the popup.

## Context menus

Right-click a page (or selection) to run Axon:

- **Scrape with Axon (copy markdown)** (`page`, `link`) → `scrape` the page/link URL and copy markdown.
- **Crawl this page with Axon** (`page`) → `crawl` the page URL.
- **Ask Axon about "…"** (`selection`) → `ask` with the selected text.

Scrape and Crawl run directly from the background worker. Scrape uses a bundled
offscreen document for the clipboard write, so no side panel or popup has to
open. Both actions flash the extension badge and show a Chrome notification.
Ask opens the side panel because the response needs a visible reading surface.

## Agent OS regression

Run the end-to-end Windows/Chrome smoke test from the repo root:

```bash
scripts/test-chrome-extension-agent-os.sh
```

The harness packages the current extension, serves the zip plus local Axon config
to `agent-os`, installs the latest Windows `axon.exe`, loads the extension into
Chrome, configures it, runs the installed Scrape/Crawl handlers, then verifies
clipboard markdown and `axon status`/`crawl list` output. The native Chrome
context menu itself is not clicked because Windows-MCP cannot currently select
Chrome's native right-click menu reliably; the test invokes the exact extension
background handlers that the context menu dispatches.

## Popup actions (toolbar)

The popup chat still supports inline Axon commands and auto-scrape:

- **Scrape + crawl** (current pre-cutover routes): `POST /v1/scrape`, then `POST /v1/crawl`, polling
  `GET /v1/crawl/{job_id}`.
- **Auto-scrape visited pages**: optional background mode (Options). Sends
  completed `http(s)` navigations to `POST /v1/scrape`, at most once per URL
  every 24 hours.
- **Cancel crawl**, **Summarize**, **Map**, **Ask**, and the full set of CLI-style
  commands typed into the composer.

## Authentication

If `axon serve` is bound to loopback, Axon allows tokenless HTTP. If the server is
non-loopback, uses `AXON_HTTP_TOKEN`, or runs OAuth/lab-auth mode, the request
must authenticate. Paste the `AXON_HTTP_TOKEN` value into the Options token
field; the extension sends it as:

```http
Authorization: Bearer <token>
```

The default Axon server is `http://100.88.16.79:8001` (a Tailscale address);
configure it from Options. If Chrome and Axon are on the same machine, use
`http://127.0.0.1:8001`.

Chrome blocks extension scripts on restricted pages like `chrome://extensions`,
the Chrome Web Store, and some browser-owned pages.

## Files

| File | Purpose |
|---|---|
| `manifest.json` | MV3 manifest (side panel, options, background, context menus, command) |
| `background.js` | service worker — side-panel behavior, context menus, auto-scrape |
| `sidepanel.html` | side-panel launcher entry |
| `aurora.css` | Aurora design tokens (dark-first, `.light` remap) |
| `launcher.css` | launcher layout + the `ext-*` styles from the design |
| `launcher-icons.js` | lucide-style icon set + the Axon neuron mark |
| `launcher-data.js` | the action catalog + tone helpers |
| `launcher-render.js` | DOM renders for each action result + response normalizers |
| `launcher.js` | controller — config, `/v1/*` requests, browse → run → doc flow |
| `popup.html` + `popup-*.js` | toolbar popup (command chat) |
| `options.html` + `options.js` | server URL + token settings |
| `package.sh` | build a distributable zip — see [Package It](#package-it) |
