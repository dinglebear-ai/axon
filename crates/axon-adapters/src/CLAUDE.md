# axon-adapters Agent Instructions

This file is the agent-facing contract for the `axon-adapters` crate docs.

## When Editing

- Keep source acquisition implementations and adapter capability declarations
  here.
- Every adapter must emit `SourceDocument` plus manifest/fetch metadata; never
  `PreparedDocument` or vector points.
- Update `../../../docs/pipeline-unification/crates/axon-adapters/README.md`, `../../../docs/pipeline-unification/sources/new-source-contract.md`,
  `../../../docs/pipeline-unification/sources/adapter-scopes.md`, and source-specific metadata docs together.
- Add fixtures for happy path, auth-required, degraded fetch, and failure.

## Review Checklist

- New sources declare scopes, auth/secrets, metadata, parsers, graph facts, and
  tests.
- No direct Qdrant, embedding, retrieval, or transport behavior.
- Acquisition errors carry fetch status and retry/degrade policy.
