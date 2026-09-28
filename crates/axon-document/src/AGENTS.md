# axon-document

Turn acquired SourceDocument values into deterministic prepared chunks and preparation results.

## Read before changing

[chunking](../../../docs/reference/sources/chunking.md) · [parsing](../../../docs/reference/sources/parsing.md) · [metadata payload](../../../docs/reference/sources/metadata-payload.md) · [adding parser](../../../docs/development/adding-parser.md)

## Implementation map

[Crate exports](lib.rs) and [manifest](../Cargo.toml); focused entry points:
[preparer.rs](preparer.rs) · [prepared.rs](prepared.rs) · [chunk_router.rs](chunk_router.rs) · [profile.rs](profile.rs) · [source_range.rs](source_range.rs) · [metadata.rs](metadata.rs) · [parse.rs](parse.rs)

## Change requirements

- Consume parser output through axon-parse; do not move parser ownership, acquisition, embedding, or publication into preparation.

- Route content kinds through explicit profiles. Keep chunk identity stable for unchanged content and carry source/item/generation, content kind, parser/chunking version, and available source spans.

- Make fallback chunking bounded and observable. Do not silently discard evidence spans, split UTF-8 incorrectly, or pretend malformed input produced a complete document.

- Changing profiles can alter index identity and retrieval quality; verify reindex/update behavior rather than only standalone string splitting.

## Verification for code changes

preparer_tests, chunk_router_tests, local_source_tests and content-specific sidecars; cover empty/oversized input, deterministic IDs, spans, and fallback diagnostics.

Use focused `cargo test -p axon-document` targets. For contract changes, follow
[generated-contract validation](../../../docs/development/documentation.md);
update the linked references and affected transport consumers together.
