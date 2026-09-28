# axon-cli

Dispatch CLI operations and render human/JSON output, progress, and exit status over shared services.

## Read before changing

[commands](../../../docs/reference/cli/commands.md) · [api parity](../../../docs/reference/api-parity.md) · [testing](../../../docs/development/testing.md)

## Implementation map

[Crate exports](lib.rs) and [manifest](../Cargo.toml); focused entry points:
[lib.rs](lib.rs) · [commands.rs](commands.rs) · [json.rs](json.rs) · [ui.rs](ui.rs) · [schema_registry.rs](schema_registry.rs)

## Change requirements

- Argument/config definitions and bare-source routing also live in axon-core/src/config; inspect that implementation when changing parsing instead of assuming all clap code lives here.

- Focused scrape/crawl/embed/ingest commands reuse SourceRequest and unified jobs; code-search reads committed state. Do not restore removed per-family queues or legacy commands.

- Map commands through the shared service boundary. Keep JSON on stdout, progress on stderr, and failures visible through a non-success exit and actionable structured context.

- Distinguish detached submission from completion. Worker startup, cancellation, and job lifecycle output must agree with service behavior and other transports.

## Verification for code changes

Affected command sidecars plus CLI help/registry and output-parity tests. Exercise malformed arguments, detached work, and failure exits.

Use focused `cargo test -p axon-cli` targets. For contract changes, follow
[generated-contract validation](../../../docs/development/documentation.md);
update the linked references and affected transport consumers together.
