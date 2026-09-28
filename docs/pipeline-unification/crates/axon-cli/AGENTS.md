# axon-cli design-contract maintenance

This directory documents the `axon-cli` boundary: Dispatch CLI operations and render human/JSON output, progress, and exit status over shared services.
Rust implementations belong in the crate, not in this documentation directory.

## Read together

[Design contract](README.md) · [Current implementation guide](../../../../crates/axon-cli/src/AGENTS.md) · [Crate exports](../../../../crates/axon-cli/src/lib.rs)

[commands](../../../../docs/reference/cli/commands.md) · [api parity](../../../../docs/reference/api-parity.md) · [testing](../../../../docs/development/testing.md)

## Review the actual boundary

- Argument/config definitions and bare-source routing also live in axon-core/src/config; inspect that implementation when changing parsing instead of assuming all clap code lives here.

- Distinguish detached submission from completion. Worker startup, cancellation, and job lifecycle output must agree with service behavior and other transports.

For implementation evidence, inspect [lib.rs](../../../../crates/axon-cli/src/lib.rs), [commands.rs](../../../../crates/axon-cli/src/commands.rs), [json.rs](../../../../crates/axon-cli/src/json.rs).
Check the manifest and actual callers before describing a dependency or API as
shipped. Distinguish current behavior, intended constraints, and remaining work;
historical phase/cutover prose is not authority to restore removed runtime paths
or to assume that existing databases are empty. Preserve dated outcomes.

## Verification and paired edits

Affected command sidecars plus CLI help/registry and output-parity tests. Exercise malformed arguments, detached work, and failure exits.

When shapes or behavior change, update this contract and its linked live guide.
Regenerate schema projections from owning inputs rather than hand-editing them;
see [documentation validation](../../../development/documentation.md).
Documentation-only edits need link/structural checks, not provider deployment.
