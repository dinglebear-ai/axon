# Qdrant Payload Schema Contract

Status: active
Last updated: 2026-07-16

This document is the authoritative reference for fields stored in Qdrant point
payloads. Code must conform to this contract; if the code diverges, update the
code and this document together in the same commit.

The pipeline-unification rewrite (#298) replaced the payload contract
wholesale. The current write path is `axon-vectors::payload` and
`axon-vectors::point` (`crates/axon-vectors/src/payload.rs`,
`crates/axon-vectors/src/point/point_payload.rs`). The generated source-family
schema lives in [`docs/reference/sources/vector-payload.md`](sources/vector-payload.md)
and [`docs/reference/sources/vector-payload.schema.json`](sources/vector-payload.schema.json).
Pre-unification payload markers are reset/cutover diagnostics only; normal
query, retrieve, delete, domain, source, and prune paths must use the current
canonical payload fields below.

---

## Required Fields

Every point built by `axon-vectors::point::build_payload` carries these fields
unconditionally. They are exactly `VECTOR_REQUIRED_FIELDS` in
`crates/axon-vectors/src/payload.rs`, enforced by
`VectorPayload::try_from_metadata`.

| Field | Qdrant type | Indexed | Notes |
|-------|-------------|---------|-------|
| `payload_contract_version` | keyword | no | Dated contract version string, e.g. `"2026-07-01"`. |
| `collection` | keyword | no | Qdrant collection name the point was written to. |
| `vector_point_id` | keyword | no | Store-level point id. |
| `vector_namespace` | keyword | yes | Dense vector namespace, usually `documents`. |
| `source_family` | keyword | yes | Source-family allowlist axis. |
| `source_kind` | keyword | yes | Canonical `SourceKind`. |
| `source_adapter` | keyword | yes | Adapter that acquired the source item. |
| `source_scope` | keyword | yes | Resolved source scope. |
| `source_id` | keyword | yes | Stable ledger id for the source identity. |
| `source_canonical_uri` | keyword | yes | Canonical URI of the source identity. |
| `source_item_key` | keyword | yes | Stable item key within the source. |
| `item_canonical_uri` | keyword | yes | Canonical URI of the item/page/file. |
| `source_generation` | integer | yes | Generation being written. |
| `committed_generation` | integer or null | yes | Latest generation safe for search; null until publisher commit. |
| `document_id` | keyword | yes | Document identity. |
| `chunk_id` | keyword | yes | Chunk identity. |
| `chunk_index` | integer | no | 0-based position within the document. |
| `content_kind` | keyword | yes | `code`, `markdown`, `html`, `plain_text`, `transcript`, `structured`, etc. |
| `content_hash` | keyword | no | Hash of normalized document content. |
| `chunk_hash` | keyword | no | Hash of normalized chunk text plus locator. |
| `chunk_text` | raw string | no | Stored chunk text; required and non-empty. |
| `chunk_locator` | object | no | Stable locator for the chunk within the source. |
| `source_range` | object | no | Byte/line/selector/time range for the chunk. |
| `visibility` | keyword | yes | `public`, `internal`, `sensitive`, `redacted`, or `derived`. |
| `redaction_status` | keyword | yes | `clean`, `redacted`, or `failed`. |
| `job_id` | keyword | yes | Job that produced this point. |
| `document_status` | keyword | yes | Usually `vectorized`, then `published` after commit. |
| `embedding_model` | keyword | yes | Embedding model name. |
| `embedding_dimensions` | integer | no | Dense vector dimensions. |
| `embedding_provider` | keyword | yes | TEI/OpenAI/etc. provider. |
| `embedding_profile` | keyword | yes | Embedding-pipeline profile. |
| `embedded_at` | datetime | no | Embed completion time (RFC3339). |
| `chunking_profile` | keyword | no | Chunking profile selected by the preparer. |
| `chunking_method` | keyword | no | Concrete chunking method used. |

`chunk_text` is required, not optional. The validator rejects empty or missing
chunk text and the point builder always stamps it from the prepared chunk.

---

## Optional Shared Fields

These fields are declared in `VECTOR_SHARED_FIELDS` but are not required. Write
them only when the adapter, preparer, or chunker has real data; do not emit null
placeholders.

| Field | Notes |
|-------|-------|
| `chunk_key` | Stable cleanup/upsert key; normally populated by `point_payload.rs`. |
| `embedding_batch_id` | Batch correlation id; normally populated by `point_payload.rs`. |
| `chunk_content_kind` | Chunk-level content classification. |
| `chunking_fallback`, `chunking_fallback_from` | Set when the ideal chunker falls back to a safer method. |
| `preferred_chunking_method`, `actual_chunking_method`, `code_chunk_source` | Set by code/document chunkers. |
| `markdown_block_kind`, `section_level`, `code_fence_language` | Set by the markdown chunker. |
| `structured_record_kind`, `toml_table` | Set by structured-format chunkers. |
| `transcript_speaker` | Transcript-turn speaker metadata. |
| `redaction_version`, `redacted_field_count`, `dropped_field_count`, `detector_names` | Redaction proof fields. |

The old universal structured-data fields are retired. The current equivalent,
`web_structured_kind` and `web_structured_blob`, is scoped to the `web` source
family.

---

## Source-Specific Fields

Source-specific payload fields are grouped by `source_family` and must appear
only in that family's allowlist. The allowlist is maintained in
`crates/axon-vectors/src/payload_families.rs` and rendered into the generated
source payload reference:

- [`docs/reference/sources/vector-payload.md`](sources/vector-payload.md)
- [`docs/reference/sources/vector-payload.schema.json`](sources/vector-payload.schema.json)

Unknown adapter metadata is rejected by the vector payload validator rather
than silently indexed as public data. New adapter fields must either be added to
the appropriate family allowlist or projected into one of the shared fields
above.

---

## Payload Index Profile

Collections are created with named `dense` and `bm42` sparse vectors for hybrid
RRF search. `required_retrieval_payload_indexes()` in
`crates/axon-vectors/src/collection.rs` defines the required field indexes.
The index profile is intentionally bounded to keyword/integer filter fields;
raw text, hashes, ranges, structured blobs, and family-specific metadata are
not indexed unless they are a proven query/filter surface.

| Field | Schema | Purpose |
|-------|--------|---------|
| `source_id` | keyword | Source-scoped lookup and cleanup. |
| `source_family` | keyword | Family filters and validation audits. |
| `source_kind` | keyword | Source-kind filters. |
| `source_adapter` | keyword | Adapter provenance filters. |
| `source_scope` | keyword | Page/site/repo/session scope filters. |
| `source_canonical_uri` | keyword | Source identity lookup. |
| `source_item_key` | keyword | Item lookup within a source. |
| `item_canonical_uri` | keyword | Retrieve/delete/domain/source lookup. |
| `source_generation` | integer | Generation-fenced cleanup and prune. |
| `committed_generation` | integer | Search visibility and generation fence. |
| `document_id` | keyword | Document lookup. |
| `chunk_id` | keyword | Chunk lookup. |
| `job_id` | keyword | Job provenance and audit. |
| `vector_namespace` | keyword | Vector namespace filtering. |
| `visibility` | keyword | Redaction/search visibility. |
| `redaction_status` | keyword | Fail-closed redaction filtering. |
| `document_status` | keyword | Lifecycle filtering. |
| `content_kind` | keyword | Content-kind filters. |
| `embedding_provider` | keyword | Embedding-provider audits. |
| `embedding_model` | keyword | Embedding-model audits. |
| `embedding_profile` | keyword | Embedding-profile audits. |
| `web_domain` | keyword | Domain listing and web domain filters. |

`normalize_collection_spec()` adds missing required indexes, and
`check_collection_drift()` rejects incompatible index schemas. Generation
indexes are integers; null committed generations remain valid payload values
but are excluded from committed-only query paths.

---

## Point Lifecycle

### Upsert

Points are upserted through `QdrantVectorStore::upsert`. Payload validation
runs before conversion to Qdrant REST bodies, so unknown fields, forbidden
field names, secret-looking values, failed redaction, and invalid generation
shapes fail before writes. Point IDs are deterministic store-level IDs.

### Retrieve And Query Visibility

Normal retrieval uses canonical target fields:

- `item_canonical_uri`
- `source_canonical_uri`
- `chunk_locator.canonical_uri`

Retrieve/query paths exclude uncommitted generations, explicitly redacted
points, and failed redaction status. Normal query/retrieve/delete paths must
not depend on pre-unification payload field names.

### Delete And Prune

Delete operations use generation-fenced selectors or canonical target fields.
Point-id deletes are bounded at 1000 IDs per `points/delete?wait=true` request.
Whole-collection normal prune deletes scroll point IDs and delete them in
bounded batches; it must not drop or recreate a Qdrant collection. Collection
recreation belongs only to explicit reset flows with a receipt.

---

## Design Rules

1. **Absent beats null.** Do not write `"field": null` for optional fields that
   are not applicable. Qdrant equality filters on absent fields produce no
   results, same as null, but absent fields do not bloat the payload.

2. **Flat beats nested for indexed fields.** Fields used for filtering or
   faceting must be flat top-level keys. Nested blobs are stored for reference
   but are not efficient filter surfaces.

3. **Arrays are stored as Qdrant keyword arrays.** Qdrant matches keyword
   arrays with values-count or match-any filters.

4. **Prefix namespacing is mandatory.** Every source-specific field must carry
   its source-family prefix or be explicitly listed in that family's allowlist.
   Shared fields have no source prefix.

5. **Indexes are bounded.** Add a field index only when a normal query,
   retrieve, delete, domain, source, or prune path needs it. Avoid indexing raw
   bodies, high-cardinality hashes, large structured blobs, and secrets.

6. **Renames are clean-break changes.** Renaming an indexed payload field
   requires cutover/reset or re-index evidence. Normal-path payload readers
   must use the new canonical field only.
