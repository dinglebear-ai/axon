# MCP Tool Contract
Last Modified: 2026-06-30

## Contract

This is the target clean-break MCP contract. The current MCP server already uses
one `axon` tool, but it still advertises the older action set.

MCP is a first-class transport over the same Axon service contracts as CLI and
REST. It must not invent alternate command names, alternate request shapes, or
alternate semantics.

```text
MCP client
  -> tool axon(input)
  -> ActionRouter
  -> axon-api request DTO
  -> axon-services
  -> axon-api result DTO
  -> MCP response envelope
```

The MCP contract must ship in the same release as the CLI/REST clean break.
Agents must not learn a new CLI surface while MCP still advertises old actions.

## Design Rules

- Expose one MCP tool named `axon`.
- MCP Apps/widget tools are allowed only as presentation helpers. They are not
  additional operation tools and must not own source, job, retrieval, memory, or
  graph semantics.
- Use `action` plus optional `subaction`; do not expose one MCP tool per
  operation.
- Route every action to an `axon-api` DTO and `axon-services` entry point.
- Keep source acquisition under `action=source`.
- Keep structured LLM extraction under `action=extract`.
- Keep durable memory under `action=memory`.
- Keep operational surfaces grouped: `jobs`, `watches`, `artifacts`, `uploads`,
  `prune`, `collections`, `graph`, `providers`.
- Keep destructive clean-slate reset under `action=reset` with admin scope and
  explicit confirmation.
- Return structured envelopes for every response.
- Background work must always return a pollable `job` or `watch`.
- Removed actions must be absent from the MCP schema.
- Machine-readable help and capabilities are required; prose-only discovery is
  not enough for agents.

## Current Implementation Snapshot

Implemented today:

- MCP exposes one routed operation tool named `axon`.
- Current MCP also exposes an `axon_status_dashboard` MCP Apps/widget tool for
  dashboard rendering. This is a presentation helper, not a second operation
  surface.
- The active action registry still includes older families such as `crawl`,
  `extract`, `embed`, `ingest`, `scrape`, `purge`, `code_search`,
  `vertical_scrape`, system operations, screenshot, query, retrieve, ask,
  search, research, memory, and task/job helpers.
- The current MCP response envelope is shaped around `ok`, `action`,
  `subaction`, optional `warnings`, and `data`.
- Current MCP scopes are broad `axon:read` / `axon:write` checks.

Planned by this contract:

- Source acquisition moves under `action=source`.
- Removed actions are deleted from the schema and cannot dispatch.
- Responses include the full shared envelope with request/job/progress/warnings
  metadata rather than the current narrower MCP envelope.
- Capabilities expose adapters, scopes, providers, and limits in
  machine-readable form.

## Single Tool Definition

Tool name:

```text
axon
```

Tool description:

```text
Acquire, normalize, embed, refresh, search, retrieve, answer from, inspect, and
operate on Axon source knowledge. Use action=source to index or refresh sources.
Use action=search for external web discovery, action=query for indexed vector
retrieval, action=retrieve for known stored content, and action=ask for RAG
answers.
```

Minimum tool input schema:

```json
{
  "type": "object",
  "required": ["action"],
  "properties": {
    "action": {
      "type": "string",
      "description": "Canonical Axon action."
    },
    "subaction": {
      "type": "string",
      "description": "Grouped operation under memory/jobs/watches/artifacts/uploads/prune/collections/graph/providers."
    },
    "source": {
      "type": "string",
      "description": "Source URI, URL, path, shorthand, package id, repo id, or source id."
    },
    "body": {
      "type": "object",
      "description": "Action-specific payload when the action is too structured for top-level fields."
    },
    "wait": {
      "type": "boolean",
      "default": false
    },
    "stream": {
      "type": "boolean",
      "default": false,
      "description": "Request a streaming response for stream-capable synthesis actions."
    },
    "response_mode": {
      "type": "string",
      "enum": ["auto", "summary", "full", "inline", "artifact", "path", "job_only"],
      "default": "auto"
    }
  },
  "additionalProperties": true
}
```

`additionalProperties` remains true only because action-specific fields are
allowed at the top level for ergonomic MCP calls. Unknown fields must still be
validated against the selected action and surfaced as warnings or errors.

## Common Input Fields

These fields may appear on many actions.

| Field | Type | Applies To | Meaning |
|---|---|---|---|
| `action` | string | all | Required action name. |
| `subaction` | string | grouped actions | Operation inside grouped action. |
| `source` | string | source/map/retrieve/summarize/etc. | Source URI, URL, path, shorthand, or source id. |
| `sources` | string[] | multi-source actions | Multiple source values. |
| `query` | string | search/query/research/memory search/graph query | Search or query text. |
| `question` | string | ask/evaluate | Natural-language question to answer. |
| `schema` | object\|string | extract | Structured extraction schema or schema id. |
| `scope` | string | source/map/watch | Adapter-declared scope. |
| `embed` | bool | source/memory | Whether to store vectors. Default true for source lifecycle. |
| `refresh` | string\|bool | source/watch | `if_stale`, `force`, `never`, or boolean shortcut. |
| `watch` | string\|bool | source/watch | `disabled`, `ensure`, `enabled`, or boolean shortcut. |
| `wait` | bool | async-capable actions | Block until terminal state when supported. |
| `stream` | bool | ask/research/summarize/chat | Emit `StreamEvent` sequence when supported instead of one final payload. |
| `limit` | integer | list/search/query/map | Max returned items. |
| `cursor` | string | paged actions | Pagination cursor. |
| `filters` | object | query/ask/retrieve/jobs/graph | Typed filter object. |
| `include` | string[] | get/detail actions | Extra related data to include. |
| `include_content` | bool | retrieve/artifacts/chunks | Include stored content bytes/text when allowed. |
| `collection` | string | vector actions | Vector collection override. |
| `response_mode` | string | all | `auto`, `summary`, `full`, `inline`, `artifact`, `path`, or `job_only`. |
| `idempotency_key` | string | mutating actions | Caller-provided dedupe key. |

## Streaming Contract

MCP does not use separate `ask_stream`, `research_stream`, `summarize_stream`,
or `chat_stream` actions. The canonical actions are `ask`, `research`,
`summarize`, and `chat` with `stream=true`.

When `stream=true` and the host supports streaming/tool progress, responses are
emitted as `StreamEvent` frames from `schemas/event-schema.md`:

```text
progress -> token -> citation/artifact/warning* -> final
```

Rules:

- `final` contains the same result DTO as the non-streaming action.
- `error` contains the shared `ApiError` projection.
- stream-capable actions still create a `job_id` when durable work is needed.
- hosts without streaming support receive a normal response envelope containing
  either the completed result or a job descriptor plus artifact pointer.
- REST SSE routes and MCP stream frames use the same `StreamEvent` schema.

## Canonical Actions

Direct actions:

```text
source
resolve
map
search
query
retrieve
ask
chat
evaluate
suggest
research
summarize
endpoints
brand
diff
screenshot
extract
memory
jobs
watches
artifacts
uploads
prune
collections
graph
providers
reset
status
doctor
preflight
smoke
capabilities
help
```

Removed actions:

```text
scrape
crawl
embed
ingest
code_search
code_search_watch
purge
dedupe
```

Removed actions are not recognized as canonical actions and are not
compatibility aliases.

## Action Registry

| Action | Subaction | DTO Request | DTO Result | Mutates | Async | Purpose |
|---|---|---|---|---:|---:|---|
| `source` | none | `SourceRequest` | `SourceResult` | yes | yes | Acquire, normalize, embed, refresh, and optionally watch a source. |
| `resolve` | none | `ResolveSourceRequest` | `ResolvedSource` | no | no | Resolve source identity and adapter without acquiring content. |
| `map` | none | `SourceRequest` | `SourceResult` | no | maybe | Discover source items/URLs with `scope=map`, `embed=false`. |
| `search` | none | `SearchRequest` | `SearchResult` | optional | no | External web discovery. |
| `query` | none | `QueryRequest` | `QueryResult` | no | no | Indexed vector/graph retrieval. |
| `retrieve` | none | `RetrievalRequest` | `RetrievalResult` | no | no | Stored content lookup by known identity. |
| `ask` | none | `AskRequest` | `AskResult` | trace only | maybe | RAG answer from indexed context. |
| `chat` | none | `ChatRequest` | `ChatResult` | trace only | maybe | Direct LLM chat without retrieval. |
| `evaluate` | none | `EvaluationRequest` | `EvaluationResult` | trace only | yes | Evaluate RAG answer and baseline. |
| `suggest` | none | `SuggestRequest` | `SuggestResult` | no | maybe | Suggest sources or next acquisition targets. |
| `research` | none | `ResearchRequest` | `ResearchResult` | optional | yes | Web search/fetch/synthesis. |
| `summarize` | none | `SummarizeRequest` | `SummarizeResult` | artifact only | maybe | Fetch and summarize without indexing by default. |
| `endpoints` | none | `EndpointDiscoveryRequest` | `EndpointDiscoveryResult` | artifact only | maybe | Discover network/API endpoints. |
| `brand` | none | `BrandRequest` | `BrandResult` | artifact only | maybe | Extract brand identity assets. |
| `diff` | none | `DiffRequest` | `DiffResult` | artifact only | maybe | Compare two sources. |
| `screenshot` | none | `ScreenshotRequest` | `ScreenshotResult` | artifact | maybe | Capture screenshot artifact. |
| `extract` | none | `ExtractRequest` | `ExtractResult` | artifact/graph optional | yes | Structured LLM extraction. |
| `memory` | required | `Memory*Request` | `Memory*Result` | yes | maybe | Durable memory lifecycle. |
| `jobs` | required | `Job*Request` | `Job*Result` | yes | no | Job status/control. |
| `watches` | required | `Watch*Request` | `Watch*Result` | yes | no | Watch lifecycle. |
| `artifacts` | required | `Artifact*Request` | `Artifact*Result` | no | no | Artifact listing/detail/content. |
| `uploads` | required | `Upload*Request` | `Upload*Result` | yes | no | Staged uploads. |
| `prune` | required | `Prune*Request` | `Prune*Result` | yes | yes | Cleanup, purge, dedupe. |
| `collections` | required | `Collection*Request` | `Collection*Result` | maybe | no | Collection listing/detail/maintenance. |
| `graph` | required | `Graph*Request` | `Graph*Result` | no | no | SourceGraph query/resolve/detail. |
| `providers` | required | `Provider*Request` | `Provider*Result` | no | no | Provider capabilities/health. |
| `reset` | none | `Reset*Request` | `Reset*Result` | yes | yes | Explicit destructive clean-slate reset. |
| `status` | none | `StatusRequest` | `StatusReport` | no | no | Runtime status. |
| `doctor` | none | `DoctorRequest` | `DoctorReport` | no | maybe | Diagnostic checks. |
| `preflight` | none | `PreflightRequest` | `PreflightReport` | no | maybe | Readiness checks before starting work. |
| `smoke` | none | `SmokeRequest` | `SmokeReport` | maybe | yes | Explicit live smoke checks against configured providers. |
| `capabilities` | none | `CapabilityRequest` | `CapabilityDocument` | no | no | Machine-readable server capability contract. |
| `help` | none | `HelpRequest` | `HelpDocument` | no | no | Agent-facing help/action schema. |

## Source Action

`action=source` is the only source acquisition/indexing happy path.

Example:

```json
{
  "action": "source",
  "source": "shadcn.com",
  "scope": "docs",
  "embed": true,
  "refresh": "if_stale",
  "watch": "disabled",
  "wait": false,
  "response_mode": "summary"
}
```

Normalized request:

```json
{
  "source": "shadcn.com",
  "scope": "docs",
  "embed": true,
  "refresh": "if_stale",
  "watch": "disabled",
  "wait": false,
  "options": {}
}
```

Rules:

- `source` is required.
- `embed` defaults to true.
- `scope` defaults through adapter capability rules.
- `watch=true` means create or ensure a durable watch.
- `refresh=true` means force refresh.
- `wait=false` returns a job descriptor when work is asynchronous.
- `response_mode=inline` may still be upgraded to `artifact` when output exceeds
  MCP size limits.

## Resolve and Map Actions

`action=resolve` resolves identity without acquisition.

```json
{
  "action": "resolve",
  "source": "shadcn-ui/ui"
}
```

`action=map` discovers source items, links, URLs, tools, resources, or package
members without embedding.

```json
{
  "action": "map",
  "source": "shadcn.com",
  "scope": "map",
  "limit": 100
}
```

Map rules:

- `map` is a projection over `SourceRequest`.
- `scope` defaults to `map`.
- `embed` defaults to false and must remain false unless explicitly set.
- `map` may fetch sitemaps/indexes/tool schemas.
- `map` must not publish vectors as a side effect.

## Retrieval and Search Action Boundaries

MCP actions must preserve the same boundaries as CLI and REST. Agents should
not need to guess whether "search" means web search, vector search, lookup, or
RAG synthesis.

| Action | Primary Question | Input Interpreted As | Reads | Writes | Calls Web Search | Calls LLM | Output |
|---|---|---|---|---|---:|---:|---|
| `search` | "What does the outside web say exists for this query?" | Search-engine query text | `SearchProvider` | optional source jobs only when `auto_source=true` | yes | no | web result list, source hints, optional queued jobs |
| `query` | "Which indexed chunks match this text?" | Semantic/vector query text | `VectorStore`, optional `SourceGraph`, optional `DocumentCache` | no | no | no | ranked chunks/documents with scores and metadata |
| `retrieve` | "Show me the stored content for this known source/document/url." | Source id, document id, chunk id, URL, or canonical source URI | `SourceLedger`, `DocumentCache`, `ArtifactStore`, `VectorStore` metadata | no | no | no | stored documents/chunks/content in source order |
| `ask` | "Answer my question from indexed knowledge." | Natural-language question | retrieval stack: `VectorStore`, `SourceGraph`, `DocumentCache`, `MemoryStore` when requested | optional trace/job/event rows | no | yes | synthesized answer with citations and retrieval trace |

Action rules:

- `action=search` is external discovery. It must not read VectorStore or pretend
  to answer from indexed knowledge.
- `action=query` is indexed semantic retrieval. It must not call `LlmProvider`
  and must not synthesize prose beyond concise result summaries.
- `action=retrieve` is lookup by known identity. It must not run semantic search
  unless the request explicitly includes a separate query field for filtering
  inside the retrieved source.
- `action=ask` is RAG synthesis. It may call `LlmProvider`, but it must not
  acquire new source/web content by default.
- Source acquisition/indexing remains `action=source`; none of these actions are
  aliases for source ingestion.

Validation rules:

- `search` requires `query`.
- `query` requires `query`.
- `retrieve` requires one of `source`, `source_id`, `document_id`, `url`, or
  `chunk_id`.
- `ask` requires `question`.
- When the caller sends a natural-language question to `query`, the tool still
  returns retrieval results, not an answer.
- When the caller sends only a URL/source id to `ask`, return a validation error
  suggesting `retrieve` or `source`.

## Retrieval Action Schemas

### search

```json
{
  "action": "search",
  "query": "latest qdrant payload indexing",
  "limit": 10,
  "time_range": "month",
  "auto_source": false
}
```

Result data:

```json
{
  "results": [
    {
      "title": "Result title",
      "url": "https://example.com",
      "snippet": "Short snippet",
      "source_hint": {
        "source": "https://example.com",
        "scope": "page"
      }
    }
  ],
  "jobs": [],
  "warnings": []
}
```

### query

```json
{
  "action": "query",
  "query": "source ledger generation cleanup",
  "filters": {
    "source_kind": "git",
    "content_kind": "code"
  },
  "generation": "committed",
  "limit": 10,
  "include_graph": true
}
```

Result data:

```json
{
  "results": [
    {
      "chunk_id": "chk_...",
      "document_id": "doc_...",
      "source_id": "src_...",
      "score": 0.82,
      "chunk_locator": "crates/axon-ledger/src/lib.rs:42-96",
      "metadata": {}
    }
  ],
  "next_cursor": null,
  "warnings": []
}
```

### retrieve

```json
{
  "action": "retrieve",
  "source": "https://ui.shadcn.com/docs",
  "include_content": true,
  "limit": 50
}
```

Result data:

```json
{
  "documents": [
    {
      "document_id": "doc_...",
      "source_id": "src_...",
      "canonical_uri": "https://ui.shadcn.com/docs",
      "chunks": []
    }
  ],
  "warnings": []
}
```

### ask

```json
{
  "action": "ask",
  "question": "How should source generations be published?",
  "filters": {
    "source_id": "src_..."
  },
  "include_trace": true
}
```

Result data:

```json
{
  "answer": "Use a write generation and publish only after all chunks are ready...",
  "citations": [
    {
      "chunk_id": "chk_...",
      "locator": "docs/pipeline-unification/source-pipeline.md:40-63"
    }
  ],
  "retrieval": {},
  "graph": {},
  "model": {
    "provider": "gemini-headless",
    "model": "configured-model"
  },
  "warnings": []
}
```

## Analysis and Inspection Actions

These actions are not source acquisition happy paths. They may fetch/render/call
providers, and may write artifacts, but they do not index by default.

| Action | Required Fields | Optional Fields | Result |
|---|---|---|---|
| `chat` | `message` | `system`, `model`, `temperature`, `history`, `stream` | `ChatResult` |
| `evaluate` | `question` | `expected`, `filters`, `judge`, `limit` | `EvaluationResult` |
| `suggest` | none | `focus`, `source_id`, `limit`, `constraints` | `SuggestResult` |
| `research` | `query` | `limit`, `depth`, `full_content`, `auto_source`, `stream` | `ResearchResult` |
| `summarize` | one of `source`, `url`, `urls` | `instructions`, `format`, `headers`, `stream` | `SummarizeResult` |
| `endpoints` | `source` or `url` | `render_mode`, `capture`, `limit` | `EndpointDiscoveryResult` |
| `brand` | `source` or `url` | `render_mode`, `include_screenshot` | `BrandResult` |
| `diff` | `source_a`, `source_b` | `mode`, `headers` | `DiffResult` |
| `screenshot` | `source` or `url` | `viewport`, `full_page`, `render_mode`, `wait_for` | `ScreenshotResult` |
| `extract` | `source`, `schema` | `instructions`, `persist_artifact`, `trusted_graph_write` | `ExtractResult` |

Rules:

- `research` may create source jobs only when `auto_source=true`.
- `summarize`, `endpoints`, `brand`, `diff`, and `screenshot` may write
  artifacts.
- `extract` writes structured results and may write graph candidates only when
  explicitly trusted by the caller/service policy.
- `chat` has no retrieval by default. Use `ask` for RAG.

## Memory Action

Memory uses grouped subactions.

| Subaction | Required Fields | Optional Fields | Mutates |
|---|---|---|---:|
| `remember` | `body` | `memory_type`, `scope`, `embed`, `graph_links` | yes |
| `search` | `query` | `scope`, `limit`, `include_archived` | no |
| `context` | `prompt` or `source` | `budget_tokens`, `scope`, `include_working` | no |
| `show` | `memory_id` | `include_graph`, `include_events` | no |
| `link` | `memory_id`, `target` | `edge_kind`, `confidence` | yes |
| `supersede` | `old_memory_id`, `new_memory_id` | `reason` | yes |
| `reinforce` | `memory_id` | `signal`, `amount`, `context` | yes |
| `contradict` | `memory_id`, `other_memory_id` | `reason` | yes |
| `pin` | `memory_id` | `reason` | yes |
| `archive` | `memory_id` | `reason` | yes |
| `forget` | `memory_id` | `reason`, `hard_delete` | yes |
| `review` | none | `reason`, `limit`, `cursor` | maybe |
| `compact` | `memory_ids` | `instructions`, `target_scope` | yes |

Example:

```json
{
  "action": "memory",
  "subaction": "remember",
  "body": "Use SourceGraph for typed source relationships.",
  "memory_type": "decision",
  "scope": {
    "kind": "repo",
    "source": "github.com/jmagar/axon"
  },
  "embed": true
}
```

Memory rules:

- Memory is not a source adapter.
- Memory may embed through `EmbeddingProvider` and `VectorStore`.
- Memory may create graph nodes/edges through `GraphStore`.
- Memory decay, reinforcement, supersession, review, and forgetting belong to
  `axon-memory`, not source acquisition.

## Jobs Action

| Subaction | Required Fields | Optional Fields | Result |
|---|---|---|---|
| `list` | none | `status`, `kind`, `limit`, `cursor` | paged job summaries |
| `get` | `job_id` | `include`, `include_events` | job detail |
| `events` | `job_id` | `after_sequence`, `limit`, `cursor` | progress/event page |
| `cancel` | `job_id` | `reason` | cancellation result |
| `retry` | `job_id` | `from_phase`, `idempotency_key` | new job descriptor |
| `recover` | none | `kind`, `older_than_seconds` | recovery summary |
| `cleanup` | none | `older_than`, `dry_run` | cleanup summary |
| `clear` | none | `status`, `older_than`, `confirm` | clear summary |

Job descriptors must include `job_id`, `kind`, `status`, `phase`,
`poll_after_ms`, and the exact MCP polling request.

## Watches Action

| Subaction | Required Fields | Optional Fields | Result |
|---|---|---|---|
| `create` | `source` | `scope`, `every`, `embed`, `refresh`, `options` | watch descriptor |
| `list` | none | `status`, `source_id`, `limit`, `cursor` | paged watches |
| `get` | `watch_id` | `include_history` | watch detail |
| `status` | `watch_id` | none | heartbeat/progress |
| `exec` | `source` or `watch_id` | `wait`, `refresh` | job descriptor/result |
| `pause` | `watch_id` | `reason` | watch detail |
| `resume` | `watch_id` | none | watch detail |
| `delete` | `watch_id` | `delete_state`, `reason` | deletion result |
| `history` | `watch_id` | `limit`, `cursor` | run history |

`watch exec` replaces the older `run-now` wording in the contract.

## Artifacts and Uploads Actions

Artifact subactions:

| Subaction | Required Fields | Optional Fields | Result |
|---|---|---|---|
| `list` | none | `kind`, `source_id`, `job_id`, `limit`, `cursor` | artifact page |
| `get` | `artifact_id` | `include_content_url` | artifact metadata |
| `content` | `artifact_id` | `download`, `range` | content pointer or inline content |

Upload subactions:

| Subaction | Required Fields | Optional Fields | Result |
|---|---|---|---|
| `create` | `filename`, `content_type`, `size_bytes`, `purpose` | `sha256`, `source_hint` | upload descriptor |
| `get` | `upload_id` | none | upload status |
| `put_content` | `upload_id`, `content` or `content_ref` | `sha256` | received status |
| `complete` | `upload_id` | `sha256`, `source_options` | artifact/source ref |
| `abort` | `upload_id` | `reason` | abort result |

Large MCP payloads must use uploads/artifacts rather than stuffing raw bytes
into tool arguments/results.

## Prune, Collections, Graph, and Providers Actions

Prune subactions:

| Subaction | Required Fields | Optional Fields | Result |
|---|---|---|---|
| `plan` | `targets` | `include`, `retention`, `filters` | prune plan |
| `exec` | `prune_plan_id` or inline plan | `confirm` | job descriptor |
| `dedupe` | none | `collection`, `threshold`, `source_id`, `dry_run` | summary/job |
| `purge` | `target` | `prefix`, `dry_run`, `confirm` | summary/job |

Collection subactions:

| Subaction | Required Fields | Optional Fields | Result |
|---|---|---|---|
| `list` | none | none | collection summaries |
| `get` | `collection` | `include_schema`, `include_indexes` | collection detail |

Graph subactions:

| Subaction | Required Fields | Optional Fields | Result |
|---|---|---|---|
| `kinds` | none | none | supported node/edge/evidence kinds |
| `resolve` | `identifier` | `kind`, `limit` | graph matches |
| `query` | `query` or structured `body` | `limit`, `cursor` | graph query result |
| `node` | `node_id` | `include_edges`, `include_evidence` | node detail |
| `edge` | `edge_id` | `include_evidence` | edge detail |
| `source` | `source_id` | `depth`, `edge_kind`, `limit`, `cursor` | source subgraph |

Provider subactions:

| Subaction | Required Fields | Optional Fields | Result |
|---|---|---|---|
| `list` | none | `kind`, `status` | provider summaries |
| `get` | `provider_id` | `include_health`, `include_limits` | provider detail |

## Response Envelope

Every MCP response returns the same envelope. The `data` shape is the exact
`axon-api` result DTO for the action.

Success:

```json
{
  "ok": true,
  "action": "source",
  "subaction": null,
  "request_id": "req_...",
  "contract_version": "2026-06-30",
  "data": {},
  "job": null,
  "watch": null,
  "artifacts": [],
  "warnings": [],
  "pagination": null,
  "trace": {
    "job_id": "job_...",
    "trace_id": "trace_..."
  }
}
```

Failure:

```json
{
  "ok": false,
  "action": "source",
  "subaction": null,
  "request_id": "req_...",
  "contract_version": "2026-06-30",
  "error": {
    "code": "source.resolve.unsupported",
    "message": "No adapter can resolve this source.",
    "stage": "resolving",
    "retryable": false,
    "severity": "failed",
    "details": {}
  },
  "warnings": [],
  "trace": {
    "job_id": null,
    "trace_id": "trace_..."
  }
}
```

Envelope fields:

| Field | Required | Meaning |
|---|---:|---|
| `ok` | yes | Boolean success flag. |
| `action` | yes | Canonical action executed or rejected. |
| `subaction` | no | Grouped subaction. |
| `request_id` | yes | Request correlation id. |
| `contract_version` | yes | MCP contract version. |
| `data` | on success | Action result DTO. |
| `error` | on failure | Structured error. |
| `job` | when async | Pollable job descriptor. |
| `watch` | when watch created/used | Watch descriptor. |
| `artifacts` | no | Artifact refs produced by the call. |
| `warnings` | yes | Non-fatal warnings. |
| `pagination` | when paged | Cursor/page metadata. |
| `trace` | yes | Trace/job correlation ids. |

## Job and Task Behavior

Background work must include a job descriptor:

```json
{
  "job_id": "job_...",
  "kind": "source",
  "status": "running",
  "phase": "embedding",
  "poll_after_ms": 1000,
  "poll": {
    "action": "jobs",
    "subaction": "get",
    "job_id": "job_..."
  },
  "events": {
    "action": "jobs",
    "subaction": "events",
    "job_id": "job_..."
  }
}
```

MCP task ids, when the client supports task augmentation:

```text
axon:<job_kind>:<job_id>
```

Task/progress events must reuse `SourceProgressEvent`:

```json
{
  "event_id": "evt_...",
  "sequence": 42,
  "job_id": "job_...",
  "source_id": "src_...",
  "phase": "embedding",
  "status": "running",
  "severity": "info",
  "visibility": "public",
  "message": "embedding changed files",
  "timestamp": "2026-06-30T20:20:00Z",
  "counts": {
    "items_total": 1200,
    "items_done": 431,
    "chunks_total": 5200,
    "chunks_done": 1800,
    "bytes_total": 1234567,
    "bytes_done": 456789
  },
  "current": {
    "source_item_key": "src/lib.rs",
    "adapter": "github"
  }
}
```

## Pagination

Paged actions use the same shape everywhere.

Request fields:

```json
{
  "limit": 50,
  "cursor": "opaque_cursor"
}
```

Response metadata:

```json
{
  "pagination": {
    "limit": 50,
    "next_cursor": "opaque_cursor_or_null",
    "has_more": true
  }
}
```

Do not expose offset pagination for large stores unless the underlying service
contract explicitly supports it.

## Response Modes and Size Limits

MCP has practical result-size limits. The tool must never silently truncate
important data without returning an artifact or cursor.

| Mode | Behavior |
|---|---|
| `inline` | Return full content only if below size and visibility limits. |
| `summary` | Return concise summary plus ids/cursors/artifacts. |
| `artifact` | Write full output to ArtifactStore and return artifact refs. |
| `path` | Return local path/content pointer when safe for local clients. |
| `auto` | Choose inline for small safe output, artifact for large output. |

Rules:

- Large `retrieve`, `research`, `summarize`, `endpoints`, `screenshot`,
  `extract`, tool-output, and upload responses should use artifacts.
- If content is omitted due to size, include `artifact_id` or `next_cursor`.
- If content is omitted due to auth/redaction, include a warning with a stable
  code.

## Machine-Readable Help

`action=help` must return a full agent-usable schema, not prose only.

Required top-level fields:

```json
{
  "contract_version": "2026-06-30",
  "tool": {
    "name": "axon",
    "description": "..."
  },
  "actions": {},
  "subactions": {},
  "adapters": [],
  "scopes": {},
  "removed_actions": {},
  "examples": [],
  "limits": {},
  "auth": {},
  "warnings": []
}
```

Each action entry must include:

- description
- request DTO
- result DTO
- required fields
- optional fields
- defaults
- whether it mutates state
- whether it can run async
- whether it calls external providers
- whether it can return artifacts
- examples
- validation errors

Example action entries:

```json
{
  "search": {
    "request": "SearchRequest",
    "result": "SearchResult",
    "description": "External web discovery. Calls SearchProvider. Does not read indexed vectors.",
    "required": ["query"],
    "optional": ["limit", "time_range", "auto_source"],
    "mutates": "only_when_auto_source_true",
    "calls": ["SearchProvider"]
  },
  "query": {
    "request": "QueryRequest",
    "result": "QueryResult",
    "description": "Semantic retrieval over indexed vectors. Does not call an LLM.",
    "required": ["query"],
    "optional": ["filters", "generation", "limit", "include_graph"],
    "mutates": false,
    "calls": ["VectorStore"]
  },
  "retrieve": {
    "request": "RetrievalRequest",
    "result": "RetrievalResult",
    "description": "Identity lookup for stored documents/chunks.",
    "required_one_of": ["source", "source_id", "document_id", "url", "chunk_id"],
    "optional": ["include_content", "limit", "cursor"],
    "mutates": false,
    "calls": ["SourceLedger", "DocumentCache", "ArtifactStore"]
  },
  "ask": {
    "request": "AskRequest",
    "result": "AskResult",
    "description": "RAG answer. Retrieves indexed context, then calls LlmProvider.",
    "required": ["question"],
    "optional": ["filters", "retrieval", "synthesis", "include_trace"],
    "mutates": "trace_only",
    "calls": ["VectorStore", "GraphStore", "LlmProvider"]
  }
}
```

## Capabilities Action

`action=capabilities` returns runtime capabilities, not only static help.

It must include:

- server version and contract version
- enabled auth mode
- enabled actions/subactions
- source adapters, scopes, and option schemas
- provider capabilities and health summaries
- size limits
- artifact/upload support
- graph support
- memory support
- known degraded modes
- reset support and confirmation policy
- removed action absence

## Auth and Visibility

MCP auth scopes map to the same auth model as REST.

| Operation Class | Required Scope |
|---|---|
| read status/capabilities/help | `axon:read` |
| query/retrieve/ask/search/research/summarize | `axon:read` |
| source acquisition/watch/upload/prune/memory mutation | `axon:write` |
| destructive prune/purge/forget/hard delete/reset | `axon:write` plus admin policy and explicit confirmation |
| provider diagnostics that reveal config | admin/internal policy |

Visibility rules:

- The MCP tool must not leak secrets in errors, logs, payloads, or help.
- Local absolute paths are hidden unless explicitly allowed by local policy.
- Artifact content may require an additional read check.
- Tool/MCP-source execution must declare side-effect class and allowlist policy.

Action auth metadata:

Every action/subaction in the generated schema includes:

| Field | Meaning |
|---|---|
| `required_scope` | Minimum scope for the default non-mutating form. |
| `required_scope_if` | Conditional scope upgrades, such as `auto_source=true` or `persist_artifact=true`. |
| `mutates` | Whether the default form writes state. |
| `mutates_if` | Conditional mutation rules for options that create jobs/artifacts/graph writes. |
| `side_effect_class` | `none`, `read_external`, `write_local`, `call_tool`, `destructive`. |
| `execution_affinity` | `local`, `server`, `remote_safe`, or `restricted`. |

## Errors

All errors use a stable shape:

```json
{
  "code": "action.validation.missing_field",
  "message": "action=query requires field `query`.",
  "stage": "parsing",
  "retryable": false,
  "severity": "failed",
  "details": {
    "field": "query",
    "action": "query"
  }
}
```

Common error codes:

| Code | Meaning |
|---|---|
| `action.unknown` | Action is not canonical. |
| `action.validation.missing_field` | Required field missing. |
| `action.validation.invalid_field` | Field invalid for action. |
| `action.validation.unsupported_subaction` | Grouped action subaction not supported. |
| `source.resolve.unsupported` | No adapter can resolve source. |
| `source.scope.unsupported` | Adapter does not support requested scope. |
| `provider.unavailable` | Required provider is down/unhealthy. |
| `auth.denied` | Caller lacks required scope. |
| `output.too_large` | Output requires artifact/cursor. |
| `content.redacted` | Content exists but is not visible. |

## Removed Actions

Removed actions are absent from the MCP schema. They are not runtime aliases and
do not have a public tombstone window. If a caller sends a removed action string
anyway, the server treats it as `action.unknown` and performs no side effects.

Removed-action guidance may appear only in human documentation and generated
developer diagnostics. It must not appear as executable remap code, schema
aliases, hidden action variants, or tool examples that an agent could call.

## Crosswalk to CLI and REST

| MCP | CLI | REST | API DTO |
|---|---|---|---|
| `action=source` | `axon <source>` | `POST /v1/sources` | `SourceRequest` |
| `action=resolve` | internal/diagnostic | `POST /v1/resolve` | `ResolveSourceRequest` |
| `action=map` | `axon map <source>` | `POST /v1/map` | `SourceRequest` |
| `action=search` | `axon search <query>` | `POST /v1/search` | `SearchRequest` |
| `action=query` | `axon query <query>` | `POST /v1/query` | `QueryRequest` |
| `action=retrieve` | `axon retrieve <source-or-url>` | `POST /v1/retrieve` | `RetrievalRequest` |
| `action=ask` | `axon ask <question>` | `POST /v1/ask` | `AskRequest` |
| `action=chat` | `axon chat <message>` | `POST /v1/chat` | `ChatRequest` |
| `action=evaluate` | `axon evaluate <question>` | `POST /v1/evaluate` | `EvaluationRequest` |
| `action=suggest` | `axon suggest [focus]` | `POST /v1/suggest` | `SuggestRequest` |
| `action=research` | `axon research <query>` | `POST /v1/research` | `ResearchRequest` |
| `action=summarize` | `axon summarize <source>` | `POST /v1/summarize` | `SummarizeRequest` |
| `action=endpoints` | `axon endpoints <source>` | `POST /v1/endpoints` | `EndpointDiscoveryRequest` |
| `action=brand` | `axon brand <source>` | `POST /v1/brand` | `BrandRequest` |
| `action=diff` | `axon diff <a> <b>` | `POST /v1/diff` | `DiffRequest` |
| `action=screenshot` | `axon screenshot <source>` | `POST /v1/screenshot` | `ScreenshotRequest` |
| `action=extract` | `axon extract <source>` | `POST /v1/extract` | `ExtractRequest` |
| `action=memory` | `axon memory <sub>` | `/v1/memories/*` | `Memory*` |
| `action=jobs` | `axon jobs <sub>` | `/v1/jobs/*` | `Job*` |
| `action=watches` | `axon watch <sub>` | `/v1/watches/*` | `Watch*` |
| `action=artifacts` | `axon artifacts <sub>` | `/v1/artifacts/*` | `Artifact*` |
| `action=uploads` | `axon uploads <sub>` | `/v1/uploads/*` | `Upload*` |
| `action=prune` | `axon prune <sub>` | `/v1/prune/*` | `Prune*` |
| `action=collections` | `axon collections <sub>` | `/v1/collections/*` | `Collection*` |
| `action=graph` | `axon graph <sub>` | `/v1/graph/*` | `Graph*` |
| `action=providers` | `axon providers <sub>` | `/v1/providers/*` | `Provider*` |
| `action=reset` | `axon reset` | `/v1/reset/*` | `Reset*` |

## Validation Checklist

Implementation is incomplete until all of these pass:

- `help` advertises every canonical action.
- `capabilities` advertises every enabled adapter, scope, provider, and limit.
- Every action maps to an `axon-api` DTO.
- Every grouped action rejects unknown subactions with a structured error.
- `search`, `query`, `retrieve`, and `ask` obey their boundary rules.
- Async source/prune/extract/evaluate/research work returns a job descriptor.
- Large outputs return artifacts or cursors rather than silent truncation.
- Removed actions are absent from the schema and cannot dispatch.
- Auth failures do not reveal whether private content exists.
- Public responses are redacted according to `metadata-payload.md`.
- MCP examples in `help` match CLI and REST examples.

## Example Calls

Index a source:

```json
{
  "action": "source",
  "source": "/home/jmagar/workspace/axon",
  "scope": "repo",
  "watch": true,
  "wait": false
}
```

Search the outside web:

```json
{
  "action": "search",
  "query": "spider.rs sitemap API",
  "auto_source": false
}
```

Query indexed chunks:

```json
{
  "action": "query",
  "query": "SourceLedger cleanup debt",
  "filters": {
    "content_kind": "markdown"
  }
}
```

Retrieve stored content:

```json
{
  "action": "retrieve",
  "source": "github.com/jmagar/axon",
  "include_content": true
}
```

Ask a RAG question:

```json
{
  "action": "ask",
  "question": "What does the pipeline unification plan require for metadata?",
  "filters": {
    "source_id": "src_..."
  },
  "include_trace": true
}
```

Create memory:

```json
{
  "action": "memory",
  "subaction": "remember",
  "body": "MCP must expose one action-dispatched axon tool.",
  "memory_type": "decision",
  "scope": {
    "kind": "repo",
    "source": "github.com/jmagar/axon"
  }
}
```

Poll a job:

```json
{
  "action": "jobs",
  "subaction": "get",
  "job_id": "job_..."
}
```
