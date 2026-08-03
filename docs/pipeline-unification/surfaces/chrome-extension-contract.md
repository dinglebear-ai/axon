# Chrome Extension Contract
Last Modified: 2026-06-30

## Contract

`apps/chrome-extension/` is a first-class capture and browser-context surface
for Axon.

The extension may collect user-approved page context, screenshots, selected
text, links, metadata, and browser-visible artifacts, then submit them to Axon
through REST upload/source routes. It must not implement crawling, source
resolution, vector storage, graph storage, memory lifecycle, or LLM synthesis
locally.

## Product Role

The Chrome extension exists to make browser context easy to send to Axon:

- capture the current page
- capture selected text
- capture page metadata and links
- capture screenshots when approved
- submit a page/source for indexing
- ask/query against Axon with current-page context
- save useful browser context as memory
- show job progress for submitted sources

The extension is not a crawler. Multi-page crawl, render fallback, sitemap
discovery, extraction, graph generation, embedding, and cleanup remain server
pipeline work.

## Ownership Boundary

| Area | Extension Owns | Shared Axon Owns |
|---|---|---|
| Browser context | active tab URL/title/selection/screenshot capture | canonical source identity and acquisition |
| Permissions | extension permission prompts and host permissions | server auth and source security policy |
| Source submission | user intent and request construction | resolver, adapter, ledger, jobs, embeddings |
| Uploads | staged page bundles/screenshots | artifact storage, source creation from upload |
| Ask/query UI | small browser popup/sidepanel UX | retrieval, memory, graph, LLM synthesis |
| Progress | job/event display | job store and `SourceProgressEvent` |

## Required API Surface

| Feature | Routes |
|---|---|
| bootstrap | `GET /v1/server`, `GET /v1/capabilities`, `GET /readyz` |
| source submit | `POST /v1/sources` |
| upload bundle | `POST /v1/uploads`, `PUT /v1/uploads/{upload_id}/content`, `POST /v1/uploads/{upload_id}/complete` |
| job progress | `GET /v1/jobs/{job_id}`, `GET /v1/jobs/{job_id}/events`, `GET /v1/jobs/{job_id}/stream` |
| ask/query | `POST /v1/query`, `POST /v1/ask`, `POST /v1/ask/stream` |
| memory | `POST /v1/memories`, `POST /v1/memories/context` |
| artifacts | `GET /v1/artifacts/{artifact_id}`, `GET /v1/artifacts/{artifact_id}/content` |

The extension must use shared DTOs or schema-generated client types.

## Required Extension Modules

The target extension must expose these boundaries or equivalents:

```text
apps/chrome-extension/
  src/api/             # generated REST/SSE client
  src/auth/            # server URL/token storage
  src/capture/         # active tab, selection, screenshot, link capture
  src/redaction/       # lightweight client-side pre-redaction
  src/popup/           # quick actions
  src/sidepanel/       # ask/query/progress UI
  src/background/      # message routing, upload orchestration
  src/options/         # server/auth settings
  src/styles/          # generated presentation tokens
```

Required interfaces:

```ts
export interface BrowserCaptureProvider {
  getActivePage(): Promise<CapturedPageRef>
  getSelection(): Promise<CapturedSelection | null>
  captureVisibleScreenshot(): Promise<CapturedScreenshot>
  collectLinks(): Promise<CapturedLink[]>
}

export interface AxonExtensionClient {
  submitSource(request: SourceRequest): Promise<SourceResult>
  createUpload(request: UploadCreateRequest): Promise<UploadDescriptor>
  putUploadContent(uploadId: string, content: UploadContent): Promise<UploadResult>
  completeUpload(uploadId: string, request: UploadCompleteRequest): Promise<UploadResult>
  ask(request: AskRequest): Promise<AskResult>
  remember(request: MemoryRememberRequest): Promise<MemoryResult>
}
```

Capture providers return browser-visible context only. They do not expose
cookies, auth headers, local storage, session storage, or extension internals.

## Capture Contract

Supported capture modes:

| Mode | Captured Data | Server Path |
|---|---|---|
| `page_url` | URL, title, referrer when safe | `POST /v1/sources` with web source |
| `selection` | selected text, URL, title, DOM locator when available | upload artifact or memory/source request |
| `page_snapshot` | sanitized HTML/markdown/text bundle | upload complete then source from upload |
| `screenshot` | user-approved image capture | upload artifact or screenshot route |
| `links` | page links/resource hints | map/source request |
| `ask_context` | current URL/title/selection as retrieval filter/context hint | ask/query request |

Rules:

- user action is required before capturing page content or screenshots
- URL-only source submission may be one-click when the active tab URL is visible
- sensitive schemes such as `chrome://`, `file://`, extension pages, and
  browser internal URLs are blocked unless a future trusted local mode defines
  them explicitly
- cookies, auth headers, local storage, session storage, and browser secrets are
  never captured
- page snapshots are redacted before upload when client-side detectors can do so
  and always redacted again server-side

## Permission Contract

Extension permissions stay minimal.

Required permission principles:

- request host permissions only when needed
- prefer `activeTab` for user-triggered capture
- do not request broad browsing history by default
- do not capture all tabs continuously
- do not run background crawls
- make server URL and auth token configuration explicit

## Source and Memory Behavior

When the user chooses "index this page," the extension sends:

```json
{
  "source": "https://example.com/current-page",
  "scope": "page",
  "embed": true,
  "watch": "disabled",
  "wait": false
}
```

When the user chooses "save as memory," the extension sends a memory request,
not a source request, unless the user explicitly asks to index the page.

Rules:

- page capture and memory capture are distinct user intents
- current-page ask may include URL/source filters but does not index by default
- source submission returns a job descriptor and progress route
- repeated capture uses idempotency keys when possible

## Security Contract

The extension must:

- store tokens using browser extension storage with least exposure
- redact tokens and secrets from logs
- show the target Axon server URL before first connection
- validate TLS/public origin for remote servers
- require explicit confirmation before uploading page snapshots/screenshots
- honor server visibility/redaction metadata

The extension must not:

- exfiltrate page content automatically
- collect cookies, auth headers, or local storage
- call Qdrant/TEI/LLM providers directly
- bypass server-side source security checks
- embed page content locally

## Presentation Contract

The extension uses the shared presentation contract for:

- status colors
- warning/error/degraded states
- compact popup density
- sidepanel typography
- icons and semantic labels
- dark/light theme behavior

Extension UI should be compact and task-focused. It should not become a second
full web panel.

## Testing Contract

Required tests:

- DTO/client fixtures validate against API schemas
- blocked URL schemes cannot be captured
- selection capture omits cookies/local storage
- page URL source submission creates expected `SourceRequest`
- upload flow uses upload routes and idempotency
- ask with current-page context does not index by default
- progress event fixture renders phases/statuses
- permission prompts are required before snapshot/screenshot capture

## Acceptance Criteria

- extension can submit active page URLs and uploaded page bundles through Axon
  REST routes
- extension can ask/query with current-page context without indexing by default
- extension never captures browser secrets
- extension does not implement crawling, embedding, graph, memory lifecycle, or
  provider calls locally
- extension UI uses shared presentation/status semantics
