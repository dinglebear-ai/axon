# Server Mode and Capability Tiers

Status: draft
Last updated: 2026-05-19

## Goals

- Make Docker/server mode the recommended path: the long-running Axon server owns crawling, extraction, embedding, retrieval, jobs, artifacts, and service health.
- Keep CLI and stdio MCP useful without a server: they can run a local runtime for degraded/offline workflows.
- Make routing transparent: every command should be able to explain whether it ran against the remote server or locally, and why.
- Make capability degradation explicit: crawl and retrieve remain useful without embeddings, Qdrant, or an LLM.
- Keep all command surfaces aligned: CLI, REST, and MCP should expose the same service knobs unless a transport cannot support them.

## Terminology

- **Server runtime**: `axon serve`; owns HTTP REST API, MCP HTTP, web panel, SQLite jobs, workers, artifacts, output files, crawler/extractor, embedding provider, and Qdrant integration.
- **Local runtime**: CLI or stdio MCP constructs a local `ServiceContext` and executes service-layer operations in the current process. Artifacts and jobs are written to the local Axon data directory. Local runtime does not mean a separate Docker-only Qdrant/TEI stack; it means Axon should use host-reachable service URLs.
- **Thin client**: CLI or stdio MCP forwards the command to `AXON_SERVER_URL`; it does not create local workers or require local TEI/Qdrant for that routed operation.
- **Fallback-local**: CLI or stdio MCP first attempts the configured server, then falls back to local runtime when the server is unavailable and the command is safe to run locally.

## Target Routing Model

Default behavior:

- If `AXON_SERVER_URL` is set, CLI and stdio MCP first route remote-capable commands to the server.
- If the server is unavailable, CLI and stdio MCP fall back to local runtime for commands that can safely run without server state.
- If `AXON_SERVER_URL` is set and the command routes successfully to the server, `--wait` polls the server job and must not start local workers.
- If `AXON_SERVER_URL` is set and fallback-local is used, output must present this as a routing note, not a failure, when the local capability tier can complete the operation.
- Fallback-local output should distinguish "completed locally with equivalent capabilities" from "completed locally with degraded capabilities" and "could not complete locally".
- Local fallback should resolve service endpoints for the host context. If configured URLs use container DNS names such as `http://axon-qdrant:6333` or `http://axon-tei:80`, local fallback should not blindly reuse them from the host process. It should try configured host URLs first, then known localhost ports, then reachable LAN/Tailscale candidates discovered by doctor.
- `--local` forces local runtime.
- A future `--server-required` or equivalent should fail instead of falling back.
- `doctor` reports the active mode, route decision, server health, local runtime health, and capability tier.

Recommended install shape:

```text
axon-server
  Long-running server process: REST API, MCP HTTP, web panel, jobs, workers.

axon
  CLI. Thin client when AXON_SERVER_URL is set; local runtime fallback when safe.

axon-mcp
  Stdio MCP. Thin client when AXON_SERVER_URL is set; local runtime fallback when safe.
```

This can be implemented as one binary with subcommands or separate binary aliases. The important boundary is behavioral: the server is canonical when reachable, and local mode is explicit or fallback.

## Service Layer Contract

The service layer is the product API. CLI, REST, and MCP should be thin adapters over the same service request/response types.

Current mismatch to fix:

- `/v1/actions` accepts an MCP-shaped `AxonRequest` action envelope and has more knobs for some operations.
- Direct REST routes are cleaner but thinner in places. For example, `POST /v1/extract` currently accepts `{ urls, prompt }`, while the MCP/action path carries options like `render_mode`, `embed`, and `max_pages`.

Target:

- Direct REST routes become canonical for client/server mode.
- REST request bodies gain full feature parity with the current `/v1/actions` action envelope before CLI server mode is migrated.
- Shared service request/option structs should define the knobs once. CLI, REST, and MCP should map into those structs rather than each surface inventing its own subset.
- This is a hard cutover project. There is no backwards-compatibility requirement for `/v1/actions`; after REST parity exists and CLI/MCP have moved, remove `/v1/actions` instead of keeping a compatibility phase.
- Parity tests should compare CLI planning, REST request parsing, and MCP request parsing against the same canonical service request structs.

## REST Route Parity Requirements

REST routes should expose the same knobs as CLI/MCP where applicable:

- `POST /v1/crawl`
  - `urls`, `max_pages`, `max_depth`, `render_mode`, `format`, `include_subdomains`, `respect_robots`, `discover_sitemaps`, `max_sitemaps`, `sitemap_since_days`, `delay_ms`, headers, output/artifact preferences.
- `POST /v1/scrape`
  - `url`, `render_mode`, `format`, `embed`, selectors, headers, response mode.
- `POST /v1/extract`
  - `urls`, `prompt`, `extract_mode`, `max_pages`, `render_mode`, `embed`, headers, response/artifact preferences.
- `POST /v1/embed`
  - URL/text input, source type, collection, chunking/embedding options where supported.
- `POST /v1/ingest`
  - source type, target, GitHub source inclusion, source-specific options.
- `POST /v1/query`
  - query, collection, limit, offset, search mode options.
- `POST /v1/retrieve`
  - URL/source/artifact identifier, max points, cursor, token budget, raw markdown/artifact read mode.
- `POST /v1/ask`
  - question, collection, limit, streaming/follow-up mode, diagnostics/explain flags.

Lifecycle routes should remain first-class:

```text
POST   /v1/{crawl,extract,embed,ingest}
GET    /v1/{crawl,extract,embed,ingest}/{id}
POST   /v1/{crawl,extract,embed,ingest}/{id}/cancel
GET    /v1/{crawl,extract,embed,ingest}
POST   /v1/{crawl,extract,embed,ingest}/cleanup
DELETE /v1/{crawl,extract,embed,ingest}
POST   /v1/{crawl,extract,embed,ingest}/recover
```

## Command Route Matrix

| Command | Target server route | Local fallback | Minimum local tier | Silent fallback? | Notes |
|---------|---------------------|----------------|--------------------|------------------|-------|
| `scrape` | REST | Yes | Tier 0 | Yes, with route note | Equivalent when network and output dir are available; embed portion requires Tier 2. |
| `crawl` start | REST async job | Yes | Tier 0 | Yes, with route note | `--wait` polls server when routed; local workers only when fallback-local or `--local`. |
| `crawl status/errors/cancel/list/cleanup/clear/recover` | REST lifecycle | No for mutating/status server jobs | Server | No | These refer to a specific server job queue when server mode is selected. |
| `crawl worker` | None | Local only | Tier 0 | No | Operator command; should say server mode uses `axon serve`. |
| `crawl audit/diff` | None | Local only | Tier 0 | No | Local filesystem/audit tooling. |
| `map` | REST | Yes | Tier 0 | Yes, with route note | Pure discovery; no vector requirement. |
| `extract` start | REST async job | Yes | Tier 0 deterministic, Tier 3 LLM | Yes, with route note | `auto` can complete deterministically without LLM; `llm`/LLM fallback requires Tier 3. |
| `extract status/errors/cancel/list/cleanup/clear/recover` | REST lifecycle | No for server jobs | Server | No | Same job locality rule as crawl lifecycle. |
| `extract worker` | None | Local only | Tier 0 | No | Operator command. |
| `search` | REST | Yes | Tier 1 plus Tavily | Yes, with route note | External search can enqueue crawl; semantic ranking requires higher tiers only if requested. |
| `research` | REST | Degraded only | Tier 3 | No silent degradation | Requires LLM synthesis; without LLM, fail with remedy or return search-only if explicitly requested. |
| `embed` start | REST async job | Yes | Tier 2 | Yes, with route note | Host-local paths need explicit allowed roots; URLs/text are safer fallback inputs. |
| `embed status/cancel/list/cleanup/clear/recover` | REST lifecycle | No for server jobs | Server | No | Server job operations must not target local queue by accident. |
| `embed worker` | None | Local only | Tier 2 | No | Operator command. |
| `debug` | REST | Degraded only | Tier 3 | No silent degradation | Diagnostics may run locally; LLM diagnosis requires Tier 3. |
| `doctor` | REST health plus local probes | Yes | Tier 0 | Yes | Always useful; reports server and host-reachable service probes. |
| `doctor diagnose` | REST/local doctor plus LLM | Degraded only | Tier 3 | No silent degradation | Inject doctor JSON into configured LLM for diagnosis. |
| `query` | REST | Yes | Tier 2 | Yes, with route note | Requires embedding provider plus Qdrant/vector store. |
| `retrieve` | REST | Yes | Tier 1 | Yes, with route note | Should support artifact/manifest retrieval without Qdrant. |
| `ask` | REST | Degraded only | Tier 3 | No silent degradation | Could offer retrieve-only context if vector/LLM missing, but do not silently call it a RAG answer. |
| `evaluate` | REST | Degraded only | Tier 3 | No silent degradation | Requires retrieval and LLM judge unless an explicit non-LLM mode exists. |
| `train` | Local only | Local only | Depends | No | Keep local until product semantics are clarified. |
| `suggest` | REST | Degraded only | Tier 3 | No silent degradation | LLM-backed suggestion. |
| `sources` | REST | Yes | Tier 1 artifact, Tier 2 vector | Yes, with route note | Should report whether data comes from manifest/artifacts or vector store. |
| `domains` | REST | Yes | Tier 1 artifact, Tier 2 vector | Yes, with route note | Same as sources. |
| `stats` | REST | Yes | Tier 1 artifact, Tier 2 vector | Yes, with route note | Must label artifact stats vs vector stats. |
| `status` | REST | Yes for local status only | Tier 0 | No silent queue switch | Server mode status should show server queue; if fallback-local, label it as local status. |
| `dedupe` | REST admin | No | Server/Tier 2 | No | Mutating vector/admin operation; never silently fallback. |
| `ingest` start | REST async job | Partial | Tier 1-3 by source | No silent degradation | GitHub/Reddit/YouTube have source-specific dependencies; fallback must label missing capabilities. |
| `ingest status/cancel/list/cleanup/clear/recover` | REST lifecycle | No for server jobs | Server | No | Server job locality rule. |
| `ingest worker` | None | Local only | Source-dependent | No | Operator command. |
| `sessions` | REST async job | Yes | Tier 1, Tier 2 for embedding | Yes, with route note | Local session artifacts can be reconciled later. |
| `screenshot` | REST | Yes | Tier 0 plus Chrome | Yes, with route note | Requires reachable Chrome/headless browser. |
| `completions` | None | Local only | None | No | Shell integration, no server needed. |
| `mcp` | MCP HTTP or local stdio | Yes | Capability-dependent | Yes, with route note | Stdio MCP should thin-client to server when configured, fallback local when safe. |
| `serve` | None | Local/server runtime start | Server runtime | No | Starts the server; does not route to another server by default. |
| `setup` | Local plus optional remote deploy | Local only | None | No | Host/deploy bootstrap. |
| `migrate` | REST admin | No | Server/Tier 2 | No | Mutating vector/admin operation; never silently fallback. |
| `config` | Local config plus optional server config API | Local by default | None | No | Avoid silently editing server config when user intended local, or vice versa. |
| `watch` | REST | Partial | Tier 0+ | No silent mutation | Creating/updating/deleting schedules should not silently fallback to a different scheduler. |

## Extract Modes

Current behavior is `auto`: deterministic extraction wins, and LLM fallback only runs when deterministic extraction yields no data.

Add an explicit extract mode:

```text
auto
  Deterministic first, LLM fallback if deterministic finds nothing.

deterministic
  Deterministic only. Never call LLM.

llm
  Skip deterministic extraction and run the prompt path.

both
  Run deterministic and LLM, preserve provenance for both, then merge.
```

Default should remain `auto`, not `both`.

Reasoning:

- `both` is more expensive and can duplicate or conflict with high-confidence deterministic output.
- Deterministic verticals are useful because they are cheap, stable, and schema-aware.
- Users who pass a prompt and want LLM behavior need an explicit `--extract-mode llm`.
- Users who want comparison or enrichment can opt into `--extract-mode both`.

CLI output should say:

- Which mode was used.
- Which parser produced each result.
- Whether the prompt was used, skipped, or run as fallback.
- How to force prompt extraction when deterministic output won.

Example human note:

```text
Parser provenance
  1 page handled by open-graph; LLM fallback was not used.
  Prompt was skipped because deterministic extraction produced data.
  To force prompt extraction: axon extract <url> --query "..." --extract-mode llm
```

## Capability Tiers

## Host-Reachable Service Discovery

Local runtime should never assume container DNS names work from the host process.

Endpoint resolution order:

1. Explicit CLI flag.
2. Host-valid env/config URL.
3. Known localhost ports, for example Qdrant `http://127.0.0.1:53333`, TEI `http://127.0.0.1:52000`, Chrome `http://127.0.0.1:6000`.
4. Doctor-discovered LAN/Tailscale candidates when reachable and clearly associated with Axon services.
5. Fail with a remedy.

If a configured URL host is a Docker service DNS name such as `axon-qdrant`, `axon-tei`, or `axon-chrome`, local runtime should warn and try host-reachable candidates. Doctor should show both configured and effective endpoints.

Doctor should include:

- server health at `AXON_SERVER_URL`.
- host-reachable Qdrant probes.
- host-reachable TEI/embedding provider probes.
- host-reachable Chrome probes.
- whether each endpoint came from config, localhost probing, LAN probing, or Tailscale probing.
- remedies for mismatched container DNS in host-local mode.

### Tier 0: Crawl-Only

Requirements:

- Axon binary.
- Network access to target sites.
- Writable Axon data directory.
- No Qdrant required.
- No embedding provider required.
- No LLM required.

Available:

- `scrape`
- `crawl`
- `map`
- `screenshot` if Chrome/headless browser is available.
- `extract --extract-mode deterministic`
- `extract --extract-mode auto` only for deterministic output; LLM fallback is unavailable.
- `status` for local/server job state.
- `doctor`
- `config`
- `serve`
- `mcp` local stdio with only Tier 0 actions.

Unavailable or degraded:

- `embed`
- `query`
- `ask`
- `evaluate`
- LLM fallback extraction.
- semantic `sources`/`domains`/`stats` backed by Qdrant.

### Tier 1: Crawl + Retrieve

Requirements:

- Tier 0.
- Markdown/artifact storage index or manifest that can resolve URL/source/artifact identifiers.

Available:

- All Tier 0 commands.
- `retrieve` by URL, source id, or artifact id from local/server markdown/artifact storage.
- `sources` from artifact/source manifest, even without Qdrant.
- `domains` from artifact/source manifest, even without Qdrant.
- `search` as external web search if Tavily is configured, with crawl enqueue allowed.

Unavailable or degraded:

- semantic `query`.
- RAG `ask`.
- `evaluate` that depends on semantic retrieval or LLM judge.
- `stats` should report artifact/source stats but mark vector stats unavailable.

### Tier 2: Vector Search

Requirements:

- Tier 1.
- Qdrant or equivalent vector store.
- Embedding provider: current TEI, future fastembed, or another provider behind the same service abstraction.

Available:

- All Tier 1 commands.
- `embed`
- `query`
- vector-backed `sources`
- vector-backed `domains`
- vector-backed `stats`
- crawl/scrape/extract/ingest with `embed=true`
- background embedding of local backlog items.

Unavailable or degraded:

- `ask` answer synthesis if no LLM backend is configured.
- `evaluate` LLM judge if no LLM backend is configured.
- `suggest`, `research`, and `debug` synthesis if no LLM backend is configured.

### Tier 3: RAG + LLM

Requirements:

- Tier 2.
- LLM backend available.

Available:

- All Tier 2 commands.
- `ask`
- `evaluate`
- `research`
- `suggest`
- `debug`
- LLM fallback extraction.
- `extract --extract-mode llm`
- `extract --extract-mode both`

## Local Backlog and Server Reconciliation

Local fallback should not create a dead-end data island.

When a CLI or stdio MCP command falls back to local runtime and produces crawl/scrape/extract artifacts:

- Write artifacts to the local Axon data directory.
- Reuse the existing crawl output layout where possible:
  - crawl output directory
  - `manifest.jsonl`
  - `markdown/`
  - structured blob fields already consumed by the embedding path
- Extend the existing manifest model, or add a small companion sync index beside it, with:
  - URL/source id.
  - artifact paths.
  - content hash.
  - creation time.
  - requested collection.
  - embedding status: `pending`, `embedded`, `failed`.
  - origin: `local-fallback` or `local-explicit`.
- If embedding is locally available, embed immediately.
- If server mode later becomes reachable, offer or automatically run reconciliation:
  - upload/register local artifacts with server.
  - server embeds any pending content.
  - mark local entries as synced.
- Reconciliation should run in both forms:
  - automatic best-effort reconciliation when server mode becomes reachable and artifacts live under Axon-owned data dirs.
  - explicit `axon sync pending` for operator control and debugging.

Open policy decision:

- Default to automatic reconciliation for content under Axon-owned data dirs.
- Require confirmation for arbitrary host-local paths.
- Prefer reusing `src/crawl/manifest.rs`, `src/services/crawl.rs` predicted artifact paths, and the existing manifest readers in `src/vector/ops/tei/` before adding a new manifest format.

## Doctor UX

`doctor` should behave like a real diagnostic report: status, impact, and remedies.

Target shape:

```text
Doctor Report

Mode
  client: fallback-local
  server_url: http://127.0.0.1:8001
  route: local runtime
  note: server unavailable; completed locally with equivalent crawl capability
  fallback: enabled
  local_runtime: available

Capabilities
  + Tier 0 crawl-only
  + Tier 1 crawl + retrieve
  ! Tier 2 vector search unavailable
    cause: embedding provider unreachable
    impact: query, embed, vector-backed ask unavailable
    remedy: start TEI/fastembed or run `just services-up`
  ! Tier 3 RAG + LLM degraded
    cause: vector search unavailable
    remedy: fix Tier 2 first

Services
  + sqlite path=/home/jmagar/.axon/jobs.db
  + server http 200 http://127.0.0.1:8001
  ! tei connection refused http://127.0.0.1:52000
  + qdrant http 200 http://127.0.0.1:53333
  + chrome http 200 http://127.0.0.1:6000
  + gemini_headless command validation passed

Recommendations
  1. Start embeddings: docker compose --env-file ~/.axon/.env up -d axon-tei
  2. Reconcile local backlog: axon sync pending --server http://127.0.0.1:8001
```

Doctor should support:

- `axon doctor`
- `axon doctor --json`
- `axon doctor --command extract <url>` to explain routing for a specific command.
- `axon doctor --server-url ...` to test a specific server.
- `axon doctor diagnose` to run doctor, inject the structured doctor JSON into the configured LLM, and return a diagnosis plus concrete remedies. If no LLM is configured, print the normal doctor report and explain how to enable diagnosis.

## Additional Contracts

### Server Availability Semantics

Fallback-local is allowed for transport-level server unavailability:

- DNS failure.
- TCP connect failure.
- connection refused.
- connect timeout.
- read timeout before a valid server response.
- HTTP 502/503/504 from a gateway when the server cannot be reached.

Fallback-local is not silent for semantic or policy failures:

- 401/403 auth failure.
- schema/version mismatch.
- 400 invalid request.
- 404 route not found.
- server 5xx after the request was accepted or may have produced side effects.

Those cases should fail with a targeted remedy unless the user explicitly reruns with `--local`.

### Timeouts

Default client/server timeouts should be explicit:

- connect timeout: 2 seconds for initial route probe.
- request timeout for sync REST calls: 30 seconds unless the endpoint is documented as long-running.
- async job poll interval: 1 second initially, with optional backoff after 30 seconds.
- async job wait timeout: keep the existing command-level timeout default unless overridden by CLI/config.
- fallback decision timeout: use connect timeout plus one lightweight health/capability request, not the full operation timeout.

All timeouts should be configurable and visible in `doctor --json`.

### Conflict Rules

Local/server reconciliation must be content-hash based:

- Same source URL and same content hash: dedupe, mark synced, do not re-embed.
- Same source URL and different content hash: treat as a newer revision unless the server has a newer timestamp.
- Different source URL and same content hash: allow one stored blob with multiple source references.
- Local artifact synced to a server with an existing job for that URL: attach to source/artifact inventory, not necessarily to the old job record.
- Never delete local artifacts after sync unless an explicit cleanup command requests it.

### Endpoint Discovery Guardrails

Normal command execution should use:

1. explicit CLI flags.
2. configured host-valid URLs.
3. known localhost defaults.
4. trusted candidates cached by doctor.

Doctor may probe LAN/Tailscale candidates, but normal commands should not scan networks on every run. Cached candidates must include probe time, endpoint kind, URL, and enough evidence to identify the service as Axon/Qdrant/TEI/Chrome.

### Mode Banner Rules

Human output should stay concise:

- No banner for normal local mode when no server URL is configured.
- One short route note for fallback-local success.
- Warning-style note only for degraded or failed fallback.
- No repeated route notes during polling.

JSON output should always include route metadata.

### Server Version and Schema Checks

Thin clients should check server capabilities before routing:

- server version.
- REST contract/schema version.
- supported routes/actions.
- minimum client schema version.
- auth mode/scope requirements when exposed safely.

If the server is reachable but incompatible, do not silently fallback. Report schema mismatch and recommend rebuilding/restarting or upgrading the server/client.

### MCP Stdio Fallback

Stdio MCP should use the same routing rules as CLI:

- prefer server when `AXON_SERVER_URL` is set.
- fallback local only for safe commands.
- include route metadata in tool responses.
- never silently switch job lifecycle, scheduler, vector admin, or config mutation state stores.

Because agents may not notice subtle state locality, fallback-local MCP responses should include a compact `route_note` field even when the operation completed equivalently.

### Security Boundary

Local fallback and reconciliation must preserve existing file safety rules:

- host-local file reads require explicit allowed roots.
- dotfiles and symlinks remain rejected unless explicitly allowed.
- sync uploads only Axon-owned artifacts by default.
- arbitrary host paths require explicit confirmation or allowed-root config.
- secret-like content should be redacted from route metadata, doctor output, and LLM-powered doctor diagnosis.

### Observability

Route decisions should be logged with structured fields:

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

These events should go to normal logs and be available in `--json` output where relevant.

### Cutover Exit Criteria

Delete `/v1/actions` only after all are true:

- Direct REST routes cover all current server-routed CLI behavior.
- Stdio MCP can thin-client through REST when `AXON_SERVER_URL` is set.
- CLI server mode no longer posts to `/v1/actions`.
- Canonical service request tests cover CLI, REST, and MCP request mapping.
- Auth/scope parity tests cover REST and MCP HTTP.
- Doctor route explanation is implemented.
- Stable JSON route envelope is implemented for routed commands.
- Local fallback and no-silent-fallback policies are tested.

## Dev Wrapper Direction

Current `scripts/axon` behavior:

- Builds host debug binary: `target/debug/axon`.
- Symlinks it into `~/.local/bin/axon`.
- Copies the debug binary to `${AXON_HOME:-~/.axon}/dev/axon`, which is visible inside the dev container at `/home/axon/.axon/dev/axon`.
- `docker-compose.dev.yaml` runs the dev container through `/home/axon/.axon/dev/axon`.
- If the debug binary changed and the `axon` container exists, restarts/recreates the dev container with `--no-build`.
- If image-level inputs changed, starts a background `docker compose build axon && docker compose up -d axon --no-deps`.

Preferred dev behavior:

- Keep building the host debug binary on every invocation.
- For a running dev container, update the mounted debug binary, then restart only the `axon` process/container.
- Avoid full Docker image rebuilds for normal Rust source edits.
- Rebuild the image only when Dockerfile, runtime dependencies, web assets, compose files, or other image inputs change.

Implemented path:

```text
Rust-only edit:
  cargo build --bin axon
  install -m 0755 target/debug/axon ~/.axon/dev/axon
  docker compose -f docker-compose.yaml -f docker-compose.dev.yaml up -d axon --no-deps --no-build
  docker compose -f docker-compose.yaml -f docker-compose.dev.yaml restart axon

Image-input edit:
  docker compose build axon
  docker compose up -d axon --no-deps
```

## Implementation Phases

### Phase 1: Documentation and Visibility

- Add this spec.
- Add `doctor --command` route explanation.
- Add human output warnings when `AXON_SERVER_URL` is set but a command runs locally.
- Add extract provenance guidance for forcing LLM mode.

### Phase 2: REST Parity

- Expand direct REST request bodies to match service/CLI knobs.
- Add REST lifecycle list/cleanup/clear/recover parity where missing.
- Add tests that CLI planning, REST request parsing, and MCP request parsing produce the same canonical service request/job config for crawl/extract/embed/ingest.

### Phase 3: Thin Clients

- Move CLI server mode from `/v1/actions` to direct REST routes.
- Add stdio MCP thin-client mode when `AXON_SERVER_URL` is set.
- Keep local runtime fallback for safe commands.
- Add `--server-required` to disable fallback.
- Remove `/v1/actions` after direct REST and MCP thin-client paths cover the current behavior.

### Phase 4: Local Backlog Sync

- Add artifact/source manifest for local fallback output.
- Add pending embedding/reconciliation state.
- Add server sync endpoint and CLI sync command.
- Embed local backlog when server/vector capability becomes available.

### Phase 5: Dev Wrapper

- Change `scripts/axon` to rebuild the debug binary and update the dev container without full Docker rebuilds for Rust-only edits.
- Keep full image rebuild for image inputs.
- Add debounce/coalescing only for the slower image rebuild path.
