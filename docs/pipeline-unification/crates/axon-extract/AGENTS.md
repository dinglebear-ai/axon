# axon-extract design-contract maintenance

This directory documents the `axon-extract` boundary: Implement site/API vertical extractors and narrow context/output types; dispatch policy belongs in axon-adapters.
Rust implementations belong in the crate, not in this documentation directory.

## Read together

[Design contract](README.md) · [Current implementation guide](../../../../crates/axon-extract/src/AGENTS.md) · [Crate exports](../../../../crates/axon-extract/src/lib.rs)

[vertical extractor metadata](../../../../docs/architecture/specs/vertical-extractor-metadata.md) · [adding source adapter](../../../../docs/development/adding-source-adapter.md) · [metadata payload](../../../../docs/reference/sources/metadata-payload.md)

## Review the actual boundary

- A vertical exposes INFO, matches(), and extract(). Register the implementation here and matching dispatch/list entries in axon-adapters::vertical_registry; these are two crates, not two repositories.

- Honor auto_dispatch and policy boundaries; a matches() result must not bypass opt-in requirements. Network failures need safe actionable VerticalError context.

For implementation evidence, inspect [lib.rs](../../../../crates/axon-extract/src/lib.rs), [context.rs](../../../../crates/axon-extract/src/context.rs), [error.rs](../../../../crates/axon-extract/src/error.rs).
Check the manifest and actual callers before describing a dependency or API as
shipped. Distinguish current behavior, intended constraints, and remaining work;
historical phase/cutover prose is not authority to restore removed runtime paths
or to assume that existing databases are empty. Preserve dated outcomes.

## Verification and paired edits

Vertical matches truth tables and extraction fixtures, plus vertical_registry dispatch/exhaustiveness tests in axon-adapters.

When shapes or behavior change, update this contract and its linked live guide.
Regenerate schema projections from owning inputs rather than hand-editing them;
see [documentation validation](../../../development/documentation.md).
Documentation-only edits need link/structural checks, not provider deployment.
