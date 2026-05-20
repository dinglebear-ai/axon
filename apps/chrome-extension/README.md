# Page Scraper Clipboard

Small unpacked Chrome extension that sends the current tab URL to Axon, copies useful outputs to the clipboard, and can queue, poll, and cancel crawl jobs. It includes both a toolbar popup and a Chrome side panel surface.

## Load It

1. Open `chrome://extensions`.
2. Enable **Developer mode**.
3. Click **Load unpacked**.
4. Select this `chrome-page-scraper-extension` directory.
5. Use the popup's **Sidebar** button or Chrome's side panel picker to pin the Axon panel.

## Use It

1. Start Axon locally: `axon serve`.
2. Open any normal web page.
3. Click the extension icon.
4. Use one of the Axon actions.

The default Axon server is `http://100.88.16.79:8001`, the Tailscale address observed for the Linux host where Axon is running. If Chrome and Axon are on the same machine, use `http://127.0.0.1:8001` instead.

Current actions:

- **Capture + crawl**: `POST /v1/scrape`, then `POST /v1/crawl`, then polls `GET /v1/crawl/{job_id}`.
- **Cancel crawl**: `POST /v1/crawl/{job_id}/cancel` for the currently tracked crawl job.
- **Summarize page**: `POST /v1/summarize`.
- **Map URLs**: `POST /v1/map`.
- **Ask Axon**: `POST /v1/ask`.
- **Visible text fallback**: browser DOM text only. This bypasses Axon and is for authenticated or browser-only pages Axon cannot fetch by URL.

## Authentication

If `axon serve` is bound to loopback for local development, Axon allows tokenless HTTP. If the server is non-loopback, uses `AXON_MCP_HTTP_TOKEN`, or runs OAuth/lab-auth mode, the request must authenticate.

For static token auth, paste the `AXON_MCP_HTTP_TOKEN` value into the token field. The extension sends it as:

```http
Authorization: Bearer <token>
```

**Copy visible DOM** is an explicit fallback for logged-in app pages or browser state that Axon cannot fetch by URL. If text is selected on the page, the fallback copies the selection. Otherwise it copies text from `article`, then `main`, then `body`.

Chrome blocks extension scripts on restricted pages like `chrome://extensions`, the Chrome Web Store, and some browser-owned pages.
