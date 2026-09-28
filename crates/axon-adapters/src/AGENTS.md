# axon-adapters

Acquire and normalize source content through SourceAdapter; do not own the downstream ingestion pipeline.

## Read before changing

[adding source](../../../docs/development/adding-source.md) · [adding source adapter](../../../docs/development/adding-source-adapter.md) · [adapter scopes](../../../docs/reference/sources/adapter-scopes.md) · [metadata payload](../../../docs/reference/sources/metadata-payload.md)

## Implementation map

[Crate exports](lib.rs) and [manifest](../Cargo.toml); focused entry points:
[adapter.rs](adapter.rs) · [family_matrix.rs](family_matrix.rs) · [onboarding.rs](onboarding.rs) · [artifact_candidates.rs](artifact_candidates.rs) · [vertical_registry.rs](vertical_registry.rs) · [web_engine.rs](web_engine.rs)

## Change requirements

- Reuse local/upload, web/feed, registry, cli_tool, or mcp_tool before introducing a new source family. Register SourceAdapterSpec, stable identity, scopes, capabilities, auth requirements, and fixtures; onboarding_status checks declarations, not execution correctness.

- Emit SourceDocument and acquisition/manifest metadata. Chunking, embeddings, ledger writes, graph persistence, and transport envelopes belong elsewhere. materialize/discover/acquire/normalize must consume a consistent snapshot; streaming must preserve ordinal/final and progress semantics.

- Test unchanged, added, modified, removed, partial-page, and interrupted snapshots. Never infer deletion from incomplete acquisition. Temporary sensitive materializations require private permissions and cleanup on every exit.

- Artifact candidates are bounded evidence associated with changed documents, not a second acquisition path or Depot publication authority. Keep neutral candidate shape separate from Axon batch/sink receipts.

- Keep skills.sh catalog acquisition metadata-only until licensing/right gates authorize content retrieval. Authenticated enrichment is opt-in and bounded; never persist its bearer token. CLI/MCP execution requires explicit policy, allowlists, timeout/output limits, and redaction.

## Verification for code changes

adapter_tests, family_matrix_tests, onboarding_tests, source-specific sidecars, and the source-job integration tests in axon-services. Include denied execution, SSRF, cancellation, and retry/degradation cases.

Use focused `cargo test -p axon-adapters` targets. For contract changes, follow
[generated-contract validation](../../../docs/development/documentation.md);
update the linked references and affected transport consumers together.
