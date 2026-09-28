# axon-llm design-contract maintenance

This directory documents the `axon-llm` boundary: Own synthesis/completion backends, streaming, capability handling, and provider concurrency.
Rust implementations belong in the crate, not in this documentation directory.

## Read together

[Design contract](README.md) · [Current implementation guide](../../../../crates/axon-llm/src/AGENTS.md) · [Crate exports](../../../../crates/axon-llm/src/lib.rs)

[adding provider](../../../../docs/development/adding-provider.md) · [providers](../../../../docs/reference/runtime/providers.md) · [ask rag](../../../../docs/guides/ask-rag.md) · [codex control](../../../../docs/guides/codex-control.md)

## Review the actual boundary

- Inspect runtime/ and BackendTextCompleter as well as LlmProvider. Do not assume every production call flows exclusively through the trait just because a design contract describes that target.

- Bound output, timeouts, concurrency, and retries; validate structured results and identify actual fallback/degradation. Preserve cancellation and final-event/error behavior while streaming.

For implementation evidence, inspect [provider.rs](../../../../crates/axon-llm/src/provider.rs), [runtime.rs](../../../../crates/axon-llm/src/runtime.rs), [runtime/](../../../../crates/axon-llm/src/runtime/).
Check the manifest and actual callers before describing a dependency or API as
shipped. Distinguish current behavior, intended constraints, and remaining work;
historical phase/cutover prose is not authority to restore removed runtime paths
or to assume that existing databases are empty. Preserve dated outcomes.

## Verification and paired edits

Provider/runtime backend sidecars plus synthesis/streaming tests in axon-services; include malformed output, partial streams, authentication, timeout, and redaction.

When shapes or behavior change, update this contract and its linked live guide.
Regenerate schema projections from owning inputs rather than hand-editing them;
see [documentation validation](../../../development/documentation.md).
Documentation-only edits need link/structural checks, not provider deployment.
