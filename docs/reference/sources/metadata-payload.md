# Metadata Payload

Last Modified: 2026-09-30

Every vector point carries a structured payload that makes it traceable to its
source, generation, document, chunk, and embedding. Payloads are the queryable
surface for filtering, grouping, pruning, and citation — prose-only fields are
not allowed for values clients need to filter on.

> Authoritative schema:
> [`vector-payload.schema.json`](vector-payload.schema.json). Contract source:
> [`docs/pipeline-unification/sources/metadata-payload.md`](../../pipeline-unification/sources/metadata-payload.md).
> Live enforcement: [`crates/axon-vectors/src/payload.rs`](../../../crates/axon-vectors/src/payload.rs)
> (`VECTOR_REQUIRED_FIELDS`) and `payload_families.rs` (`VECTOR_SOURCE_FAMILIES`).
> A payload missing any required field is rejected **before** it reaches Qdrant.

## Field-name prefixes

| Prefix | Owner |
|---|---|
| `source_*` | resolver/router/ledger |
| `job_*` | JobStore |
| `document_*` | DocumentPreparer/Status |
| `content_*`, `chunk_*` | adapter/preparer |
| `embedding_*` | EmbeddingProvider |
| `vector_*` | VectorStore |
| `graph_*` | SourceGraph |
| `artifact_*`, `memory_*` | ArtifactStore / MemoryStore |
| `web_*`, `git_*`, `code_*`, `package_*`, `session_*`, `tool_*` | source-family-specific |

No transport-specific (`mcp_*`/`rest_*`) fields unless the source is itself an
MCP/REST object.

## `source_family` (required, narrower than `source_kind`)

14 families: `code`, `web`, `package`, `session`, `graph`, `memory`, `feed`,
`social`, `media`, `local`, `tool`, `docker`, `env`, `upload`.

> **Shipped divergences:** the `registry` adapter stamps the invalid
> `source_family="registry"` and `axon-services` remaps it to `package` (and
> `pkg_*` → `package_*`) before validation. Reddit ships `reddit_*` field names
> (not `social_*`); YouTube ships `yt_*` (not `transcript_*`) — deliberate
> deferred promotions, since renaming shipped indexed fields is a breaking
> payload change.

## Required fields

`payload_contract_version`, `collection`, `vector_point_id`, `vector_namespace`,
`source_family`, `source_id`, `source_kind`, `source_adapter`, `source_scope`,
`source_canonical_uri`, `source_generation`, `committed_generation`,
`source_item_key`, `item_canonical_uri`, `document_id`, `chunk_id`,
`chunk_index`, `content_kind`, `content_path`, `content_language`,
`content_hash`, `chunk_hash`, `chunk_text` (non-empty), `chunk_locator`,
`source_range`, `visibility`, `redaction_status`, `job_id`, `document_status`,
`embedding_model`, `embedding_dimensions`, `embedding_provider`,
`embedding_profile`, `embedded_at`, `chunking_profile`, `chunking_method`.

Optional: `graph_node_ids`, `graph_edge_ids`, `graph_confidence`,
`authority_score`, `freshness_score`, `quality_score`, `dedupe_key`,
`artifact_id`, `redaction_profile`, `tenant_id`.

Identical Git files can share a canonical document. `source_item_aliases` and
`item_canonical_uri_aliases` retain its other repository paths and URIs as arrays;
path filters include these aliases. Alias membership participates in refresh and
prepared-document reuse. Inventories exceeding the alias count or payload budget
retain separate documents instead of truncating paths.
`source_path_prefixes` records canonical and aliased directory ancestors for
exact directory filtering on complete Git inventories, including older Qdrant
versions without native prefix matching. Legacy points retain their prior text
matching behavior until refreshed.
Removing an aliased file is reconciled by a complete source refresh. Standalone
canonical-URI point deletion targets the canonical document only; an alias URI
does not delete the shared document and its other paths.

## Code-specific fields

`code_file_path` (required for code), `code_language`, `code_file_type`
(`source`/`test`/`config`/`docs`/`generated`/`lockfile`/`schema`),
`code_is_test`, `code_is_generated`, `code_parser`, `code_parser_version`,
`code_parse_status` (`parsed`/`partial`/`fallback`/`unsupported`/`failed`),
`code_chunk_source` (`ast_symbol`/`ast_node`/`markdown_fence`/`line_window`),
**`code_symbol_name`** / **`code_symbol_kind`** (not bare `symbol_*` — bare
forms are rejected for the `code` family), `symbol_kind` ∈
{`function`,`method`,`struct`,`class`,`module`,`trait`},
`symbol_qualified_name`, `symbol_signature`, `symbol_visibility`,
`symbol_parent`, `symbol_extraction_status`, `dependency_manifest_kind`,
`schema_kind`.

`code_symbol_aliases` retains multiple symbols sharing one source interval.
`code_symbol_source_range` records the original declaration range when its
embedding interval is partitioned. `code_syntax_recovered` identifies clean AST
symbols retained from a partially parsed file. `chunking_method` describes the
individual chunk, including any bounded-window fallback.
Methods can include `tree_sitter`, `heuristic_symbol`, `atomic_code`,
`atomic_manifest`, `line_window`, `plain_text_windows`, `source_adjacent_packing`,
or `source_context`; a document summary does not overwrite that chunk's method.

## Visibility, redaction, hashing

- **Visibility** classes: `public`, `internal`, `sensitive`, `redacted`,
  `derived`.
- **`redaction_status`** (required, stamped immediately before validation):
  `clean`, `redacted`, `failed`.
- **Hashing** — all SHA-256 over UTF-8 bytes, encoded `sha256:<lowercase hex>`:
  `raw_content_hash` (pre-normalization), `content_hash` (post-normalization),
  `chunk_hash` (chunk text + locator metadata).

## Forbidden in payloads

Absolute local home paths (unless explicitly allowed); raw request/response
headers that may carry credentials; bearer tokens/API keys/cookies/session ids/
signed URLs; unredacted environment variables; raw tool inputs marked sensitive;
raw LLM prompts that include secrets; private file contents the caller lacks
scope to see.

If the payload contract changes, update this file,
`crates/axon-vectors/src/payload.rs`, and regenerate
`vector-payload.schema.json` in the same PR.
