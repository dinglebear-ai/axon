---
title: "Vertical Extractor Metadata Spec"
created: 2026-05-21
updated: 2026-08-02
---

# Vertical Extractor Metadata Spec

Status: active

Vertical and registry adapters use the unified source pipeline. They do not
write legacy `ScrapedDoc` or `PreparedDoc` payloads directly to Qdrant.

## Contract

```text
SourceAdapter
  -> SourceDocument + MetadataMap
  -> SourceParseFacts / GraphCandidate
  -> DocumentPreparer
  -> canonical vector payload
  -> VectorStore
```

Adapters place source-specific acquisition metadata in the
`SourceDocument.metadata` map. Parsers may project evidence into
`SourceParseFacts`, and adapters or parsers may emit evidence-backed
`GraphCandidate` values. Shared preparation and vector publication construct
the final payload; adapters must never upsert points themselves.

The transport-neutral DTOs live in
[`crates/axon-api/src/source/`](../../../crates/axon-api/src/source/). The
canonical vector contract is
[`docs/reference/sources/vector-payload.schema.json`](../../reference/sources/vector-payload.schema.json),
with the readable field guide in
[`docs/reference/sources/metadata-payload.md`](../../reference/sources/metadata-payload.md).

## Naming and shape

- Shared provenance and lifecycle fields use the canonical `source_*`,
  `job_*`, `document_*`, `chunk_*`, `embedding_*`, and `vector_*` names.
- Adapter-specific fields use the registered source-family prefix documented
  by the vector-payload schema.
- Omit fields that do not apply; do not publish meaningless `null` values.
- Use structured scalars or arrays for values that callers filter on. Do not
  hide queryable values inside prose or untyped blobs.
- Preserve stable item keys and canonical URIs so ledger diffs, citations,
  graph evidence, and cleanup debt refer to the same source identity.

## Publication rules

Only committed source generations are visible to normal retrieval. Every
point carries the current `payload_contract_version`, source and generation
identity, document and chunk identity, and publication state required by the
generated schema. Schema-incompatible collections require the plan-first reset
and re-source flow; do not add compatibility writes in individual adapters.

## Adding or changing a vertical

1. Register or update its `SourceAdapterSpec` and source-family capability.
2. Emit `SourceDocument` metadata and any parse or graph facts.
3. Update the vector-payload family registry and generated schema when fields
   change.
4. Add adapter, payload, graph, and source-job fixtures.
5. Regenerate schemas and run the drift checks.

The complete onboarding checklist is
[`docs/development/adding-source.md`](../../development/adding-source.md).
