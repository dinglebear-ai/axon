# Android Contract
Last Modified: 2026-06-30

## Contract

`apps/android/` is a first-class Axon client surface. It is not an optional or
experimental afterthought.

Android consumes Axon through REST/SSE APIs, authenticated user/session state,
artifact endpoints, and generated DTOs. It must not implement its own source
pipeline, vector retrieval, graph store, memory store, source resolver, or LLM
orchestration.

## Ownership Boundary

| Area | Android Owns | Shared Axon Owns |
|---|---|---|
| UI | Compose screens, navigation, local presentation state | Canonical DTOs, status semantics |
| Auth | token storage, sign-in UX, refresh handling | OAuth/bearer validation, scopes, caller context |
| Sessions | mobile session draft/cache/sync UX | `/v1/mobile/sessions/*` persistence and schemas |
| Streaming | SSE client, reconnect UX, partial rendering | event schema and streaming routes |
| Sources | source submission form and status display | source resolution, jobs, ledger, vector writes |
| Ask/query | query inputs and result rendering | retrieval, synthesis, citations, graph context |
| Artifacts | download/view/share UX | artifact metadata, bytes, retention, auth |
| Offline | local cache of safe display data | authoritative state, conflict resolution |

Android may cache server responses for UX, but server state remains
authoritative.

## Required API Surface

This is the target Android API surface. Current Android still uses generated
and hand-written wrappers around the current direct REST routes, including
`/v1/mobile/sessions/{id}` DTOs with fields such as `id`,
`first_message_preview`, `turn_count`, `injected_op_count`, and `items`. The
clean-break target below moves Android to the canonical source/job/event DTOs
and the normalized mobile-session model.

Android depends on these REST routes:

| Feature | Routes |
|---|---|
| bootstrap | `GET /v1/server`, `GET /v1/capabilities`, `GET /readyz` |
| auth | OAuth/bearer endpoints from `auth-contract.md` and `rest-contract.md` |
| source submission | `POST /v1/sources`, `GET /v1/jobs/{job_id}`, `GET /v1/jobs/{job_id}/events`, `GET /v1/jobs/{job_id}/stream` |
| retrieval | `POST /v1/query`, `POST /v1/retrieve`, `POST /v1/ask`, `POST /v1/ask/stream` |
| memory | `POST /v1/memories`, `POST /v1/memories/search`, `POST /v1/memories/context`, `GET /v1/memories/{memory_id}` |
| artifacts | `GET /v1/artifacts/{artifact_id}`, `GET /v1/artifacts/{artifact_id}/content` |
| mobile sessions | `GET /v1/mobile/sessions`, `GET /v1/mobile/sessions/{session_id}`, `PUT /v1/mobile/sessions/{session_id}`, `DELETE /v1/mobile/sessions/{session_id}` |
| operations | `GET /v1/status`, `GET /v1/doctor` |

Android must use generated DTOs or schema-derived models for these routes. It
must not hand-roll divergent request/response shapes.

## Required Android Modules

The target Android app must expose these modules or equivalent package
boundaries:

```text
apps/android/
  core/api/          # generated DTOs, REST client, SSE client
  core/auth/         # token storage, auth interceptors
  core/design/       # generated Axon presentation tokens and components
  core/session/      # MobileSession repository/cache
  feature/sources/   # source submit/status/detail UI
  feature/jobs/      # job progress/event UI
  feature/ask/       # ask/query/retrieve/search UI
  feature/memory/    # memory search/context/review UI
  feature/artifacts/ # artifact metadata/content UI
  feature/settings/  # server/auth/config status
```

Required client interfaces:

```kotlin
interface AxonApiClient {
    suspend fun server(): ServerInfo
    suspend fun capabilities(): CapabilityDocument
    suspend fun submitSource(request: SourceRequest): SourceResult
    suspend fun getJob(jobId: String): JobSummary
    fun streamJobEvents(jobId: String): Flow<SourceProgressEvent>
    suspend fun ask(request: AskRequest): AskResult
    fun streamAsk(request: AskRequest): Flow<StreamEvent>
    suspend fun query(request: QueryRequest): QueryResult
    suspend fun retrieve(request: RetrievalRequest): RetrievalResult
}

interface MobileSessionRepository {
    suspend fun list(): List<MobileSession>
    suspend fun get(sessionId: String): MobileSession
    suspend fun upsert(session: MobileSession): MobileSession
    suspend fun delete(sessionId: String)
}
```

These interfaces are implemented over REST/SSE only.

## Mobile Session Model

`MobileSession` stores mobile chat and workflow state that should survive app
restarts and sync across devices.

Required fields:

| Field | Meaning |
|---|---|
| `session_id` | Stable mobile session id. |
| `title` | User-visible title. |
| `status` | `active`, `archived`, `deleted`, or `sync_conflict`. |
| `created_at` | Server creation time. |
| `updated_at` | Server update time. |
| `last_opened_at` | Client/server last-opened marker. |
| `source_refs` | Sources/jobs/artifacts linked to the session. |
| `messages` | Redacted message summaries or server-approved content. |
| `draft` | Optional encrypted or redacted draft payload. |
| `sync_version` | Optimistic concurrency token. |

Rules:

- Android uses optimistic concurrency with `sync_version`.
- Server rejects stale updates with a structured conflict.
- Android may keep a local draft cache, but server-persisted session state is
  the sync source of truth.
- Raw secrets, auth headers, and private file contents are never stored in
  mobile session rows.

## Offline and Cache Contract

Android may cache:

- server info and capability documents
- recent sessions
- recent query/ask summaries
- artifact metadata
- downloaded artifact bytes explicitly saved by the user
- safe thumbnails/previews

Android must not cache:

- bearer tokens outside platform secure storage
- unredacted secrets
- raw local files selected for upload after upload completion unless user saved
  them separately
- hidden redacted artifact content
- provider credentials or config secrets

Offline behavior:

- queued user actions must show local pending state
- source jobs are submitted only when online
- ask/query require online server unless a future explicit offline retrieval
  provider is added
- stale cached results must be visibly marked stale

## Streaming Contract

Android streaming uses REST SSE.

Required stream event handling:

| Event Shape | Android Behavior |
|---|---|
| `SourceProgressEvent` | update job/status UI with phase, status, counts, message |
| token/final synthesis event | update ask/chat answer view |
| citation event | attach citation metadata to answer |
| artifact event | show artifact availability |
| warning event | show non-blocking warning |
| error event | stop stream and show structured error |

Rules:

- reconnect with last event id when supported
- never assume stream-only state is durable until final DTO or job/event route
  confirms it
- render unknown event types as internal warnings in debug builds and ignore in
  release builds unless severity is error

## Design and Presentation

Android uses the presentation contract for:

- Aurora-derived color tokens
- typography scale
- semantic status colors
- icon semantics
- density and spacing
- accessibility contrast
- loading, degraded, failed, and offline states

Android may adapt layouts for mobile ergonomics, but token meaning must match
web, CLI, and Palette.

## Security Contract

Android must:

- use platform secure storage for tokens
- respect server-provided visibility/redaction fields
- avoid logging raw request/response bodies containing user content
- avoid exposing local file paths unless user-visible and safe
- require explicit user action before uploading files
- show provider/auth errors without revealing secrets

Android must not:

- bypass server auth by calling Qdrant/TEI/LLM providers directly
- perform source acquisition outside the server pipeline
- run arbitrary CLI/MCP tools locally
- persist unredacted tool outputs without server artifact metadata

## Testing Contract

Required Android tests:

- DTO serialization fixtures match `schemas/api-dto-schema.md`
- auth token refresh and unauthorized flows
- source job submission and progress rendering with fixture events
- ask stream rendering with citations and final answer
- session sync happy path and conflict path
- artifact metadata and content download
- offline cache stale-state rendering
- redacted fields do not display secret values

Required smoke:

```text
start test Axon server
load Android app
sign in or configure bearer token
submit source
observe job progress
ask a question
open citation/artifact
sync mobile session
```

## Acceptance Criteria

- Android can perform source submission, job progress, ask/query, artifacts,
  memory search/context, and mobile session sync through REST/SSE only
- Android uses generated/shared DTOs
- offline caches are visibly stale and never authoritative
- streaming events match `SourceProgressEvent` and route-specific event schemas
- server remains the only owner of source pipeline, retrieval, graph, memory,
  and provider orchestration
