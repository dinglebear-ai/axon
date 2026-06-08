# Axon Page Scraper

Small unpacked Chrome extension that sends the current tab URL to Axon, copies useful outputs to the clipboard, and can queue, poll, and cancel crawl jobs. It includes both a toolbar popup and a Chrome side panel surface.

## Load It

1. Open `chrome://extensions`.
2. Enable **Developer mode**.
3. Click **Load unpacked**.
4. Select this `apps/chrome-extension` directory.
5. Open extension **Options** to set the Axon URL/token.
6. Use the popup's **Sidebar** button or Chrome's side panel picker to pin the Axon panel.

## Package It

For a distributable ZIP (sharing or Chrome Web Store upload):

```bash
./package.sh   # -> dist/axon-page-scraper-<version>.zip
# or: just package-extension
```

The version is read from `manifest.json`. The `assets/` entry here is a symlink
into the monorepo's top-level `assets/`; "Load unpacked" follows it locally, but
a ZIP must contain real files. `package.sh` copies only the referenced icons as
real files (no symlinks) and omits dev-only files (`README.md`, `package.sh`).

## Release It

The extension is released independently of the main axon `v*` releases, on its
own tag. Bump `version` in `manifest.json`, then push a matching tag:

```bash
git tag chrome-ext-v0.1.0   # must match manifest.json's "version"
git push origin chrome-ext-v0.1.0
```

The `chrome-extension-release` workflow builds the zip, checksums it, and
publishes a GitHub Release with `axon-page-scraper-<version>.zip` +
`.sha256` attached. The tag version must match `manifest.json` or the workflow
fails. A manual **Run workflow** (workflow_dispatch) builds the zip as a run
artifact without creating a release (dry-run).

## Use It

1. Start Axon locally: `axon serve`.
2. Open any normal web page.
3. Click the extension icon.
4. Use one of the Axon actions.

The default Axon server is `http://100.88.16.79:8001`, the Tailscale address observed for the Linux host where Axon is running. Configure it from the extension options page. If Chrome and Axon are on the same machine, use `http://127.0.0.1:8001` instead.

Current actions:

- **Scrape + crawl**: `POST /v1/scrape`, then `POST /v1/crawl`, then polls `GET /v1/crawl/{job_id}`.
- **Auto-scrape visited pages**: optional background mode from Options. Sends completed `http://` and `https://` navigations to `POST /v1/scrape`, at most once per URL every 24 hours.
- **Cancel crawl**: `POST /v1/crawl/{job_id}/cancel` for the currently tracked crawl job.
- **Summarize page**: `POST /v1/summarize`.
- **Map URLs**: `POST /v1/map`.
- **Ask Axon**: `POST /v1/ask`.
## Authentication

If `axon serve` is bound to loopback for local development, Axon allows tokenless HTTP. If the server is non-loopback, uses `AXON_MCP_HTTP_TOKEN`, or runs OAuth/lab-auth mode, the request must authenticate.

For static token auth, paste the `AXON_MCP_HTTP_TOKEN` value into the options page token field. The extension sends it as:

```http
Authorization: Bearer <token>
```

Chrome blocks extension scripts on restricted pages like `chrome://extensions`, the Chrome Web Store, and some browser-owned pages.
