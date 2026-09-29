# axon-extract

Implement site/API vertical extractors and narrow context/output types; dispatch policy belongs in axon-adapters.

## Read before changing

[vertical extractor metadata](../../../docs/architecture/specs/vertical-extractor-metadata.md) · [adding source adapter](../../../docs/development/adding-source-adapter.md) · [metadata payload](../../../docs/reference/sources/metadata-payload.md)

## Implementation map

[Crate exports](lib.rs) and [manifest](../Cargo.toml); focused entry points:
[lib.rs](lib.rs) · [context.rs](context.rs) · [error.rs](error.rs) · [types.rs](types.rs) · [verticals.rs](verticals.rs) · [verticals/](verticals/)

## Change requirements

- A vertical exposes INFO, matches(), and extract(). Register the implementation here and matching dispatch/list entries in axon-adapters::vertical_registry; these are two crates, not two repositories.

- Keep the dependency one-way: adapters depend on extract, not the reverse. No ledger, job, chunking, embedding, vector, or transport ownership.

- Bump extractor output/version metadata when output shape materially changes. Keep normalized ScrapedDoc metadata and error distinctions usable by the shared acquisition path.

- Honor auto_dispatch and policy boundaries; a matches() result must not bypass opt-in requirements. Network failures need safe actionable VerticalError context.

## Verification for code changes

Vertical matches truth tables and extraction fixtures, plus vertical_registry dispatch/exhaustiveness tests in axon-adapters.

Use focused `cargo test -p axon-extract` targets. For contract changes, follow
[generated-contract validation](../../../docs/development/documentation.md);
update the linked references and affected transport consumers together.
