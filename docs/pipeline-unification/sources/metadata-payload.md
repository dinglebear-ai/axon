# Metadata and Payload Contract
Last Modified: 2026-06-30

## Contract

This is the target metadata contract. Current Qdrant payloads are narrower and
must be expanded during implementation.

Every item that enters Axon must carry one shared metadata envelope from
resolution through acquisition, ledger diffing, document preparation, embedding,
retrieval, graph extraction, memory, and status reporting.

Source-specific metadata is additive. It must never replace, rename, or
reinterpret shared fields.

The same logical identifiers must be visible in:

- SourceLedger rows
- SourceGraph nodes, edges, and evidence
- DocumentStatus rows
- VectorStore payloads
- ArtifactStore metadata
- MemoryStore rows when memory is involved
- progress events, logs, traces, and job status
- retrieval citations and ask/evaluate traces

Metadata is not a dumping ground for arbitrary adapter blobs. Every field must
have an owner, a stability rule, and a redaction rule.

## Current Implementation Snapshot

Implemented today:

- Ordinary vector payloads currently include core fields such as `url`,
  `domain`, `source_type`, `content_type`, `chunk_index`, `chunk_text`,
  `seed_url`, `scraped_at`, `payload_schema_version`, optional `title`,
  optional `extractor_name`, structured data, and safe extra fields.
- Local code payloads add fields such as `local_project_key`,
  `local_project_display`, `local_file_hash`, `local_index_version`,
  `local_generation`, `code_file_path`, and `code_path_prefixes`.
- Prepared chunks can add fields such as `chunk_content_kind`, `chunk_locator`,
  `source_range`, code language/type fields, and symbol extraction metadata.
- Memory uses `memory://<uuid>` source URLs, embeds atomic memory chunks into
  the configured memory collection, and stores memory node metadata in SQLite.

Planned by this contract:

- Required target fields such as `vector_namespace`, `source_id`,
  `source_kind`, `source_adapter`, `source_scope`, `source_generation`,
  `document_id`, `chunk_id`, `chunk_hash`, `job_id`, `document_status`,
  `embedding_model`, `embedding_dimensions`, `embedding_provider`,
  `embedding_profile`, and `embedded_at` are not emitted universally today.
- The implementation must promote these fields into shared payload construction
  instead of leaving them as adapter-specific extras.

## Goals

- make every stored chunk traceable back to the exact source item, job,
  generation, adapter, and content range that produced it
- make Qdrant/vector filtering fast without requiring heavy scroll/facet queries
- let SourceLedger own freshness, generation, cleanup, and retry state
- let SourceGraph link repos, docs, packages, tools, sessions, issues, and
  external resources without guessing after the fact
- let retrieval cite stable document and chunk locators
- keep source-specific optimization possible without fragmenting the pipeline
- make it obvious which fields are safe to expose through REST/MCP/CLI
- make stale payload fields detectable during implementation

## Naming Rules

Field names use snake_case. Shared fields use stable prefixes:

| Prefix | Owner | Meaning |
|---|---|---|
| `source_*` | SourceResolver, SourceRouter, SourceLedger | source identity, scope, generation, adapter, item keys |
| `job_*` | JobStore | source job/run identity and execution state |
| `document_*` | DocumentPreparer, DocumentStatus | document-level identity and status |
| `content_*` | SourceAdapter, DocumentPreparer | normalized content properties |
| `chunk_*` | DocumentPreparer | chunk identity, range, chunking method |
| `embedding_*` | EmbeddingProvider | model/batch/vector metadata |
| `vector_*` | VectorStore | point ids, collection, vector mode |
| `graph_*` | SourceGraph | graph nodes, edges, evidence, confidence |
| `artifact_*` | ArtifactStore | large result and binary/object metadata |
| `memory_*` | MemoryStore | durable memory lifecycle metadata |
| `web_*` | web adapter | URL, crawl, render, HTTP metadata |
| `git_*` | git adapters | repo/ref/commit/provider metadata |
| `code_*` | parser/chunker | code file, parser, symbol, dependency metadata |
| `package_*` | registry adapters | package registry and release metadata |
| `session_*` | session adapter/parser | Claude/Codex/Gemini session metadata |
| `tool_*` | cli/mcp adapters and session parser | CLI/MCP tool identity and execution metadata |

Adapters may add namespaced fields, but new shared fields must be promoted here
before implementation. No transport-specific names such as `mcp_*` or `rest_*`
belong in document payloads unless the source itself is an MCP object.

## Visibility Classes

Every field must be classified before it is emitted.

| Class | May Appear In | Rule |
|---|---|---|
| `public` | REST/MCP/CLI, vector payloads, citations | safe by default |
| `internal` | ledger, jobs, logs, traces, status | may reveal local topology or operational internals |
| `sensitive` | encrypted/controlled stores only | credentials, raw headers, tokens, private env values |
| `redacted` | public/internal after scrubbing | original exists only in a safer store or not at all |
| `derived` | anywhere allowed by source fields | computed from other fields; must be reproducible |

Public document/chunk payloads must not contain:

- absolute local home paths unless explicitly allowed by the user
- raw request/response headers that may contain credentials
- bearer tokens, API keys, cookies, session ids, or signed URLs
- unredacted environment variables
- raw tool inputs marked sensitive
- raw LLM prompts that include secrets
- private file contents when the caller lacks the matching auth scope

## Lifecycle Shapes

### SourceRequest Metadata

The request layer starts the envelope. It should not invent document ids or
chunk ids.

| Field | Type | Required | Visibility | Description |
|---|---|---:|---|---|
| `request_id` | string | yes | internal | Transport request correlation id. |
| `job_id` | string | yes when async or durable | public | Durable source job id. Prefer `job_id` over `run_id`. |
| `requested_uri` | string | yes | public | Raw user input before normalization. |
| `source_canonical_uri` | string | yes | public | Normalized canonical source identity. |
| `source_scope` | string | yes | public | Requested or adapter-default scope. |
| `source_options_hash` | string | yes | internal | Hash of source options that affect output. |
| `embed_requested` | bool | yes | public | False only for `--no-embed`, map, or explicit analysis-only operations. |
| `watch_requested` | bool | yes | public | Whether freshness/watch lifecycle is requested. |
| `refresh_requested` | bool | yes | public | Whether existing source state should be forced stale. |
| `requested_by` | string | no | internal | User, agent, token subject, or local actor. |
| `idempotency_key` | string | no | public | Caller-provided dedupe key for job creation. |

### Resolved Source Metadata

The resolver canonicalizes identity and chooses an adapter.

| Field | Type | Required | Visibility | Description |
|---|---|---:|---|---|
| `source_id` | string | yes | public | Stable ledger id for the canonical source identity. |
| `source_kind` | string | yes | public | Broad canonical `SourceKind` enum value such as `web`, `git`, `local`, `registry`, `feed`, `reddit`, `youtube`, `session`, `cli_tool`, `mcp_tool`, `memory`, or `upload`. |
| `source_adapter` | string | yes | public | Adapter name that will acquire source items. |
| `source_adapter_version` | string | yes | public | Adapter contract/schema version. |
| `source_canonical_uri` | string | yes | public | Canonical URI selected by resolver for the source identity. |
| `source_authority` | string | yes | public | `official`, `user_pinned`, `inferred`, `community`, `mirror`, or `unknown`. |
| `authority_evidence` | string[] | no | public | Evidence URIs or graph ids used to choose authority. |
| `source_display_name` | string | no | public | Human-friendly label. |
| `source_domain` | string | when URL-like | public | Normalized registrable domain or host. |
| `source_owner` | string | no | public | Org/user/project owner when applicable. |
| `source_slug` | string | no | public | Stable short slug for display and filtering. |
| `source_fingerprint` | string | yes | internal | Hash of canonical identity fields. |

Provider-specific names are not `source_kind` values. A GitHub repo uses
`source_kind="git"`, `source_adapter="github"`, and `git_provider="github"`.
A local checkout uses `source_kind="local"` and `source_adapter="local"`.

### Ledger Metadata

Ledger fields own freshness, diffing, generation, leases, and cleanup. Vector
payloads may copy a subset, but the ledger remains authoritative.

| Field | Type | Required | Visibility | Description |
|---|---|---:|---|---|
| `source_generation` | integer | when mutable | public | Generation being written or published. |
| `committed_generation` | integer | when mutable | public | Latest generation safe for search. |
| `previous_generation` | integer | no | public | Generation used for diffing. |
| `generation_status` | string | when mutable | public | Shared `LifecycleStatus`: `pending`, `running`, `completed`, `completed_degraded`, `failed`, `canceled`, etc. |
| `generation_publish_state` | string | when mutable | public | Publish-specific state: `planning`, `writing`, `publishing`, `committed`, `cleanup_pending`, `cleaning`, `cleaned`. |
| `source_item_key` | string | yes | public | Stable item key within source. |
| `item_canonical_uri` | string | yes | public | Canonical URI for the item/document/page/file, distinct from `source_canonical_uri`. |
| `source_item_hash` | string | yes | public | Hash of normalized source item content or manifest payload. |
| `source_item_size_bytes` | integer | no | public | Normalized item size. |
| `source_item_mtime` | timestamp | no | public | Source-reported modification time. |
| `source_item_status` | string | yes | public | `added`, `modified`, `unchanged`, `removed`, `skipped`, `failed`. |
| `source_item_error` | object | no | internal | Structured last item error. |
| `cleanup_debt_id` | string | no | internal | Cleanup/prune debt row created by generation replacement. |
| `lease_owner` | string | no | internal | Worker/process lease holder. |
| `lease_expires_at` | timestamp | no | internal | Lease expiry. |

### Job and Progress Metadata

These fields appear in job rows, progress JSON, status, logs, traces, and
optionally vector payloads for correlation.

| Field | Type | Required | Visibility | Description |
|---|---|---:|---|---|
| `job_id` | string | yes | public | Single id tying together logs, progress, ledger rows, graph updates, artifacts, and vector payloads. |
| `job_kind` | string | yes | public | `source`, `watch`, `extract`, `ask`, `prune`, `evaluate`, etc. |
| `job_status` | string | yes | public | Shared `LifecycleStatus`: `queued`, `running`, `waiting`, `completed`, `completed_degraded`, `failed`, `canceled`, etc. |
| `job_phase` | string | yes | public | Shared `PipelinePhase`: `resolving`, `discovering`, `fetching`, `normalizing`, `preparing`, `embedding`, `publishing`, `cleaning`, etc. |
| `job_attempt` | integer | yes | public | Attempt number, starting at 1. |
| `trace_id` | string | no | internal | Distributed trace id. |
| `span_id` | string | no | internal | Current span id. |
| `worker_id` | string | no | internal | Worker instance id. |
| `started_at` | timestamp | no | public | Job start time. |
| `completed_at` | timestamp | no | public | Job completion time. |
| `last_heartbeat_at` | timestamp | no | public | Last progress heartbeat. |
| `degraded_reason` | string | no | public | Human-readable degraded-mode reason. |

## Document Metadata

`SourceDocument` is the normalized item emitted by every adapter. Adapters must
emit `SourceDocument`; they must not skip directly to `PreparedDocument`.

| Field | Type | Required | Visibility | Stored In | Description |
|---|---|---:|---|---|---|
| `document_id` | string | yes | public | ledger, status, payload | Stable id for this source item and generation. |
| `source_id` | string | yes | public | all | Stable source id. |
| `source_item_key` | string | yes | public | all | Stable item key within source. |
| `item_canonical_uri` | string | yes | public | all | Canonical item URI, not source root. |
| `document_uri` | string | yes | public | all | URI that directly identifies this document/item. |
| `document_status` | string | yes | public | status | `pending`, `prepared`, `embedded`, `published`, `failed`, `removed`, `pruned`. |
| `document_version` | string | no | public | ledger, payload | Source version, commit, package version, or item version. |
| `content_kind` | string | yes | public | all | `code`, `markdown`, `html`, `plain_text`, `transcript`, `structured`, `binary_metadata`, etc. |
| `content_title` | string | no | public | payload, citations | Title, heading, path, package name, or display label. |
| `content_language` | string | no | public | payload | Programming language or natural language. |
| `content_path` | string | no | public | payload | Repo/local/site-relative path. |
| `content_mime` | string | no | public | payload | MIME/content type if known. |
| `content_hash` | string | yes | public | all | Hash of normalized document content. |
| `content_size_bytes` | integer | yes | public | ledger, status | Normalized content byte length. |
| `raw_content_hash` | string | no | internal | artifact, ledger | Hash before normalization. |
| `normalization_version` | string | yes | public | payload | Version of normalization rules. |
| `parser_family` | string | no | public | payload | Parser family: `tree_sitter`, `markdown`, `html`, `json`, etc. |
| `parser_version` | string | no | public | payload | Parser grammar/tool version. |
| `fetch_status` | string | no | public | ledger, status | `fetched`, `not_modified`, `missing`, `forbidden`, `failed`, `synthetic`. |
| `extraction_status` | string | no | public | payload, status | `not_needed`, `parsed`, `partial`, `fallback`, `failed`. |
| `structured_payload` | object | no | public/redacted | document cache/artifact | Source-specific structured data when small and safe. |
| `artifact_id` | string | no | public | artifact, payload | Artifact holding raw/large/binary content. |
| `created_at` | timestamp | no | public | payload | Source item creation time if known. |
| `updated_at` | timestamp | no | public | payload | Source item update time if known. |
| `fetched_at` | timestamp | no | public | payload | Acquisition/fetch time. |
| `published_at` | timestamp | no | public | status | Generation publish time. |

## PreparedDocument Metadata

`PreparedDocument` is the only shape accepted by the embedding pipeline.

| Field | Type | Required | Visibility | Description |
|---|---|---:|---|---|
| `document_id` | string | yes | public | Same id as `SourceDocument`. |
| `prepared_document_id` | string | yes | internal | Preparation attempt id when retries or variants exist. |
| `prepare_version` | string | yes | public | Document preparation contract version. |
| `chunk_count` | integer | yes | public | Number of chunks emitted. |
| `chunking_method` | string | yes | public | Default method used for the document. |
| `chunking_profile` | string | yes | public | Tuned profile: `code_symbol`, `markdown_sections`, `transcript_turns`, etc. |
| `chunking_fallback` | string | no | public | Fallback reason if ideal parser failed. |
| `cleanup_keys` | string[] | no | internal | Vector/document keys replaced by this prepared document. |
| `graph_refs` | string[] | no | public | Graph nodes/edges created or referenced during preparation. |
| `warnings` | object[] | no | public | Non-fatal preparation warnings. |

## Chunk Metadata

Every chunk must be independently citable and traceable to a source range.

| Field | Type | Required | Visibility | Stored In | Description |
|---|---|---:|---|---|---|
| `chunk_id` | string | yes | public | vector, status | Stable chunk id. |
| `chunk_index` | integer | yes | public | vector, status | Zero-based index within prepared document. |
| `chunk_key` | string | yes | public | vector | Stable key for cleanup and idempotent upsert. |
| `chunk_locator` | string | yes | public | vector, citations | Human-usable locator: URL/path plus line/range/selector. |
| `chunk_title` | string | no | public | vector, citations | Section/function/heading title. |
| `chunk_content_kind` | string | yes | public | vector | Chunk-level content kind. |
| `chunk_hash` | string | yes | public | vector | Hash of normalized chunk text. |
| `chunk_size_bytes` | integer | yes | public | vector | Chunk text byte length. |
| `chunk_token_estimate` | integer | no | public | vector | Token estimate used by retrieval. |
| `source_range` | object | yes | public | vector, citations | Combined byte/line/selector/time range. |
| `line_start` | integer | when text/code | public | vector, citations | One-based start line. |
| `line_end` | integer | when text/code | public | vector, citations | One-based end line, inclusive. |
| `byte_start` | integer | yes | public | vector | Start byte in normalized document content. |
| `byte_end` | integer | yes | public | vector | End byte in normalized document content. |
| `char_start` | integer | no | public | vector | Character offset when needed. |
| `char_end` | integer | no | public | vector | Character end offset. |
| `time_start_ms` | integer | when transcript/media | public | vector, citations | Start timestamp. |
| `time_end_ms` | integer | when transcript/media | public | vector, citations | End timestamp. |
| `dom_selector` | string | when web/html | public | vector, citations | CSS/XPath-ish selector when useful. |
| `parent_chunk_id` | string | no | public | vector | Parent section/module chunk. |
| `sibling_chunk_ids` | string[] | no | internal | document cache | Adjacent chunks for context expansion. |

## Embedding and Vector Payload Metadata

The VectorStore payload must be filterable without requiring joins for common
retrieval paths. It may duplicate a compact subset of source, document, chunk,
generation, graph, and embedding metadata.

### Required Vector Payload Fields

| Field | Type | Required | Filter Index | Description |
|---|---|---:|---:|---|
| `payload_contract_version` | string | yes | no | Payload schema version. |
| `collection` | string | yes | no | Vector collection name. |
| `vector_point_id` | string | yes | no | Store-level point id. |
| `vector_namespace` | string | yes | yes | `documents`, `memory`, `graph`, `artifacts`, etc. |
| `source_id` | string | yes | yes | Source filter. |
| `source_kind` | string | yes | yes | Source kind filter. |
| `source_adapter` | string | yes | yes | Adapter filter/debug. |
| `source_scope` | string | yes | yes | Scope filter. |
| `source_generation` | integer | when mutable | yes | Generation filter. |
| `committed_generation` | integer | when mutable | yes | Committed snapshot filter/correlation. |
| `source_item_key` | string | yes | yes | Item filter and cleanup key. |
| `item_canonical_uri` | string | yes | yes | Exact item/document URI lookup. |
| `document_id` | string | yes | yes | Document filter. |
| `chunk_id` | string | yes | yes | Chunk lookup. |
| `chunk_index` | integer | yes | no | Chunk ordering. |
| `content_kind` | string | yes | yes | Code/docs/transcript/etc filter. |
| `content_title` | string | no | no | Citation display. |
| `content_path` | string | no | yes | Path/prefix filter. |
| `content_language` | string | no | yes | Language filter. |
| `content_hash` | string | yes | no | Drift/dedupe evidence. |
| `chunk_hash` | string | yes | no | Dedupe evidence. |
| `chunk_locator` | string | yes | no | Citation locator. |
| `source_range` | object | yes | no | Citation range. |
| `job_id` | string | yes | yes | Correlation. |
| `document_status` | string | yes | yes | Must be searchable only when publish-safe. |
| `embedding_model` | string | yes | yes | Model/debug. |
| `embedding_dimensions` | integer | yes | no | Dense vector dimensions. |
| `embedding_provider` | string | yes | yes | TEI/OpenAI/etc provider. |
| `embedding_profile` | string | yes | yes | Query/document instruction profile. |
| `embedded_at` | timestamp | yes | no | Embed completion time. |

### Optional Vector Payload Fields

| Field | Type | Use |
|---|---|---|
| `graph_node_ids` | string[] | Graph-aware retrieval and explanations. |
| `graph_edge_ids` | string[] | Graph-aware retrieval and explanations. |
| `graph_confidence` | number | Ranking/answer confidence hints. |
| `authority_score` | number | Ranking boost for official/pinned sources. |
| `freshness_score` | number | Recency ranking hint. |
| `quality_score` | number | Adapter/preparer quality hint. |
| `dedupe_key` | string | Cross-generation/content duplicate grouping. |
| `artifact_id` | string | Link to raw/large source artifact. |
| `redaction_profile` | string | Redaction policy applied before storage. |
| `visibility` | string | Shared `Visibility`: `public`, `internal`, `sensitive`, `redacted`, `derived`. |
| `tenant_id` | string | Future multi-tenant boundary; optional for local single-user runtime. |

### Vector Payload Example

```json
{
  "payload_contract_version": "2026-06-30",
  "vector_namespace": "documents",
  "source_id": "src_01J...",
  "source_kind": "git",
  "source_adapter": "github",
  "source_scope": "repo",
  "source_generation": 12,
  "committed_generation": 12,
  "source_item_key": "crates/axon-cli/src/main.rs",
  "item_canonical_uri": "github://jmagar/axon?rev=abc123#crates/axon-cli/src/main.rs",
  "document_id": "doc_01J...",
  "chunk_id": "chk_01J...",
  "content_kind": "code",
  "content_path": "crates/axon-cli/src/main.rs",
  "content_language": "rust",
  "chunk_locator": "crates/axon-cli/src/main.rs:42-96",
  "source_range": { "line_start": 42, "line_end": 96, "byte_start": 1204, "byte_end": 3390 },
  "job_id": "job_01J...",
  "document_status": "published",
  "embedding_provider": "tei",
  "embedding_model": "Qwen3-Embedding-0.6B",
  "embedding_dimensions": 1024,
  "embedding_profile": "document_code",
  "embedded_at": "2026-06-30T20:20:00Z",
  "code_file_type": "source",
  "symbol_name": "run",
  "symbol_kind": "function",
  "graph_node_ids": ["graph_node_01J..."]
}
```

## Graph Metadata

Graph metadata appears on graph rows, graph evidence, graph-aware vector
payloads, and status events when a document or chunk produced graph facts.

| Field | Type | Required | Description |
|---|---|---:|---|
| `graph_node_id` | string | when node | Node id produced or referenced. |
| `graph_node_kind` | string | when node | `source`, `repo`, `package`, `file`, `symbol`, `tool`, `session`, etc. |
| `graph_edge_id` | string | when edge | Edge id produced or referenced. |
| `graph_edge_kind` | string | when edge | `depends_on`, `documented_by`, `invoked`, `mentions`, etc. |
| `graph_evidence_id` | string | yes | Evidence row id. |
| `graph_evidence_kind` | string | yes | `manifest`, `schema`, `source_code`, `session`, `crawler`, `tool_output`, etc. |
| `graph_evidence_locator` | string | yes | URL, file path, line range, selector, transcript turn, or artifact locator. |
| `graph_authority` | string | yes | `official`, `user_pinned`, `inferred`, `community`, `mirror`, `unknown`. |
| `graph_confidence` | number | yes | 0.0 to 1.0 confidence score. |
| `graph_merge_key` | string | no | Stable merge key for equivalent nodes. |
| `graph_extraction_method` | string | yes | Parser/extractor method. |
| `graph_extractor_version` | string | yes | Extractor version. |

Graph fields should be compact in VectorStore payloads. Large evidence bodies
belong in ArtifactStore or DocumentCache.

## Artifact Metadata

Artifacts store large, binary, raw, or auxiliary outputs. Chunks should point to
artifacts instead of embedding unbounded raw payloads.

| Field | Type | Required | Description |
|---|---|---:|---|
| `artifact_id` | string | yes | Durable artifact id. |
| `artifact_kind` | string | yes | `raw_html`, `screenshot`, `warc`, `repomix`, `tool_output`, `manifest`, `network_capture`, etc. |
| `artifact_uri` | string | no | Internal artifact URI/path. |
| `artifact_content_type` | string | yes | MIME/content type. |
| `artifact_size_bytes` | integer | yes | Artifact byte length. |
| `artifact_hash` | string | yes | Content hash. |
| `artifact_redaction_profile` | string | no | Redaction applied. |
| `artifact_retention` | string | yes | `keep`, `cache`, `temporary`, `prunable`. |
| `artifact_created_at` | timestamp | yes | Creation time. |
| `artifact_producer` | string | yes | Adapter/provider/job component that produced it. |

## Error and Warning Metadata

Errors and warnings should be structured consistently across ledger, jobs,
status, and public responses.

| Field | Type | Required | Description |
|---|---|---:|---|
| `error_code` | string | yes | Stable machine code. |
| `error_stage` | string | yes | `resolving`, `acquiring`, `diffing`, `preparing`, `embedding`, `publishing`, `cleaning`, etc. |
| `error_message` | string | yes | Human-readable message, redacted. |
| `error_retryable` | bool | yes | Whether retry may succeed. |
| `error_severity` | string | yes | `info`, `warning`, `degraded`, `failed`, `fatal`. |
| `error_source_item_key` | string | no | Item associated with the error. |
| `error_provider` | string | no | Provider/adapter that failed. |
| `error_attempt` | integer | no | Attempt number. |
| `error_details` | object | no | Redacted structured details. |

## Source-Specific Metadata

### Web and Crawl Fields

| Field | Type | Required | Description |
|---|---|---:|---|
| `web_url` | string | yes | Final fetched URL. |
| `web_seed_url` | string | yes | Crawl/source seed URL. |
| `web_domain` | string | yes | Normalized domain. |
| `web_origin` | string | yes | Scheme + host + port. |
| `web_path` | string | no | URL path. |
| `web_canonical_url` | string | no | HTML/link canonical URL. |
| `web_normalized_url` | string | yes | URL after normalization contract. |
| `web_status_code` | integer | no | HTTP status. |
| `web_fetch_method` | string | yes | `http`, `chrome`, `cache`, `warc`, `synthetic`. |
| `web_render_mode` | string | no | `http`, `chrome`, `auto_switch`. |
| `web_depth` | integer | no | Crawl depth from seed. |
| `web_referrer_url` | string | no | Parent URL. |
| `web_etag` | string | no | ETag. |
| `web_last_modified` | string | no | Last-Modified header. |
| `web_content_type` | string | no | Response content type. |
| `web_robots_allowed` | bool | no | Robots decision if checked. |
| `web_sitemap_url` | string | no | Sitemap source. |
| `web_sitemap_lastmod` | string | no | Sitemap `<lastmod>` when available. |
| `web_thin_content` | bool | no | Whether page was considered thin. |
| `web_cache_status` | string | no | `hit`, `miss`, `not_modified`, `bypass`, or `disabled`. |
| `web_warc_artifact_id` | string | no | WARC artifact containing the captured response. |
| `web_automation_artifact_id` | string | no | Automation script/result artifact used for capture. |
| `web_vertical_extractor` | string | no | Vertical extractor name when a page used one. |
| `web_vertical_extractor_version` | string | no | Vertical extractor version. |
| `web_chrome_fallback_reason` | string | no | Why HTTP switched to Chrome in auto-switch mode. |
| `web_redirect_chain` | string[] | no | Redacted redirect chain. |

### Local File and Workspace Fields

| Field | Type | Required | Description |
|---|---|---:|---|
| `local_project_key` | string | when local | Stable anonymous local project/worktree key. |
| `local_root_label` | string | when local | Safe display label for root. |
| `local_relative_path` | string | when local file | Path relative to source root. |
| `local_path_hash` | string | when local | Hash of absolute path for stable private correlation. |
| `local_file_mode` | string | no | File mode/permissions if relevant. |
| `local_file_size_bytes` | integer | no | Source file size. |
| `local_file_mtime` | timestamp | no | Filesystem mtime. |
| `local_symlink_target_hash` | string | no | Hash of symlink target, not raw target unless safe. |
| `local_ignore_reason` | string | no | Gitignore/source ignore reason. |

Absolute paths should not be public payload fields by default. Use
`local_project_key`, `local_root_label`, and `local_relative_path`.

### Git and Repo Fields

| Field | Type | Required | Description |
|---|---|---:|---|
| `git_provider` | string | yes | `github`, `gitlab`, `gitea`, `git`. |
| `git_host` | string | yes | Hostname. |
| `git_owner` | string | no | Owner/org/namespace. |
| `git_repo` | string | no | Repository name. |
| `git_repo_slug` | string | when hosted | `owner/repo` or namespace path. |
| `git_remote_url` | string | no | Redacted remote URL. |
| `git_default_branch` | string | no | Default branch. |
| `git_branch` | string | no | Branch used. |
| `git_tag` | string | no | Tag used. |
| `git_ref` | string | yes | Branch/tag/SHA input or resolved ref. |
| `git_commit` | string | when available | Commit SHA. |
| `git_tree_hash` | string | no | Tree hash/manifest hash. |
| `git_is_private` | bool | no | Repository visibility if known. |
| `git_license` | string | no | Repo license. |
| `git_topics` | string[] | no | Repo topics/tags. |

### Code Fields

| Field | Type | Required | Description |
|---|---|---:|---|
| `code_file_path` | string | yes for code | Repo/local relative path. |
| `code_language` | string | yes for code | Language inferred from parser/extension. |
| `code_file_type` | string | yes for code | `source`, `test`, `config`, `docs`, `generated`, `lockfile`, `schema`, etc. |
| `code_is_test` | bool | yes for code | Test path indicator. |
| `code_is_generated` | bool | no | Generated/vendor indicator. |
| `code_parser` | string | no | Parser used. |
| `code_parser_version` | string | no | Parser/grammar version. |
| `code_parse_status` | string | yes for code | `parsed`, `partial`, `fallback`, `unsupported`, `failed`. |
| `code_chunk_source` | string | yes for code chunks | `ast_symbol`, `ast_node`, `markdown_fence`, `line_window`, etc. |
| `symbol_name` | string | no | Extracted symbol name. |
| `symbol_kind` | string | no | `function`, `method`, `struct`, `class`, `module`, `trait`, etc. |
| `symbol_qualified_name` | string | no | Fully qualified name when available. |
| `symbol_signature` | string | no | Redacted/normalized signature. |
| `symbol_visibility` | string | no | `public`, `private`, `protected`, etc. |
| `symbol_parent` | string | no | Parent symbol/module. |
| `symbol_extraction_status` | string | yes for code | `parsed`, `fallback`, `unsupported`, `failed`, `none`. |
| `dependency_manifest_kind` | string | no | `cargo_toml`, `package_json`, `requirements_txt`, etc. |
| `schema_kind` | string | no | `openapi`, `graphql`, `json_schema`, `protobuf`, etc. |

### Dependency and Manifest Fields

These fields are emitted by parsers for files such as `Cargo.toml`,
`package.json`, `pyproject.toml`, `requirements.txt`, `go.mod`,
`docker-compose.yaml`, `.env.example`, OpenAPI specs, GraphQL schemas, and
similar manifests.

| Field | Type | Required | Description |
|---|---|---:|---|
| `manifest_kind` | string | yes for manifest docs | Manifest/parser kind. |
| `manifest_name` | string | no | Project/package/service name from manifest. |
| `manifest_version` | string | no | Version from manifest. |
| `manifest_environment` | string | no | `dev`, `test`, `prod`, `example`, etc. |
| `dependency_name` | string | per dependency chunk/fact | Dependency/package/service name. |
| `dependency_version_req` | string | no | Version requirement. |
| `dependency_group` | string | no | `dependencies`, `devDependencies`, `build`, `services`, etc. |
| `dependency_registry` | string | no | Registry/ecosystem. |
| `dependency_optional` | bool | no | Optional dependency flag. |
| `dependency_source_uri` | string | no | Git/path/registry source. |
| `service_name` | string | for compose/service docs | Compose/service name. |
| `service_image` | string | no | Container image. |
| `service_ports` | string[] | no | Redacted port mappings. |
| `service_env_keys` | string[] | no | Env var keys only; never raw secret values. |
| `api_schema_kind` | string | for API schemas | `openapi`, `graphql`, `grpc`, `asyncapi`, etc. |
| `api_endpoint` | string | per endpoint chunk/fact | Method/path or operation identifier. |

### Registry and Package Fields

| Field | Type | Required | Description |
|---|---|---:|---|
| `package_registry` | string | yes | `crates`, `npm`, `pypi`, `docker`, etc. |
| `package_name` | string | yes | Package/image/module name. |
| `package_version` | string | no | Version/tag. |
| `package_namespace` | string | no | Scope/org/group/namespace. |
| `package_owner` | string | no | Registry owner/maintainer if known. |
| `package_license` | string | no | License metadata. |
| `package_description` | string | no | Short description. |
| `package_repo_url` | string | no | Linked repo. |
| `package_docs_url` | string | no | Linked docs. |
| `package_homepage_url` | string | no | Homepage. |
| `package_downloads` | integer | no | Registry download/popularity count if fetched. |
| `package_published_at` | timestamp | no | Version publish time. |
| `package_yanked` | bool | no | Yank/deprecation flag. |

### Reddit, Feed, and Social Fields

| Field | Type | Required | Description |
|---|---|---:|---|
| `social_provider` | string | yes | `reddit`, `feed`, etc. |
| `social_container` | string | no | Subreddit/feed/channel/list. |
| `social_thread_id` | string | no | Thread/post id. |
| `social_item_id` | string | no | Entry/comment/item id. |
| `social_author` | string | no | Redacted/normalized author when allowed. |
| `social_score` | integer | no | Score/upvotes/likes when available. |
| `social_comment_count` | integer | no | Comment/reply count. |
| `social_permalink` | string | no | Canonical item permalink. |
| `feed_url` | string | no | Feed URL. |
| `feed_entry_id` | string | no | Feed entry id/guid. |

### YouTube and Transcript Fields

| Field | Type | Required | Description |
|---|---|---:|---|
| `transcript_provider` | string | yes | `youtube`, `claude`, `codex`, `gemini`, etc. |
| `transcript_id` | string | yes | Video/session/transcript id. |
| `transcript_title` | string | no | Video/session title. |
| `transcript_author` | string | no | Channel/user/agent if known. |
| `transcript_language` | string | no | Transcript language. |
| `transcript_duration_ms` | integer | no | Total media/session duration. |
| `transcript_segment_id` | string | no | Segment/turn id. |
| `time_start_ms` | integer | no | Segment start timestamp. |
| `time_end_ms` | integer | no | Segment end timestamp. |

### Session Fields

| Field | Type | Required | Description |
|---|---|---:|---|
| `session_provider` | string | yes | `claude`, `codex`, `gemini`, etc. |
| `session_id` | string | yes | Stable session id. |
| `session_project` | string | no | Project/repo context. |
| `session_file_path` | string | no | Safe normalized source path. |
| `session_turn_id` | string | no | Turn/message id. |
| `session_turn_index` | integer | no | Turn index. |
| `session_role` | string | no | `user`, `assistant`, `tool`, `system`. |
| `session_model` | string | no | Model name if known. |
| `session_skill_names` | string[] | no | Skills invoked during turn/job. |
| `session_agent_ids` | string[] | no | Agents/subagents involved. |
| `session_tool_call_ids` | string[] | no | Tool calls referenced by chunk/turn. |

### CLI Tool and Script Fields

CLI tools and scripts are first-class sources. Execution output is metadata-rich
and must be safety-scoped.

| Field | Type | Required | Description |
|---|---|---:|---|
| `tool_kind` | string | yes | `cli`, `script`, `mcp`, `session_tool`. |
| `tool_name` | string | yes | Binary/script/tool name. |
| `tool_version` | string | no | Reported version. |
| `tool_command` | string | no | Redacted command label, not necessarily full argv. |
| `tool_arg_fingerprint` | string | no | Hash of argv/options. |
| `tool_working_directory_key` | string | no | Safe key for working directory. |
| `tool_exit_code` | integer | when executed | Process exit code. |
| `tool_status` | string | yes | `discovered`, `schema`, `executed`, `failed`, `skipped`. |
| `tool_side_effect_class` | string | yes | `read_only`, `local_mutation`, `network_read`, `network_mutation`, `filesystem_mutation`, `unknown`. |
| `tool_allowlist_policy` | string | when executed | Policy id that allowed execution. |
| `tool_stdout_hash` | string | no | Hash of stdout after redaction. |
| `tool_stderr_hash` | string | no | Hash of stderr after redaction. |
| `tool_output_artifact_id` | string | no | Artifact holding full output. |
| `tool_schema_artifact_id` | string | no | Artifact holding schema/help. |

### MCP Server and Tool Fields

MCP server/tool calls are source acquisitions when Axon intentionally calls or
introspects them for indexing.

| Field | Type | Required | Description |
|---|---|---:|---|
| `mcp_server_id` | string | yes | Stable server id. |
| `mcp_server_name` | string | yes | Server display/name. |
| `mcp_transport` | string | no | `stdio`, `http`, `sse`, etc. |
| `mcp_tool_name` | string | when tool | Tool name. |
| `mcp_resource_uri` | string | when resource | Resource URI. |
| `mcp_prompt_name` | string | when prompt | Prompt name. |
| `mcp_schema_hash` | string | no | Hash of tool/resource/prompt schema. |
| `mcp_call_id` | string | when call | Concrete call id. |
| `mcp_call_status` | string | when call | `succeeded`, `failed`, `partial`, `canceled`. |
| `mcp_response_artifact_id` | string | no | Artifact holding large response. |
| `mcp_client_provider` | string | no | Client implementation, e.g. `mcporter`; informational only. |

Graph evidence must describe the MCP server/tool/call/result. A helper such as
`mcporter` is implementation detail unless the helper itself is the indexed
source.

### Extract/LLM Enrichment Fields

| Field | Type | Required | Description |
|---|---|---:|---|
| `llm_provider` | string | when LLM used | Provider boundary used. |
| `llm_model` | string | when LLM used | Model id. |
| `llm_task` | string | when LLM used | `extract`, `summarize`, `classify`, `enrich`, `judge`. |
| `llm_prompt_hash` | string | no | Hash of prompt/template. |
| `llm_response_hash` | string | no | Hash of response. |
| `llm_output_schema` | string | no | Schema name/version. |
| `llm_confidence` | number | no | Model/extractor confidence if available. |
| `llm_artifact_id` | string | no | Artifact for full prompt/response when retained. |

### Memory Fields

Memory is not a source adapter, but memory records can use the same embedding,
graph, status, and payload rules.

| Field | Type | Required | Description |
|---|---|---:|---|
| `memory_id` | string | yes | Durable memory id. |
| `memory_type` | string | yes | `decision`, `fact`, `preference`, `task`, `bug`, `procedure`, `incident`, `entity`, `episode`, `working`. |
| `memory_status` | string | yes | `active`, `review`, `superseded`, `archived`, `forgotten`. |
| `memory_scope_kind` | string | yes | `global`, `project`, `repo`, `file`, `source_id`, `graph_node_id`, `agent`, `user`, `environment`. |
| `memory_scope_value` | string | yes | Scope-specific id or canonical value. |
| `memory_confidence` | number | yes | Truth/confidence score. |
| `memory_salience` | number | yes | Importance/usefulness score. |
| `memory_decay_mode` | string | yes | `none`, `time`, `access`, `confidence`, `supersession`, `custom`. |
| `memory_decay_score` | number | yes | Current decay-adjusted score. |
| `memory_recency_score` | number | no | Recency component used during recall. |
| `memory_pinned` | bool | yes | Whether decay/ranking is pinned above a floor. |
| `memory_review_required` | bool | yes | Whether high-confidence recall must exclude or flag it. |
| `memory_supersedes` | string[] | no | Memory ids superseded by this memory. |
| `memory_contradicts` | string[] | no | Conflicting memory ids. |
| `memory_graph_node_id` | string | no | Mirrored SourceGraph node id. |

## Required Field Sets by Store

### SourceLedger

Must store:

- `source_id`, `source_kind`, `source_adapter`, `source_adapter_version`
- `source_canonical_uri`, `item_canonical_uri`, `source_scope`, `source_fingerprint`
- `source_generation`, `committed_generation`, `generation_status`, `generation_publish_state`
- `source_item_key`, `source_item_hash`, `source_item_status`
- item size, mtime, fetch status, error summary
- `job_id`, lease fields, timestamps
- cleanup debt refs

### DocumentStatus

Must store:

- `document_id`, `source_id`, `source_item_key`, `source_generation`
- `document_status`, `content_kind`, `content_hash`
- chunk count, vector point count, graph refs
- current error/warnings
- publish and cleanup status

### VectorStore

Must store the required vector payload fields listed above. It should not be the
only owner of source state, cleanup state, or generation state.

### SourceGraph

Must store:

- graph node/edge/evidence ids and kinds
- source/document/chunk/job provenance
- authority, confidence, extraction method/version
- merge keys and evidence locators

### ArtifactStore

Must store artifact metadata, content hashes, retention policy, producer refs,
and redaction profile. Artifact bodies may live on disk/object storage.

## Filter and Index Guidance

VectorStore payload indexes should favor high-value filters:

- `source_id`
- `source_kind`
- `source_scope`
- `source_adapter`
- `source_generation`
- `committed_generation`
- `source_item_key`
- `item_canonical_uri`
- `document_id`
- `content_kind`
- `content_path`
- `content_language`
- `document_status`
- `job_id`
- `embedding_model`
- `embedding_provider`
- `vector_namespace`
- source-specific: `git_repo_slug`, `git_commit`, `web_domain`,
  `package_registry`, `package_name`, `local_project_key`, `session_id`,
  `tool_name`, `mcp_server_id`

Do not index high-cardinality or low-filter-value fields unless a real query
path needs them:

- hashes except for exact dedupe workflows
- large arrays
- raw titles
- raw headers
- arbitrary structured payload blobs
- stack traces
- long error messages

## Redaction and Normalization Rules

- Redaction happens before VectorStore, public logs, public progress events, and
  public API responses.
- Raw source artifacts may be retained only when their artifact retention and
  visibility allow it.
- Header names may be stored; header values are redacted unless explicitly safe.
- Environment variable keys may be stored; values are redacted by default.
- Local absolute paths become `local_path_hash`, `local_project_key`, and
  relative paths.
- Tool commands should store a redacted command label and argument fingerprint,
  not a secret-bearing raw argv string.
- URL query parameters follow the URL normalization contract; known tracking
  parameters are dropped and sensitive parameters are redacted.
- LLM prompts/responses are artifacts only when retention is enabled and
  redaction has run.

## Validation Rules

Implementation must reject or degrade documents before embedding when:

- `source_id`, `source_item_key`, `document_id`, `content_kind`, or
  `content_hash` is missing
- a chunk lacks `chunk_id`, `chunk_index`, `chunk_locator`, `chunk_hash`, or
  source range
- mutable sources lack a generation
- vector payload lacks `job_id`
- source-specific required fields are missing for that source kind
- a public payload contains a field classified as sensitive
- an adapter emits `PreparedDocument` without first emitting `SourceDocument`

Implementation may degrade, warn, and continue when:

- graph extraction fails
- symbol extraction falls back to line windows
- a package registry omits optional metadata
- a page has no canonical link
- a transcript lacks timestamps
- an MCP tool schema is partial but the call/result is usable

## Promotion Rule

If two adapters emit the same concept under different names, promote the concept
to this contract and update both adapters to the shared name. Do not preserve
backwards-compatible aliases in the clean-break implementation.
