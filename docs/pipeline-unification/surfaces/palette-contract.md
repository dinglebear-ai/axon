# Palette Desktop Contract
Last Modified: 2026-06-30

## Contract

`apps/palette-tauri/` is the desktop Palette app surface for Axon.

Palette owns desktop UX, Tauri windowing, local shell integration approved by
policy, and rich inspection workflows. It consumes the shared Axon API through
REST/SSE and generated DTOs. It must not fork the source pipeline, provider
stack, vector store, graph store, memory store, or job runtime.

## Product Role

Palette is the power-user desktop control surface for:

- source/job/watch monitoring
- local source submission
- ask/query/retrieve workflows
- graph exploration
- memory review
- artifact inspection
- provider health and doctor output
- configuration editing with validation
- desktop-friendly upload/drop workflows

Palette is not a second Axon server.

## Ownership Boundary

| Area | Palette Owns | Shared Axon Owns |
|---|---|---|
| Desktop shell | Tauri app, windows, menus, tray, native dialogs | server process, API, auth |
| UI | layout, panels, keyboard shortcuts, views | DTOs, status semantics, permissions |
| Local files | picker/drop UX and upload staging | source resolution, acquisition, ledger |
| Config editor | editing experience, diff preview, validation display | config schema, env schema, reload behavior |
| Jobs | dashboards, progress rendering, cancellation prompts | job store, events, retries, recovery |
| Artifacts | preview/download/open UX | artifact store, retention, redaction |
| Graph | visualization and filters | graph schema, query execution, evidence |
| Memory | review UI, pin/archive/forget UX | memory lifecycle, decay, embeddings |

Palette may launch or connect to a local Axon server only through explicit
bootstrap/runtime controls. It must treat the server API as authoritative.

## Required API Surface

Palette depends on:

| Feature | Routes |
|---|---|
| bootstrap | `GET /v1/server`, `GET /v1/capabilities`, `GET /v1/providers`, `GET /v1/status`, `GET /v1/doctor` |
| source lifecycle | `POST /v1/sources`, source detail/list/item/document/generation routes |
| watch lifecycle | `/v1/watches*` |
| jobs | `/v1/jobs*` and SSE streams |
| retrieval | `/v1/query`, `/v1/retrieve`, `/v1/ask`, `/v1/ask/stream` |
| graph | `/v1/graph/*` |
| memory | `/v1/memories/*` |
| artifacts/uploads | `/v1/artifacts/*`, `/v1/uploads/*` |
| operations | `/v1/prune/*`, `/v1/collections*`, provider routes |
| config | panel/config routes or future typed config routes from `config-schema.md` |

Palette must not call Qdrant, TEI, Chrome, LLM providers, or SQLite directly
unless a future explicitly trusted local-admin diagnostic mode defines that
behavior.

## Required Palette Modules

The target Palette app must expose these boundaries or equivalents:

```text
apps/palette-tauri/
  src/api/             # generated REST/SSE client
  src/auth/            # token/session storage
  src/tokens/          # generated desktop presentation tokens
  src/features/sources/
  src/features/jobs/
  src/features/watches/
  src/features/ask/
  src/features/graph/
  src/features/memory/
  src/features/artifacts/
  src/features/providers/
  src/features/config/
  src/features/prune/
  src-tauri/capabilities/
  src-tauri/commands/  # approved local shell/file commands only
```

Required service interfaces:

```ts
export interface AxonDesktopClient {
  getStatus(): Promise<StatusReport>
  getDoctor(): Promise<DoctorReport>
  submitSource(request: SourceRequest): Promise<SourceResult>
  listJobs(filter: JobListRequest): Promise<Page<JobSummary>>
  streamJob(jobId: string): AsyncIterable<SourceProgressEvent>
  ask(request: AskRequest): Promise<AskResult>
  query(request: QueryRequest): Promise<QueryResult>
  retrieve(request: RetrievalRequest): Promise<RetrievalResult>
  graphQuery(request: GraphQueryRequest): Promise<GraphQueryResult>
}

export interface PaletteLocalBridge {
  pickFile(): Promise<PickedPath>
  pickDirectory(): Promise<PickedPath>
  openArtifact(ref: ArtifactRef): Promise<void>
  showNotification(notification: PaletteNotification): Promise<void>
}
```

`PaletteLocalBridge` never performs source acquisition itself. It only gathers
user-approved local inputs and hands them to the Axon API.

## Desktop Integration

Palette may use Tauri capabilities for:

- file/folder pickers
- drag-and-drop upload staging
- opening artifact files
- clipboard copy
- notifications
- local server bootstrap/status
- log file opening
- safe path display

Rules:

- every filesystem action is user-initiated or explicitly configured
- selected local paths are sent to Axon only as `SourceRequest` values or upload
  content
- raw absolute paths are classified as internal unless the user explicitly chose
  to expose them
- shell execution is disabled by default and limited to approved setup/doctor
  commands
- Palette never executes arbitrary adapter/tool commands on its own

## Presentation and Tokens

Palette is the source that motivated the palette contract. It must consume the
shared presentation tokens rather than inventing a desktop-only color language.

Required token groups:

| Group | Required Tokens |
|---|---|
| base | background, surface, panel, border, divider |
| text | primary, secondary, muted, inverse |
| brand | accent, accent_strong, service_name, automation |
| state | success, warning, error, info, neutral, degraded, waiting |
| data | source, job, graph, memory, artifact, provider |
| interaction | focus, hover, selected, disabled |
| terminal | ANSI/truecolor mappings for embedded logs/CLI output |

Palette-specific rules:

- desktop density is compact by default
- tables and timelines optimize for scanning
- graph view uses semantic colors from graph/data token roles
- status colors match CLI/web/mobile meanings
- no one-off gradient/orb decoration in operational views
- token snapshots are generated and tested

## Configuration UX

Palette may expose config editing, but the config schema remains authoritative.

Rules:

- `.env` editor shows URLs/secrets/runtime values only
- `config.toml` editor shows tuning/behavior values only
- secrets are redacted by default
- validation errors show file path, key path, expected type, and replacement
  guidance for removed keys
- config writes require explicit user confirmation
- restarting/reloading Axon after config changes is explicit

## Observability UX

Palette must render:

- job phase/status/counts
- heartbeats and stale job detection
- provider cooling/backpressure
- degraded states
- cleanup debt
- warnings with visibility/severity
- trace/job/source ids for copy/debug
- doctor remediation actions

It must use `SourceProgressEvent`, `JobSummary`, `ProviderCapability`, and
`DoctorReport` without inventing parallel status vocabularies.

## Security Contract

Palette must:

- store tokens in OS secure storage where possible
- redact secrets in logs, config editors, screenshots, and copy actions
- enforce server auth scopes in UI affordances
- require confirmation for destructive prune/reset/delete actions
- mark local-only/internal data clearly

Palette must not:

- ship with hardcoded tokens or local provider URLs
- expose hidden admin operations without server auth
- read arbitrary local files outside user-selected flows
- bypass redaction because it is a local desktop app

## Testing Contract

Required tests:

- generated DTO client matches API schemas
- token snapshot tests for desktop palette
- config editor validates `.env` vs `config.toml` placement
- job event fixture renders all phases/statuses
- provider degraded/cooling fixture renders correctly
- source submit through REST produces job descriptor
- upload/drop flow uses upload routes
- destructive operations require confirmation
- screenshots or component tests cover compact desktop layouts

## Acceptance Criteria

- Palette can operate Axon through REST/SSE only
- Palette displays source/job/watch/provider/memory/graph/artifact state with
  shared status and visibility semantics
- desktop token set is generated from the presentation contract
- config editing respects `.env` versus `config.toml` boundaries
- local filesystem and shell capabilities are explicit, scoped, and audited
