# Chunking and Document Preparation Contract
Last Modified: 2026-06-30

## Contract

This is the target document-preparation contract. Current chunking is real and
shared, but it lives inside `axon-vector` rather than a separate document crate.

Adapters emit `SourceDocument`. They never emit `PreparedDocument` directly and
they never write vector points directly.

```text
SourceAdapter
  -> SourceDocument
  -> DocumentPreparer
  -> ChunkRouter
  -> Chunker / Parser
  -> SourceParseFacts
  -> GraphCandidate
  -> PreparedDocument
  -> EmbeddingBatch
```

The document preparation layer owns chunk routing, chunk metadata, deterministic
chunk ids, cleanup keys, and `PreparedDocument` shape.

Parsers may emit `SourceParseFacts` and `GraphCandidate` values while preparing
documents, but chunkers do not persist graph facts. `SourceGraph` persistence
remains owned by the graph pipeline.

## Design Rules

- One preparation path exists for all sources.
- Source-specific chunking is selected by `ChunkRouter`, not by separate ingest
  pipelines.
- Every adapter emits `SourceDocument`.
- Every embedding path consumes `PreparedDocument`.
- Every chunk is independently citable.
- Every chunk has deterministic identity, content hash, locator, and source
  range.
- Every fallback is explicit in metadata.
- Code chunking should be AST/symbol-centric when supported.
- Markdown/docs chunking should preserve section hierarchy.
- Structured files should produce searchable chunks and parse facts.
- Large raw/binary outputs use `ArtifactStore`, not giant vector payloads.
- Chunking must not require Qdrant access.
- Chunking must not mutate SourceLedger generation state.

## Current Implementation Snapshot

Implemented today:

- `axon-vector::ops::SourceDocument` normalizes content for vector preparation.
  Current fields include `url`, `domain`, `text`, `source_type`, `title`,
  `extra`, `structured`, and an internal chunk hint.
- `prepare_source_document` routes file, markdown/plain, plain text, and atomic
  memory content into `PreparedDoc`.
- Markdown/plain chunking uses `text_splitter::MarkdownSplitter`, heading
  breadcrumbs, byte offsets, and source ranges.
- Code chunking is AST-aware through tree-sitter when supported and falls back
  to prose chunking for unsupported languages, oversized files, or zero-symbol
  extraction.
- Current per-chunk metadata can include `chunk_content_kind`, `chunk_locator`,
  `source_range`, file line fields, `code_chunking_method`, `symbol_name`,
  `symbol_kind`, `code_file_path`, `code_language`, `code_file_type`, and
  `symbol_extraction_status`.
- Current point IDs default to UUIDv5 over `url:idx`; memory can pass stable
  chunk point IDs.

Planned by this contract:

- `DocumentPreparer`, `ChunkRouter`, `Parser`, `PreparedDocument`, parse facts,
  and graph candidates become explicit shared boundaries.
- Every source adapter emits the target `SourceDocument` shape, and no adapter
  emits `PreparedDocument` directly.
- Deterministic chunk ids, content hashes, cleanup keys, graph candidates, and
  source ledger metadata become required for all source families.

## Core Types

### SourceDocument

`SourceDocument` is normalized acquisition output.

Required fields:

| Field | Type | Meaning |
|---|---|---|
| `document_id` | string | Stable document id for source item/generation. |
| `source_id` | string | Stable source id. |
| `source_item_key` | string | Stable item key within source. |
| `canonical_uri` | string | Canonical item URI. |
| `content_kind` | enum | `code`, `markdown`, `html`, `plain_text`, `transcript`, `structured`, `binary_metadata`, etc. |
| `content` | string or bytes ref | Normalized content or artifact/document-cache ref. |
| `metadata` | object | Shared metadata envelope from `metadata-payload.md`. |

Optional fields:

| Field | Type | Meaning |
|---|---|---|
| `title` | string | Display/citation title. |
| `language` | string | Programming or natural language. |
| `path` | string | Repo/local/site-relative path. |
| `mime_type` | string | Content MIME type. |
| `structured_payload` | object | Small bounded structured data. |
| `artifact_id` | string | Raw/large content artifact. |
| `chunk_hint` | object | Adapter hint to `ChunkRouter`. |
| `parser_hint` | object | Adapter hint for parser family. |

Adapters may suggest hints, but `DocumentPreparer` makes the final route
decision.

### ChunkHint

`chunk_hint` is advisory. It must not be required for correctness.

| Field | Type | Meaning |
|---|---|---|
| `profile` | string | Preferred chunking profile. |
| `content_kind` | string | More specific content kind. |
| `language` | string | Language/parser hint. |
| `file_type` | string | `source`, `test`, `config`, `schema`, `lockfile`, etc. |
| `max_chunk_tokens` | integer | Suggested maximum token budget. |
| `min_chunk_tokens` | integer | Suggested minimum useful chunk size. |
| `overlap_tokens` | integer | Suggested overlap when windowing is used. |
| `preserve_boundaries` | string[] | Boundaries to respect: symbols, headings, rows, turns, records. |
| `atomic` | bool | Whether document should become one chunk if size allows. |
| `graph_fact_mode` | string | `none`, `facts_only`, `facts_and_chunks`. |

### PreparedDocument

`PreparedDocument` is the only shape accepted by the embedding pipeline.

Required fields:

| Field | Type | Meaning |
|---|---|---|
| `document_id` | string | Same as `SourceDocument.document_id`. |
| `source_id` | string | Source id. |
| `source_item_key` | string | Source item key. |
| `canonical_uri` | string | Canonical item URI. |
| `prepare_version` | string | Preparation contract version. |
| `chunking_profile` | string | Selected profile. |
| `chunking_method` | string | Concrete method. |
| `chunks` | `PreparedChunk[]` | Chunks in source order. |
| `metadata` | object | Shared document metadata. |
| `cleanup_keys` | string[] | Keys this document supersedes. |

Optional fields:

| Field | Type | Meaning |
|---|---|---|
| `graph_refs` | string[] | Graph nodes/edges referenced/produced. |
| `parse_facts` | `SourceParseFacts[]` | Parser facts emitted during preparation. |
| `graph_candidates` | `GraphCandidate[]` | Candidate graph writes. |
| `warnings` | object[] | Non-fatal preparation warnings. |
| `errors` | object[] | Recoverable item-level errors. |

### PreparedChunk

Every chunk must be independently retrievable, citable, and embeddable.

Required fields:

| Field | Type | Meaning |
|---|---|---|
| `chunk_id` | string | Deterministic chunk id. |
| `chunk_key` | string | Deterministic cleanup/upsert key. |
| `chunk_index` | integer | Zero-based source-order index. |
| `content` | string | Normalized chunk text to embed. |
| `content_hash` | string | Hash of chunk content. |
| `chunk_locator` | string | Human-readable citation locator. |
| `source_range` | object | Byte/line/time/selector source range. |
| `metadata` | object | Chunk metadata payload. |

Optional fields:

| Field | Type | Meaning |
|---|---|---|
| `title` | string | Heading/symbol/record title. |
| `summary` | string | Extractive summary for huge structured chunks. |
| `parent_chunk_id` | string | Parent section/module chunk. |
| `neighbor_chunk_ids` | string[] | Adjacent chunks for context expansion. |
| `graph_refs` | string[] | Graph refs relevant to the chunk. |
| `embedding_text` | string | Alternate text used for embedding when display content differs. |

## Chunk Identity

Chunk identity must be stable across repeated runs when content and source
range are unchanged.

Recommended deterministic key:

```text
chunk_key = hash(
  source_id,
  source_generation or immutable source version,
  source_item_key,
  chunking_profile,
  chunk_locator,
  content_hash
)
```

`chunk_id` may be a prefixed stable id derived from `chunk_key`.

Rules:

- Do not use array index alone as identity.
- Do not use Qdrant-assigned ids.
- Do not include wall-clock timestamps.
- Include generation or immutable source version so old/new mutable snapshots can
  coexist until publish/cleanup completes.
- `cleanup_keys` must let VectorStore remove superseded chunks without scanning
  the full collection.

## ChunkRouter

`ChunkRouter` selects a profile from `SourceDocument` fields and adapter hints.

Inputs:

- `content_kind`
- MIME type
- file extension/path
- source kind and adapter
- source scope
- `chunk_hint`
- `parser_hint`
- structured payload type
- document size
- source metadata such as language, schema kind, transcript provider

Output:

```json
{
  "chunking_profile": "code_symbol",
  "chunking_method": "tree_sitter",
  "parser_family": "tree_sitter",
  "fallback_chain": ["tree_sitter", "code_blocks", "line_window"],
  "limits": {
    "max_chunk_tokens": 900,
    "overlap_tokens": 80
  }
}
```

Routing order:

1. Explicit trusted `chunk_hint.profile`.
2. Strong content kind: code, markdown, transcript, structured, html.
3. File path/extension and MIME type.
4. Source adapter defaults.
5. Size-based fallback.
6. Plain text fallback.

## Chunking Profiles

| Profile | Primary Inputs | Method | Boundary Preference | Typical Output |
|---|---|---|---|---|
| `code_symbol` | source code | tree-sitter / language parser | symbol declarations, classes, modules | symbol/function chunks |
| `code_manifest` | dependency/config manifests | structured parser | manifest sections/records | dependency/service/API chunks + facts |
| `markdown_sections` | Markdown/docs | markdown parser | heading hierarchy | section chunks |
| `html_article` | web HTML converted to text/markdown | DOM/readability + markdown | headings, article sections | page sections |
| `plain_text_windows` | plain text | sentence/paragraph windows | paragraph/sentence | text chunks |
| `transcript_segments` | media/session transcripts | transcript parser | turns/timestamps/speakers | time/turn chunks |
| `structured_records` | JSON/YAML/TOML/CSV/XML | structured parser | records/objects/items | record chunks |
| `api_schema` | OpenAPI/GraphQL/protobuf/etc. | schema parser | operations/types/messages | endpoint/type chunks |
| `tool_output` | CLI/MCP tool output | output parser | records/sections/errors | result chunks |
| `session_turns` | Claude/Codex/Gemini sessions | session parser | turns/tool calls/decisions | turn/tool chunks |
| `atomic_metadata` | small metadata docs | no split | whole document | one chunk |

## Size and Overlap Rules

Default size policy:

| Content Kind | Target Tokens | Max Tokens | Overlap |
|---|---:|---:|---:|
| code | 500-900 | 1400 | 50-120 |
| markdown/docs | 700-1100 | 1600 | 80-160 |
| plain text | 700-1000 | 1500 | 100-180 |
| transcript | 500-900 | 1300 | 0-120 depending on speaker/time boundaries |
| structured records | record-bounded | 1200 | 0-80 |
| API schemas | operation/type-bounded | 1600 | 0-120 |
| tool output | section/record-bounded | 1200 | 0-120 |

Rules:

- Prefer semantic boundaries over exact token targets.
- Never split inside a code symbol when it fits under max size.
- Split huge symbols using AST child nodes or line windows with explicit
  fallback metadata.
- Keep overlap small for structured records to avoid duplicate facts.
- Use no overlap for atomic records unless context loss is proven.
- Preserve order with `chunk_index`.

## Source Range Contract

Every chunk must include a `source_range`.

Text/code:

```json
{
  "line_start": 42,
  "line_end": 96,
  "byte_start": 1204,
  "byte_end": 3390
}
```

HTML/web:

```json
{
  "line_start": 10,
  "line_end": 40,
  "byte_start": 1000,
  "byte_end": 4200,
  "dom_selector": "main article h2:nth-of-type(3)"
}
```

Transcript:

```json
{
  "time_start_ms": 120000,
  "time_end_ms": 165000,
  "turn_start": 14,
  "turn_end": 19
}
```

Structured record:

```json
{
  "json_pointer": "/paths/~1v1~1sources/post",
  "byte_start": 8200,
  "byte_end": 9400
}
```

The range must describe where the chunk came from in the normalized source
document. If raw and normalized ranges differ, store normalized range in chunk
metadata and raw range in parse facts or artifact metadata.

## Code Chunking

Code chunking should be AST/symbol-centric for supported languages.

Required behavior:

- Detect language from parser support, extension, shebang, and adapter metadata.
- Parse with tree-sitter or equivalent parser when supported.
- Extract declarations and stable symbol names when available.
- Prefer chunks around named definitions: functions, methods, classes, structs,
  enums, traits/interfaces, modules, constants, tests.
- Include imports/package declarations with the nearest relevant chunk or as a
  small file prelude chunk when useful.
- Mark generated/vendor/minified files.
- Emit manifest/dependency facts for manifest files.
- Fallback to line-window chunks when parsing fails.

Code chunk metadata:

| Field | Required | Meaning |
|---|---:|---|
| `code_file_path` | yes | Repo/local relative path. |
| `code_language` | yes | Language. |
| `code_file_type` | yes | `source`, `test`, `config`, `docs`, `generated`, `schema`, etc. |
| `code_is_test` | yes | Test indicator. |
| `code_is_generated` | no | Generated indicator. |
| `code_parser` | no | Parser name. |
| `code_parser_version` | no | Parser/grammar version. |
| `code_parse_status` | yes | `parsed`, `partial`, `fallback`, `unsupported`, `failed`. |
| `code_chunk_source` | yes | `ast_symbol`, `ast_node`, `line_window`, etc. |
| `symbol_name` | no | Extracted symbol. |
| `symbol_kind` | no | Function/class/type/etc. |
| `symbol_qualified_name` | no | Fully qualified name. |
| `symbol_signature` | no | Normalized signature. |
| `symbol_parent` | no | Parent/module symbol. |
| `symbol_extraction_status` | yes | `parsed`, `fallback`, `unsupported`, `failed`, `none`. |

Supported language contract:

- A language is "supported" only when parser coverage, symbol extraction rules,
  and fallback tests exist.
- Unsupported languages still produce line-aware chunks.
- `symbol_extraction_status=unsupported` is valid and searchable.

Initial language/parser targets:

| Ecosystem | Files | Chunking |
|---|---|---|
| Rust | `.rs`, `Cargo.toml`, `Cargo.lock` | AST symbols + manifest parser |
| TypeScript/JavaScript | `.ts`, `.tsx`, `.js`, `.jsx`, `package.json` | AST symbols + package parser |
| Python | `.py`, `pyproject.toml`, `requirements.txt`, `setup.py` | AST symbols + manifest parser |
| Go | `.go`, `go.mod`, `go.sum` | AST symbols + module parser |
| Java/Kotlin | `.java`, `.kt`, Gradle/Maven files | AST symbols + manifest parser |
| C/C++ | `.c`, `.cc`, `.cpp`, `.h`, `.hpp` | AST symbols where supported |
| C# | `.cs`, `.csproj`, `.sln` | AST symbols + project parser |
| Ruby/PHP/Elixir/Dart | source + package manifests | AST where supported, line fallback otherwise |
| Shell/PowerShell | `.sh`, `.bash`, `.zsh`, `.ps1` | function blocks + line fallback |

## Manifest and Dependency Chunking

Manifest files should be parsed as structured data, not treated as generic text.

Targets include:

| File/Pattern | Parser Facts | Graph Candidates |
|---|---|---|
| `Cargo.toml`, `Cargo.lock` | package, workspace, dependencies, features | repo/package/version/dependency edges |
| `package.json`, lockfiles | package, scripts, dependencies, engines | package/dependency/script/tool edges |
| `pyproject.toml`, `requirements*.txt`, `setup.py` | project metadata, dependencies, extras | package/dependency edges |
| `go.mod`, `go.sum` | module, dependencies, replacements | module/dependency edges |
| `pom.xml`, `build.gradle*` | project, dependencies, plugins | package/dependency/plugin edges |
| `.csproj`, `.sln` | projects, package refs | project/dependency edges |
| `Dockerfile` | base images, stages, exposed ports | image/base/service edges |
| `docker-compose*.yml`, Compose specs | services, images, ports, volumes, env keys, networks | service/image/network edges |
| `.env.example`, `.env.sample` | env keys, categories, defaults if safe | config/service/env-key edges |
| OpenAPI/Swagger | paths, operations, schemas, servers | API endpoint/type edges |
| GraphQL schemas | types, fields, operations | API type/field edges |
| protobuf/gRPC | services, RPCs, messages | API service/message edges |
| Terraform/Helm/Kubernetes YAML | resources, providers, images, services | infra resource edges |

Rules:

- Store env keys, not secret values.
- Preserve field-level provenance with JSON pointer/YAML path/line ranges.
- Emit one chunk per meaningful record/group when possible.
- Emit parse facts even if chunks are compact.
- Do not duplicate huge lockfile content into many low-value chunks; summarize
  and emit dependency facts with artifact refs for raw content.

## Markdown and Docs Chunking

Markdown/docs chunking preserves document structure.

Required behavior:

- Parse headings and heading hierarchy.
- Preserve fenced code blocks with language labels.
- Keep tables intact when they fit.
- Split oversized sections by subheadings, paragraphs, or list blocks.
- Include breadcrumb/title path in chunk metadata.
- Include source URL/path anchors.
- Preserve frontmatter as structured facts when present.

Chunk metadata:

| Field | Required | Meaning |
|---|---:|---|
| `heading_path` | no | Heading breadcrumb. |
| `section_level` | no | Heading depth. |
| `markdown_block_kind` | no | paragraph/list/table/code/frontmatter. |
| `code_fence_language` | no | Language for fenced code. |
| `doc_anchor` | no | Slug/anchor when known. |

## HTML and Web Chunking

HTML should normally be normalized to markdown/text before chunking, but DOM
metadata should be preserved when available.

Required behavior:

- Prefer extracted main content over nav/boilerplate.
- Preserve canonical URL, final URL, title, headings, and DOM selectors.
- Preserve API endpoint/network facts from endpoint discovery when requested.
- Mark thin pages and low-quality extraction.
- Keep tables/code/pre blocks intact when possible.

Fallbacks:

1. Readability/main content extraction.
2. DOM-to-markdown sections.
3. Visible text windows.
4. Raw text fallback with `chunking_fallback`.

## Transcript and Session Chunking

Transcript chunking applies to YouTube/media transcripts and AI session exports.

Media transcripts:

- chunk by chapter when available
- otherwise chunk by timestamp windows and speaker/channel changes
- preserve time ranges
- preserve video/channel/playlist metadata
- keep descriptions/comments separate from transcript body

AI sessions:

- chunk by turn, tool call, decision, or compact episode
- preserve provider, session id, project, model, role, and turn index
- emit graph candidates for skills, agents, tool calls, issues, PRs, files, and
  decisions
- large tool outputs become artifacts with summarized chunks
- secrets and local paths are redacted before vector payloads

## Structured Record Chunking

Structured formats include JSON, YAML, TOML, XML, CSV, RSS/Atom/JSON Feed,
registry API responses, MCP schemas, CLI outputs, and extracted LLM results.

Required behavior:

- Parse with a structured parser when possible.
- Emit compact searchable text for each meaningful record.
- Preserve raw structured payload only when bounded and safe.
- Use artifact refs for large raw payloads.
- Preserve JSON pointer/YAML path/XML path/CSV row provenance.
- Emit schema/type metadata.

Record chunk text should be intentionally searchable:

```text
OpenAPI operation POST /v1/sources
summary: Create/acquire/refresh a source lifecycle.
tags: sources
request: SourceRequest
response: SourceResult
```

## CLI and MCP Tool Output Chunking

Tool/script outputs are first-class source documents when acquired intentionally.

CLI output chunking:

- separate command metadata from stdout/stderr
- identify sections, records, tables, errors, warnings, and summaries
- hash raw stdout/stderr after redaction
- store large output as artifact
- include exit code, side-effect class, allowlist policy, and working-directory
  key

MCP output chunking:

- separate server/tool/resource/prompt schema documents
- chunk tool schemas by tool
- chunk resource content by resource kind
- chunk call results by structured record or section
- graph MCP server/tool/call/result nodes
- record client provider such as `mcporter` only as implementation metadata

## Binary and Artifact-Backed Documents

Binary documents should not be embedded raw.

Allowed behavior:

- emit `binary_metadata` chunks for filename, MIME type, size, hash, EXIF-like
  safe metadata, OCR/caption summaries, or extracted text
- store raw bytes in ArtifactStore
- emit graph facts when the binary links to a source, screenshot, WARC, archive,
  or tool output

Forbidden behavior:

- embedding base64 raw bytes
- putting large binary blobs in vector payloads
- exposing raw local file paths or secret-bearing metadata

## Repomix Rule

Repomix output may be used as acquisition/enumeration input.

Rules:

- Prefer Repomix JSON/library output when available.
- Split Repomix output into one `SourceDocument` per original file.
- Preserve original file path, language, byte/line ranges, and section markers.
- Route each recovered file back through `ChunkRouter`.
- Do not index a Repomix XML/Markdown/plain packed file as one source document.
- If only packed text is available, parse file boundaries first and emit a
  warning when boundaries are uncertain.

## Parse Facts and Graph Candidates

Chunking may emit parse facts and graph candidates, but graph persistence is a
separate step.

`SourceParseFacts` examples:

- code symbol declarations
- imports/exports
- dependency declarations
- API endpoints
- Docker services/images/ports/env keys
- package metadata
- session tool calls
- skill invocations
- MCP tool schemas
- CLI command schemas
- links between docs/repos/packages

`GraphCandidate` requirements:

| Field | Required | Meaning |
|---|---:|---|
| `kind` | yes | node or edge candidate kind. |
| `candidate_id` | yes | Stable candidate id. |
| `evidence` | yes | Source document/chunk/range evidence. |
| `confidence` | yes | 0.0 to 1.0 confidence. |
| `merge_key` | no | Stable graph merge key. |
| `metadata` | no | Redacted supporting data. |

Graph candidates must reference source ranges, not just whole documents, when
the parser can identify exact provenance.

## Fallback Contract

Fallbacks are valid, but they must be visible.

Common fallback chain:

```text
preferred parser
  -> simpler structured parser
  -> markdown/plain text sectioning
  -> line/paragraph windows
  -> atomic metadata chunk
  -> failed document with SourceError
```

Fallback metadata:

| Field | Meaning |
|---|---|
| `chunking_fallback` | Fallback reason. |
| `preferred_chunking_method` | Method that was attempted. |
| `actual_chunking_method` | Method used. |
| `parser_error_code` | Redacted parser error code. |
| `parse_status` | `parsed`, `partial`, `fallback`, `unsupported`, `failed`. |
| `quality_score` | Optional quality score for ranking/debugging. |

Failures that should degrade:

- unsupported language
- parse grammar error
- malformed markdown
- partial transcript timing
- huge lockfile
- partial MCP schema
- unrecognized structured record

Failures that should fail the document:

- missing required source ids
- unreadable content with no artifact/metadata fallback
- redaction failure for sensitive content
- chunk id generation failure
- source range cannot be computed at all

## Metadata Requirements

Every chunk payload must include the required fields from
`metadata-payload.md`, especially:

- `payload_contract_version`
- `source_id`
- `source_kind`
- `source_adapter`
- `source_scope`
- `source_generation` when mutable
- `source_item_key`
- `canonical_uri`
- `document_id`
- `chunk_id`
- `chunk_index`
- `content_kind`
- `content_hash`
- `chunk_hash`
- `chunk_locator`
- `source_range`
- `job_id`
- `document_status`
- `chunking_method`
- `chunking_profile`

Source-specific metadata must use the field names in `metadata-payload.md`.
Do not invent per-chunker aliases for path, language, symbol, package, session,
tool, or graph fields.

## PreparedDocument Validation

Reject or fail the document before embedding when:

- `SourceDocument.document_id` is missing
- `source_id` or `source_item_key` is missing
- `content_kind` is missing
- content is empty and no valid metadata/artifact fallback exists
- chunk list is empty
- any chunk lacks `chunk_id`, `chunk_key`, `chunk_index`, content hash,
  locator, or source range
- chunk ids are duplicated inside a document
- source ranges are impossible or unordered
- public metadata contains sensitive fields

Warn/degrade when:

- parser falls back
- symbol extraction fails
- graph facts are partial
- content is thin
- chunk exceeds target size but remains under hard max
- document is skipped due to configured ignore policy

## Quality Metrics

The preparer should emit metrics per document:

| Metric | Meaning |
|---|---|
| `chunks_total` | Number of chunks emitted. |
| `bytes_total` | Normalized document bytes. |
| `bytes_chunked` | Bytes covered by chunks. |
| `coverage_ratio` | Chunked bytes / total bytes. |
| `parser_success` | Whether preferred parser succeeded. |
| `fallback_count` | Number of fallback steps used. |
| `graph_candidates_total` | Candidate graph facts emitted. |
| `oversized_chunks_total` | Chunks above target size. |
| `empty_chunks_dropped` | Empty/low-value chunks removed. |

These metrics feed progress events and observability, not ranking by default.

## Observability

Document preparation must log/trace:

- selected chunking profile and method
- parser family and version
- fallback reason
- source document id and item key
- chunk count and byte coverage
- parse facts count
- graph candidate count
- warnings/errors
- duration

Progress events should increment:

- documents prepared
- chunks prepared
- parse facts emitted
- graph candidates emitted
- bytes prepared
- fallback count
- failed/skipped documents

## Security and Redaction

- Redact before chunking when sensitive values could enter chunks.
- Redact again before vector payload construction.
- Never embed raw secrets from env files, headers, cookies, tokens, signed URLs,
  tool inputs, or private credentials.
- `.env.example` and similar files may emit env keys and safe example values,
  but secret-looking values are redacted.
- Tool outputs must be redacted before hashing, chunking, and artifact storage
  unless a trusted local-only policy explicitly says otherwise.
- Keep raw sensitive artifacts out of public retrieval.

## Crosswalk

| Contract Concept | API DTO | Store/Provider |
|---|---|---|
| source item | `SourceDocument` | SourceLedger / DocumentCache |
| prepared document | `PreparedDocument` | DocumentStatus / EmbeddingProvider input |
| chunk | `ChunkSummary`, `ChunkDetail` | VectorStore / DocumentCache |
| parser facts | `SourceParseFacts` | Graph pipeline input |
| graph candidate | `GraphCandidate` | SourceGraph input |
| artifact-backed raw content | `Artifact*` | ArtifactStore |
| vector payload | `VectorPointBatch` | VectorStore |

## Validation Checklist

Implementation is incomplete until all of these pass:

- every adapter emits `SourceDocument`, never `PreparedDocument`
- `ChunkRouter` selects profiles through one shared path
- code chunking is symbol-centric for supported languages
- unsupported languages degrade to line-aware chunks
- manifest files emit dependency/service/API facts
- markdown/docs preserve heading hierarchy
- transcripts preserve timestamps/turns when available
- structured records preserve JSON/YAML/XML/CSV provenance
- CLI/MCP tool outputs are chunked with side-effect/redaction metadata
- Repomix output is split into original file documents
- every chunk has deterministic id, locator, hash, and source range
- every fallback is explicit in metadata
- parse facts and graph candidates include evidence locators
- vector payload fields align with `metadata-payload.md`
- chunking emits observability metrics and progress counts
