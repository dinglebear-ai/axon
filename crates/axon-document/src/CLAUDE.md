# axon-document Agent Instructions

This file is the agent-facing contract for the `axon-document` crate docs.

## When Editing

- Keep document preparation, chunk routing, chunking profiles, prepared chunks,
  and chunk metadata here.
- Consume `SourceParseFacts`; do not implement parser ownership here.
- Do not add embedding calls, vector writes, source acquisition, or transport
  rendering.
- Update `../../../docs/pipeline-unification/crates/axon-document/README.md`, `../../../docs/pipeline-unification/sources/chunking-contract.md`, and
  `../../../docs/pipeline-unification/sources/metadata-payload.md` together.

## Review Checklist

- All adapters still emit `SourceDocument`; this crate emits `PreparedDocument`.
- Chunk ids are stable for unchanged source items.
- Unsupported content has a bounded fallback profile.
