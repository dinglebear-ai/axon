# axon-document design-contract maintenance

This directory documents the `axon-document` boundary: Turn acquired SourceDocument values into deterministic prepared chunks and preparation results.
Rust implementations belong in the crate, not in this documentation directory.

## Read together

[Design contract](README.md) · [Current implementation guide](../../../../crates/axon-document/src/AGENTS.md) · [Crate exports](../../../../crates/axon-document/src/lib.rs)

[chunking](../../../../docs/reference/sources/chunking.md) · [parsing](../../../../docs/reference/sources/parsing.md) · [metadata payload](../../../../docs/reference/sources/metadata-payload.md) · [adding parser](../../../../docs/development/adding-parser.md)

## Review the actual boundary

- Consume parser output through axon-parse; do not move parser ownership, acquisition, embedding, or publication into preparation.

- Changing profiles can alter index identity and retrieval quality; verify reindex/update behavior rather than only standalone string splitting.

For implementation evidence, inspect [preparer.rs](../../../../crates/axon-document/src/preparer.rs), [prepared.rs](../../../../crates/axon-document/src/prepared.rs), [chunk_router.rs](../../../../crates/axon-document/src/chunk_router.rs).
Check the manifest and actual callers before describing a dependency or API as
shipped. Distinguish current behavior, intended constraints, and remaining work;
historical phase/cutover prose is not authority to restore removed runtime paths
or to assume that existing databases are empty. Preserve dated outcomes.

## Verification and paired edits

preparer_tests, chunk_router_tests, local_source_tests and content-specific sidecars; cover empty/oversized input, deterministic IDs, spans, and fallback diagnostics.

When shapes or behavior change, update this contract and its linked live guide.
Regenerate schema projections from owning inputs rather than hand-editing them;
see [documentation validation](../../../development/documentation.md).
Documentation-only edits need link/structural checks, not provider deployment.
