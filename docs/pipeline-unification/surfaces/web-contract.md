# Web App Contract
Last Modified: 2026-06-30

## Contract

The web app is the browser-based Axon control surface served by `axon-web`.

It owns browser UI, routing, panel session UX, setup flows, configuration
editing, dashboards, and interactive inspection. It consumes the same
transport-neutral DTOs and REST/SSE routes as other clients. It must not
reimplement source acquisition, retrieval, graph, memory, vector, provider, or
job logic in frontend code.

## Ownership Boundary

| Area | Web App Owns | Shared Axon Owns |
|---|---|---|
| Browser UI | routes, layouts, forms, tables, graph views | DTOs, statuses, auth scopes |
| Panel session | login UX, local browser session state | panel auth/session validation |
| Setup | first-run forms and status display | setup/config validation, doctor checks |
| Config editor | safe editing UX and diff display | config schema, env schema, write/reload behavior |
| Source UX | source form, watch controls, progress views | resolver, source jobs, ledger, embeddings |
| Retrieval UX | ask/query/retrieve/search views | retrieval, synthesis, citations |
| Operations UX | doctor/status/prune/provider screens | backend operations and side effects |

The web app may render rich state, but all authoritative mutations go through
REST routes.

## Required Routes

Web app depends on the full REST contract, with emphasis on:

| Feature | Routes |
|---|---|
| bootstrap | `GET /api/panel/state`, `GET /v1/server`, `GET /v1/capabilities`, `GET /readyz` |
| auth/setup | `/api/panel/login`, setup and auth routes from `rest-contract.md` |
| config | `GET /api/panel/config`, `PUT /api/panel/config`, `GET /api/panel/env`, `PUT /api/panel/env` |
| source lifecycle | `/v1/sources*`, `/v1/watches*`, `/v1/jobs*` |
| retrieval | `/v1/search`, `/v1/query`, `/v1/retrieve`, `/v1/ask`, streaming variants |
| memory | `/v1/memories/*` |
| graph | `/v1/graph/*` |
| artifacts/uploads | `/v1/artifacts/*`, `/v1/uploads/*` |
| operations | `/v1/status`, `/v1/doctor`, `/v1/providers*`, `/v1/prune/*`, `/v1/collections*` |

Frontend request/response models must be generated from API/OpenAPI schemas or
validated against them.

## Required Web Modules

The target web app must expose these boundaries or equivalents:

```text
apps/web/
  src/api/             # generated REST/SSE client
  src/auth/            # auth/session hooks
  src/styles/          # generated presentation tokens
  src/features/dashboard/
  src/features/sources/
  src/features/jobs/
  src/features/watches/
  src/features/ask/
  src/features/search/
  src/features/graph/
  src/features/memory/
  src/features/artifacts/
  src/features/providers/
  src/features/config/
  src/features/prune/
  src/features/setup/
```

Required client interfaces:

```ts
export interface AxonWebClient {
  getServer(): Promise<ServerInfo>
  getCapabilities(): Promise<CapabilityDocument>
  getStatus(): Promise<StatusReport>
  getDoctor(): Promise<DoctorReport>
  submitSource(request: SourceRequest): Promise<SourceResult>
  listSources(request: SourceListRequest): Promise<Page<SourceSummary>>
  getSource(sourceId: string): Promise<SourceSummary>
  listJobs(request: JobListRequest): Promise<Page<JobSummary>>
  streamJob(jobId: string): AsyncIterable<SourceProgressEvent>
  ask(request: AskRequest): Promise<AskResult>
  streamAsk(request: AskRequest): AsyncIterable<StreamEvent>
  query(request: QueryRequest): Promise<QueryResult>
  retrieve(request: RetrievalRequest): Promise<RetrievalResult>
}
```

Feature modules consume `AxonWebClient`; they do not import server internals or
store/provider crates.

## UI Areas

Required end-state views:

| View | Required Capabilities |
|---|---|
| Dashboard | providers, jobs, watches, degraded state, cleanup debt |
| Sources | submit source, list sources, source detail, generations, documents, items |
| Jobs | progress timeline, events, cancel, retry, artifacts |
| Watches | create/list/status/pause/resume/delete/exec/history |
| Ask | ask stream, citations, retrieval trace, artifacts |
| Query/Retrieve/Search | command boundary clarity and result inspection |
| Graph | node/edge/evidence exploration and source subgraph |
| Memory | search/context/review/pin/archive/forget/compact |
| Artifacts | metadata, preview/download, redaction warnings |
| Config | `.env` and `config.toml` editors with validation |
| Doctor | dependency checks, remediation, provider cooling |
| Prune | plan preview, confirmation, execution progress |

## Presentation Contract

The web app uses the shared presentation/design-token contract.

Rules:

- use Aurora-derived tokens and components
- status colors match CLI, Palette, Android, and extension semantics
- operational pages are dense, scannable, and work-focused
- do not use marketing/landing-page layout for the app shell
- forms expose advanced options progressively
- warnings/errors/degraded states are visible where the user can act
- graph/data views use semantic token roles, not arbitrary colors
- layout must support desktop and tablet widths; mobile web can be secondary to
  the Android app but must remain usable for core status/ask flows

## Streaming and Progress

Web uses SSE for:

- job progress
- ask/research/summarize streams
- provider/status updates when implemented

Rules:

- render `SourceProgressEvent` fields directly
- reconnect with last event id when supported
- never treat stream-only state as authoritative until final DTO/job status
  confirms it
- expose raw event detail in debug/developer views
- show waiting/cooling/backpressure states without looking like failure

## Config and Setup Contract

Web config editing must follow the config contracts:

- `.env` contains URLs, secrets, runtime/bootstrap values, and compose-only
  interpolation
- `config.toml` contains tuning and behavior
- secrets are redacted by default
- removed keys show target replacements when known
- writes require confirmation
- restart/reload requirements are shown before save
- malformed config fails with file path and key path

First-run setup may help create minimal config, but must not hide the two-file
boundary.

## Security Contract

The web app must:

- enforce server-provided auth scopes in UI actions
- require confirmation for destructive operations
- redact secrets and sensitive fields in UI, logs, and screenshots
- avoid storing bearer tokens in insecure browser storage when safer auth is
  available
- follow content security policy and same-origin rules
- use artifact/content routes for large outputs

The web app must not:

- embed provider secrets in frontend bundles
- call provider services directly
- bypass API validation by constructing internal store writes
- expose admin routes to unauthorized callers

## Testing Contract

Required tests:

- generated client types match OpenAPI/API schemas
- source submission form emits expected `SourceRequest`
- command boundary views distinguish search/query/retrieve/ask
- job progress renders every phase/status fixture
- ask stream renders tokens, citations, final answer, warnings, and errors
- config editor rejects secrets in TOML and tuning-only env keys
- destructive prune/reset/delete actions require confirmation
- graph/memory/artifact views handle redacted and missing data
- accessibility checks for keyboard navigation, focus, contrast, and reduced
  motion

## Acceptance Criteria

- web app can operate all core Axon workflows through REST/SSE
- frontend uses generated/shared DTOs
- app shell and operational views follow presentation token semantics
- source/retrieval/job/memory/graph/provider status vocabularies match the
  shared contracts
- web app does not own source, vector, graph, memory, LLM, or provider logic
