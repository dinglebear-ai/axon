# axon-error design-contract maintenance

This directory documents the `axon-error` boundary: Own the lowest shared typed error taxonomy and safe retry/cooling/degradation context.
Rust implementations belong in the crate, not in this documentation directory.

## Read together

[Design contract](README.md) · [Current implementation guide](../../../../crates/axon-error/src/AGENTS.md) · [Crate exports](../../../../crates/axon-error/src/lib.rs)

[error handling](../../../../docs/pipeline-unification/runtime/error-handling.md) · [redaction](../../../../docs/reference/runtime/redaction.md) · [observability](../../../../docs/reference/runtime/observability.md)

## Review the actual boundary

- Do not depend on higher Axon crates or add logging, response rendering, clients, stores, or scheduling here. axon-api owns shared transport projections.

- Keep JSON names stable and redaction safe in every projection. A context field or derived Debug implementation is not evidence that all supplied text is safe; test representative secret-bearing causes.

For implementation evidence, inspect [api_error.rs](../../../../crates/axon-error/src/api_error.rs), [code.rs](../../../../crates/axon-error/src/code.rs), [stage.rs](../../../../crates/axon-error/src/stage.rs).
Check the manifest and actual callers before describing a dependency or API as
shipped. Distinguish current behavior, intended constraints, and remaining work;
historical phase/cutover prose is not authority to restore removed runtime paths
or to assume that existing databases are empty. Preserve dated outcomes.

## Verification and paired edits

Taxonomy, conversion, retry/degradation, schema, and redaction sidecars. Assert recovery data and visibility, not only is_err().

When shapes or behavior change, update this contract and its linked live guide.
Regenerate schema projections from owning inputs rather than hand-editing them;
see [documentation validation](../../../development/documentation.md).
Documentation-only edits need link/structural checks, not provider deployment.
