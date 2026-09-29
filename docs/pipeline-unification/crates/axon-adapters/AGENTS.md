# axon-adapters design-contract maintenance

This directory documents the `axon-adapters` boundary: Acquire and normalize source content through SourceAdapter; do not own the downstream ingestion pipeline.
Rust implementations belong in the crate, not in this documentation directory.

## Read together

[Design contract](README.md) · [Current implementation guide](../../../../crates/axon-adapters/src/AGENTS.md) · [Crate exports](../../../../crates/axon-adapters/src/lib.rs)

[adding source](../../../../docs/development/adding-source.md) · [adding source adapter](../../../../docs/development/adding-source-adapter.md) · [adapter scopes](../../../../docs/reference/sources/adapter-scopes.md) · [metadata payload](../../../../docs/reference/sources/metadata-payload.md)

## Review the actual boundary

- Reuse local/upload, web/feed, registry, cli_tool, or mcp_tool before introducing a new source family. Register SourceAdapterSpec, stable identity, scopes, capabilities, auth requirements, and fixtures; onboarding_status checks declarations, not execution correctness.

- Keep skills.sh catalog acquisition metadata-only until licensing/right gates authorize content retrieval. Authenticated enrichment is opt-in and bounded; never persist its bearer token. CLI/MCP execution requires explicit policy, allowlists, timeout/output limits, and redaction.

For implementation evidence, inspect [adapter.rs](../../../../crates/axon-adapters/src/adapter.rs), [family_matrix.rs](../../../../crates/axon-adapters/src/family_matrix.rs), [onboarding.rs](../../../../crates/axon-adapters/src/onboarding.rs).
Check the manifest and actual callers before describing a dependency or API as
shipped. Distinguish current behavior, intended constraints, and remaining work;
historical phase/cutover prose is not authority to restore removed runtime paths
or to assume that existing databases are empty. Preserve dated outcomes.

## Verification and paired edits

adapter_tests, family_matrix_tests, onboarding_tests, source-specific sidecars, and the source-job integration tests in axon-services. Include denied execution, SSRF, cancellation, and retry/degradation cases.

When shapes or behavior change, update this contract and its linked live guide.
Regenerate schema projections from owning inputs rather than hand-editing them;
see [documentation validation](../../../development/documentation.md).
Documentation-only edits need link/structural checks, not provider deployment.
