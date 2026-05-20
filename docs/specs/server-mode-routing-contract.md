# Server Mode Routing Contract

Status: draft
Last updated: 2026-05-19

This contract defines externally visible routing behavior for Axon CLI, stdio MCP, REST API, and the server runtime.

## Modes

- **Server mode**: `AXON_SERVER_URL` or `--server-url` is set and the target server is reachable.
- **Local mode**: `--local` is set, no server URL is configured, or fallback-local is selected after a server connection failure.
- **Server-required mode**: future strict mode where server connection failure is fatal and fallback-local is disabled.
- **Host-reachable service**: a Qdrant, embedding, Chrome, or LLM endpoint reachable from the host process. Container DNS names are not host-reachable unless the host resolver explicitly supports them.

## Routing Rules

- CLI and stdio MCP MUST prefer server mode when `AXON_SERVER_URL` is set and the command is remote-capable.
- CLI and stdio MCP MUST NOT start local workers for a command that successfully routes to the server.
- `--wait` in server mode MUST poll server job status and MUST NOT process the job locally.
- CLI and stdio MCP MAY fall back to local mode only when:
  - the server is unavailable,
  - the command is safe to run locally,
  - and server-required mode is not enabled.
- Local fallback MUST resolve effective service endpoints for the host process. It MUST NOT blindly use container DNS URLs such as `http://axon-qdrant:6333` from host-local execution.
- When fallback-local occurs, human and JSON output MUST identify that the command ran locally because the configured server was unavailable.
- Fallback-local output MUST NOT imply failure when local capabilities completed the operation successfully.
- Fallback-local output MUST classify the outcome as one of:
  - `completed_equivalent`: local capability tier matched the requested operation.
  - `completed_degraded`: local capability tier completed part of the request, with unavailable features listed.
  - `failed_local`: neither server nor local runtime could complete the operation.
- `--local` MUST force local execution and bypass `AXON_SERVER_URL`.

## Server Availability

Fallback-local MAY occur only for transport-level unavailability:

- DNS failure.
- TCP connect failure.
- connection refused.
- connect timeout.
- read timeout before a valid server response.
- HTTP 502/503/504 from a gateway when the server cannot be reached.

Fallback-local MUST NOT be silent for:

- 401/403 auth failure.
- schema/version mismatch.
- 400 invalid request.
- 404 route not found.
- server 5xx after the request was accepted or may have produced side effects.

Those cases MUST fail with a targeted remedy unless the user explicitly requests local execution.

## Timeouts

Thin clients MUST use explicit, configurable timeouts:

- connect timeout for route probe.
- request timeout for sync REST calls.
- poll interval and wait timeout for async jobs.
- fallback decision timeout independent of long operation timeout.

Timeout values MUST be visible in `doctor --json`.

## No-Silent-Fallback Commands

These commands MUST NOT silently fallback from server mode to a different local state store:

- Server job lifecycle operations: `crawl status`, `crawl errors`, `crawl cancel`, `crawl list`, `crawl cleanup`, `crawl clear`, `crawl recover`, and equivalent `extract`, `embed`, and `ingest` lifecycle operations.
- Worker operator commands: `crawl worker`, `extract worker`, `embed worker`, `ingest worker`.
- Admin or mutating vector operations: `dedupe`, `migrate`.
- Scheduler mutations: `watch create`, `watch update`, `watch pause`, `watch resume`, `watch delete`, `watch run-now`.
- Config mutation when the target is ambiguous between local and server config.

These commands may offer an explicit `--local` rerun suggestion, but they must not switch queues, schedulers, vector stores, or config targets without telling the user.

## Surface Parity

- The service layer owns command semantics and request options.
- CLI, REST, and MCP MUST map into shared service request/option types.
- REST routes are the feature-parity target for CLI server mode; `/v1/actions` is no longer a supported compatibility surface.
- A REST route MUST NOT silently ignore a knob exposed by CLI or MCP for the same service operation.
- Contract tests MUST compare effective service inputs or job configs across CLI planning, REST request parsing, and MCP request parsing.
- New service options MUST be added to the canonical service request type first, then exposed through each supported surface.

## Removed `/v1/actions`

- `/v1/actions` was an interim RPC endpoint that accepted the MCP-shaped `AxonRequest` action envelope.
- This project uses a hard cutover. New business logic belongs in the service layer and direct REST adapters.
- Clients must use direct `/v1/*` REST routes after the cutover.

## Direct REST

- Direct REST routes are the canonical future client/server API.
- REST request bodies MUST expose the same user-facing knobs as CLI/MCP for the same operation unless a documented transport constraint prevents it.
- REST lifecycle routes MUST support submit, status, cancel, list, cleanup, clear, and recover for async job families where those operations exist locally.

## Server Version and Schema

Thin clients MUST verify server compatibility before relying on server mode:

- server version.
- REST contract/schema version.
- supported routes.
- minimum client schema version.

Reachable but incompatible servers MUST NOT trigger silent fallback. The client must report the incompatibility and recommend a rebuild, restart, or upgrade.

## Auth and Scope Parity

REST, MCP HTTP, and any interim RPC route MUST share the same auth decisions:

- Read scope: status, retrieve, query, sources, domains, stats, artifact reads, and non-mutating discovery.
- Write scope: crawl/scrape/extract/embed/ingest/session submissions, job cancellation, sync, and artifact-producing operations.
- Admin scope: dedupe, migrate, destructive queue cleanup/clear, scheduler mutation, config mutation, and server management.

No surface may downgrade a write/admin operation to read-only because it is exposed through a different transport.

## Artifact Identity

JSON responses MUST prefer stable artifact handles over raw filesystem paths.

Artifact handles SHOULD include:

- `artifact_id`: stable identifier for API retrieval.
- `kind`: markdown, crawl-manifest, extract-summary, extract-items, screenshot, log, or similar.
- `source_url`: original URL when applicable.
- `job_id`: producing job when applicable.
- `content_hash`: hash for dedupe/reconciliation.
- `relative_path`: Axon-owned path relative to the artifact root.
- `bytes` and optional `line_count`.
- `debug_path`: optional raw local/container path for diagnostics only.

Raw paths MUST NOT be the only way a client can retrieve output.

## Capability Degradation

- Crawl, scrape, map, deterministic extract, and artifact-backed retrieve SHOULD work without Qdrant or an embedding provider.
- Semantic query, vector-backed sources/domains/stats, and RAG ask require vector capability.
- LLM fallback extract, research synthesis, suggest, debug synthesis, and evaluate judge require LLM capability.
- `doctor` MUST report unavailable capabilities with impact and remedies.
- `doctor` MUST probe host-reachable service candidates when local runtime is possible, including localhost defaults and configured LAN/Tailscale candidates.
- `doctor diagnose` MAY use the configured LLM to diagnose doctor JSON. If LLM capability is missing, it MUST return normal doctor findings plus a remedy instead of failing opaquely.

## Stable JSON Envelope

Machine-readable command output SHOULD include route metadata:

- `route`: `server`, `local`, or `fallback_local`.
- `fallback`: boolean.
- `fallback_outcome`: `none`, `completed_equivalent`, `completed_degraded`, or `failed_local`.
- `capability_tier`: highest tier used for the operation.
- `server_url`: configured server URL when present.
- `local_data_dir`: local Axon data dir when local state is touched.
- `effective_endpoints`: resolved Qdrant, embedding, Chrome, and LLM endpoints used by the operation.
- `artifacts`: stable artifact handles.
- `warnings`: non-fatal routing/capability notes.

Human output MUST use concise route notes:

- no route note for ordinary local mode with no configured server.
- one route note for successful fallback-local.
- warning language only for degraded or failed fallback.
- no repeated route notes during job polling.

## Local Artifact Reconciliation

- Local fallback output MUST be recorded in Axon-owned artifact/manifest state.
- Existing crawl `manifest.jsonl` and markdown output layout SHOULD be reused before introducing a new manifest format.
- Locally produced artifacts SHOULD be eligible for later server reconciliation and embedding.
- Reconciliation MUST preserve content hashes and source URLs so duplicate embedding can be avoided.
- Reconciliation SHOULD run automatically for Axon-owned local artifacts when the server becomes reachable.
- `axon sync pending` MUST provide explicit operator-controlled reconciliation.

Conflict handling MUST be content-hash based:

- same source URL and same content hash: mark synced without duplicate embedding.
- same source URL and different content hash: preserve both revisions unless a newer timestamp policy says otherwise.
- different source URL and same content hash: allow multiple source references for one blob.
- never delete local artifacts after sync unless an explicit cleanup command requests it.

## Endpoint Discovery

Normal command execution SHOULD use explicit/configured/localhost/trusted-cached endpoints only. It MUST NOT scan LAN or Tailscale networks on every command run.

Doctor MAY probe LAN/Tailscale candidates. Cached candidates MUST record endpoint kind, URL, probe time, and evidence that the service is the expected Axon/Qdrant/embedding/Chrome service.

## MCP Stdio Fallback

Stdio MCP MUST follow CLI routing rules:

- prefer server when `AXON_SERVER_URL` is set.
- fallback local only for safe commands.
- include route metadata in tool responses.
- never silently switch job lifecycle, scheduler, vector admin, or config mutation state stores.

MCP fallback-local responses SHOULD include a compact `route_note` even when the operation completed equivalently.

## Security Boundary

Local fallback and reconciliation MUST preserve file safety:

- host-local file reads require allowed roots.
- dotfiles and symlinks remain rejected unless explicitly allowed.
- sync uploads only Axon-owned artifacts by default.
- arbitrary host paths require explicit confirmation or allowed-root config.
- secret-like content must be redacted from route metadata, doctor output, and LLM-powered doctor diagnosis.

## Observability

Route decisions MUST be logged with structured fields:

- command.
- requested route.
- selected route.
- fallback reason.
- fallback outcome.
- capability tier.
- effective endpoints.
- server URL.
- local data dir.
- artifact handles.
- reconciliation result.

## Cutover Exit Criteria

`/v1/actions` deletion is complete when:

- Direct REST routes cover all current server-routed CLI behavior.
- Stdio MCP can thin-client through REST when `AXON_SERVER_URL` is set.
- CLI server mode no longer posts to `/v1/actions`.
- Canonical service request tests cover CLI, REST, and MCP request mapping.
- Auth/scope parity tests cover REST and MCP HTTP.
- Doctor route explanation is implemented.
- Stable JSON route envelope is implemented for routed commands.
- Local fallback and no-silent-fallback policies are tested.

## Dev Wrapper

- `scripts/axon` MUST build the host debug binary before invoking Axon.
- For Rust-only edits, the dev wrapper SHOULD update the mounted dev binary and restart the dev container instead of rebuilding the image.
- Full image rebuilds SHOULD be reserved for image-level inputs such as Dockerfile, compose files, runtime dependency changes, and web assets.
