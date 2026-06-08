# Axon Chrome Extension

An unpacked **Manifest V3** Chrome extension that brings Axon to the page you're
on. The **side panel** is an Aurora-styled launcher: browse the full Axon action
surface (scrape, crawl, ingest, search, query, ask, …), run an action against the
current tab, and read the result inline. Right-click **context menus** ("Scrape
with Axon", "Ingest this page", "Ask Axon about *selection*") pre-fill and run an
action straight from the page.

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
./package.sh   # -> dist/axon-page-scraper-<version>.zip
# or: just package-extension
```

The version is read from `manifest.json`. The `assets/` entry here is a symlink
into the monorepo's top-level `assets/`; "Load unpacked" follows it locally, but
a ZIP must contain real files. `package.sh` ships the runtime files and copies the
referenced assets as real files (no symlinks), omitting dev-only files
(`README.md`, `package.sh`).

## Release It

The extension is released independently of the main axon `v*` releases, on its
own tag. Bump `version` in `manifest.json`, then push a matching tag:

```bash
git tag chrome-ext-v0.2.0   # must match manifest.json's "version"
git push origin chrome-ext-v0.2.0
```

The `chrome-extension-release` workflow builds the zip, checksums it, and
publishes a GitHub Release with `axon-page-scraper-<version>.zip` +
`.sha256` attached. The tag version must match `manifest.json` or the workflow
fails. A manual **Run workflow** (workflow_dispatch) builds the zip as a run
artifact without creating a release (dry-run).

## Side panel — the launcher

Recreated from the Aurora design handoff (`Axon Extension.html`):

- **Brand strip** — Axon mark + a server status dot (green online / red offline)
  + the configured host + a Settings button.
- **This page** card — the current tab URL with quick actions: **Scrape**,
  **Ingest**, **Endpoints**.
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

Right-click a page (or selection) to run Axon without opening the panel first:

- **Scrape with Axon** (`page`, `link`) → `scrape` the page/link URL.
- **Ingest this page into Axon** (`page`) → `ingest` the page URL.
- **Ask Axon about "…"** (`selection`) → `ask` with the selected text.

The background worker opens the side panel and forwards the intent
(`{ type: 'axon-intent', op, arg }`); the launcher pre-fills and runs it.

## Popup actions (toolbar)

The popup chat still supports inline Axon commands and auto-scrape:

- **Scrape + crawl**: `POST /v1/scrape`, then `POST /v1/crawl`, polling
  `GET /v1/crawl/{job_id}`.
- **Auto-scrape visited pages**: optional background mode (Options). Sends
  completed `http(s)` navigations to `POST /v1/scrape`, at most once per URL
  every 24 hours.
- **Cancel crawl**, **Summarize**, **Map**, **Ask**, and the full set of CLI-style
  commands typed into the composer.

## Authentication

If `axon serve` is bound to loopback, Axon allows tokenless HTTP. If the server is
non-loopback, uses `AXON_MCP_HTTP_TOKEN`, or runs OAuth/lab-auth mode, the request
must authenticate. Paste the `AXON_MCP_HTTP_TOKEN` value into the Options token
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
