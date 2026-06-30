# Vector Payload Schema Contract
Last Modified: 2026-06-30

## Contract

Vector payload schemas define the exact redacted metadata written to
`VectorStore`. They are separate from raw source metadata because vector payloads
are retrieval-facing and must never contain secrets or unbounded content.

## Generated Artifacts

This section is the target generated-artifact contract. Current implementation
payload docs live at `docs/reference/qdrant-payload-schema.md`, and current
payload builders live in the singular `axon-vector` crate. The
`docs/reference/sources/*` artifacts below are desired outputs of the
clean-break schema generator.

```text
docs/reference/sources/vector-payload.schema.json
docs/reference/sources/vector-payload.md
```

Generator:

```bash
cargo xtask schemas vector-payload
cargo xtask schemas vector-payload --check
```

## Source Inputs

The vector payload schema generator reads:

```text
crates/axon-vector/src/ops/tei/pipeline/payload.rs
crates/axon-vector/src/ops/tei/qdrant_store.rs
crates/axon-api/src/document.rs
crates/axon-api/src/vector.rs
crates/axon-parse/src/metadata*.rs       # target crate/input
docs/pipeline-unification/sources/metadata-payload.md
docs/pipeline-unification/sources/chunking-contract.md
```

The generated artifact records these paths in `x-axon.source_inputs`.

## Required Payload Fields

Every vector point includes exactly the required fields from
[../sources/metadata-payload.md](../sources/metadata-payload.md). This schema
contract must not maintain a second hand-written required-field list.

Canonical required fields:

- `payload_contract_version`
- `collection`
- `vector_point_id`
- `vector_namespace`
- `source_id`
- `source_kind`
- `source_adapter`
- `source_scope`
- `source_generation`
- `committed_generation`
- `source_item_key`
- `item_canonical_uri`
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
- `embedding_model`
- `embedding_dimensions`
- `embedding_provider`
- `embedding_profile`
- `embedded_at`

Current implementation fields such as `payload_schema_version`, `source_type`,
`url`, and `domain` are current-state only. The clean-break schema replaces
them with `payload_contract_version`, `source_kind`, `source_adapter`, and
canonical URI fields.

## Payload Schema Shape

```json
{
  "type": "object",
  "required": [
    "source_id",
    "vector_point_id",
    "vector_namespace",
    "source_kind",
    "source_canonical_uri",
    "item_canonical_uri",
    "source_item_key",
    "document_id",
    "chunk_id",
    "generation",
    "content_kind",
    "chunk_locator",
    "redaction_status",
    "visibility",
    "job_id",
    "embedding_model",
    "embedding_dimensions",
    "embedded_at"
  ],
  "properties": {
    "vector_point_id": { "type": "string", "x-qdrant-index": "keyword" },
    "vector_namespace": { "type": "string", "x-qdrant-index": "keyword" },
    "source_id": { "type": "string", "x-qdrant-index": "keyword" },
    "source_kind": { "type": "string", "x-qdrant-index": "keyword" },
    "source_canonical_uri": { "type": "string", "x-qdrant-index": "keyword" },
    "item_canonical_uri": { "type": "string", "x-qdrant-index": "keyword" },
    "source_item_key": { "type": "string", "x-qdrant-index": "keyword" },
    "document_id": { "type": "string", "x-qdrant-index": "keyword" },
    "chunk_id": { "type": "string", "x-qdrant-index": "keyword" },
    "generation": { "type": "integer", "minimum": 0, "x-qdrant-index": "integer" },
    "content_kind": { "type": "string", "x-qdrant-index": "keyword" },
    "chunk_text": { "type": "string", "x-qdrant-index": "full_text" },
    "chunk_locator": { "type": "string" },
    "source_range": { "$ref": "#/$defs/SourceRange" },
    "redaction_status": { "$ref": "#/$defs/RedactionStatus" },
    "visibility": { "$ref": "#/$defs/Visibility" },
    "job_id": { "type": "string", "x-qdrant-index": "keyword" },
    "embedding_model": { "type": "string", "x-qdrant-index": "keyword" },
    "embedding_dimensions": { "type": "integer", "minimum": 1 },
    "embedded_at": { "type": "string", "format": "date-time" }
  },
  "additionalProperties": false
}
```

`chunk_text` is optional and bounded. If present, it is redacted text used for
full-text or sparse retrieval. Large raw content belongs in ArtifactStore or
DocumentCache, not payload.

## Payload Index Plan

The generated schema emits a Qdrant index plan:

```json
{
  "collection": "axon",
  "indexes": [
    { "field_name": "source_id", "field_schema": "keyword" },
    { "field_name": "vector_namespace", "field_schema": "keyword" },
    { "field_name": "source_kind", "field_schema": "keyword" },
    { "field_name": "source_canonical_uri", "field_schema": "keyword" },
    { "field_name": "item_canonical_uri", "field_schema": "keyword" },
    { "field_name": "document_id", "field_schema": "keyword" },
    { "field_name": "generation", "field_schema": "integer" },
    { "field_name": "content_kind", "field_schema": "keyword" },
    { "field_name": "job_id", "field_schema": "keyword" },
    { "field_name": "embedding_model", "field_schema": "keyword" }
  ]
}
```

Index rules:

- every field used by query/retrieve/ask filters has an index entry
- every indexed payload field exists in the schema
- index plans are generated before collection creation
- changing index type requires collection recreation or explicit migration plan

## Required Source-Specific Field Families

| Family | Prefix | Examples |
|---|---|---|
| code | `code_` | `code_language`, `code_symbol_name`, `code_symbol_kind`, `code_file_type` |
| web | `web_` | `web_title`, `web_domain`, `web_status_code`, `web_depth` |
| package | `package_` | `package_ecosystem`, `package_name`, `package_version` |
| session | `session_` | `session_id`, `session_turn_index`, `session_tool_name`, `session_skill_name` |
| graph | `graph_` | `graph_node_ids`, `graph_edge_ids`, `graph_confidence` |
| memory | `memory_` | `memory_id`, `memory_importance`, `memory_status` |

All source-specific fields must be documented in
`sources/metadata-payload.md` and generated into this schema.

Source-specific field record:

```json
{
  "field_name": "code_symbol_name",
  "family": "code",
  "json_type": "string",
  "required_for": ["content_kind=code"],
  "visibility": "public",
  "redaction": "none",
  "qdrant_index": "keyword",
  "owner": "axon-parse"
}
```

Unknown source-specific fields are rejected unless they are added to this
registry.

## Dense/Sparse Vector Contract

The payload schema is paired with collection vector config:

```json
{
  "vectors": {
    "dense": {
      "size": 1024,
      "distance": "Cosine"
    }
  },
  "sparse_vectors": {
    "bm42": {
      "modifier": "idf"
    }
  }
}
```

Dimension is provider-derived, not hardcoded; the generated schema records the
configured embedding model and dimensions.

## Vector Point Shape

The vector store receives points shaped like:

```json
{
  "id": "vpt_...",
  "vectors": {
    "dense": [0.01],
    "bm42": {
      "indices": [1],
      "values": [0.5]
    }
  },
  "payload": { "$ref": "#/$defs/VectorPayload" }
}
```

Point id rules:

- stable for `(collection, vector_namespace, document_id, chunk_id,
  embedding_model, generation)`
- changes when embedding model or chunk hash changes
- old point ids become cleanup debt when generation is replaced

## Redaction Contract

Payload builder must prove:

- every payload has `redaction_status`
- every payload has `visibility`
- secret detectors ran before vector write
- fields marked `sensitive` are absent or redacted
- unknown adapter metadata is not copied into payload by default

Forbidden payload fields:

- raw auth headers
- cookies
- API keys/tokens
- full absolute home paths as public identity
- raw `.env` values
- unbounded raw HTML
- unbounded raw transcript/session body
- raw LLM prompts containing private content
- adapter response blobs

## Validation Fixtures

Required fixtures:

```text
crates/axon-vectors/tests/fixtures/payload/code.valid.json
crates/axon-vectors/tests/fixtures/payload/web.valid.json
crates/axon-vectors/tests/fixtures/payload/session.valid.json
crates/axon-vectors/tests/fixtures/payload/memory.valid.json
crates/axon-vectors/tests/fixtures/payload/package.valid.json
crates/axon-vectors/tests/fixtures/payload/secret.invalid.json
crates/axon-vectors/tests/fixtures/payload/missing_generation.invalid.json
crates/axon-vectors/tests/fixtures/payload/unknown_source_field.invalid.json
crates/axon-vectors/tests/fixtures/payload/bad_visibility.invalid.json
```

Source-specific fields are allowed only under approved field families from
`metadata-payload.md`.

## Rules

- no secrets
- no raw absolute local paths as public identity
- no giant raw content fields
- generation fields are filterable
- source/document/chunk ids join back to ledger
- payload indexes are declared here before collection creation

## Drift Checks

Fail when:

- payload builder writes field absent from schema
- schema requires field absent from payload builder
- redaction metadata is missing
- Qdrant index plan differs from payload schema
- examples in metadata-payload docs fail validation
- payload fixture contains a field not in the generated schema
- query/retrieve filter references an unindexed payload field
- source-specific metadata registry differs from payload schema

## Acceptance Criteria

- payload builder cannot write fields absent from schema
- schema cannot require fields missing from payload builder
- generated Qdrant index plan matches schema annotations
- secret fixture is rejected before vector write
- every payload joins to ledger by `source_id`, `source_item_key`,
  `document_id`, `chunk_id`, and `generation`
- source-specific metadata fields are documented before use
- all queryable payload fields have generated Qdrant index specs
- old-generation payload cleanup can be selected without scrolling unrelated
  sources
