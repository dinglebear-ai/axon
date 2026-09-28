# axon-parse

Convert SourceDocument content into parser facts and evidence-backed graph candidates before preparation/persistence.

## Read before changing

[adding parser](../../../docs/development/adding-parser.md) · [parsing](../../../docs/reference/sources/parsing.md) · [source graph](../../../docs/reference/sources/source-graph.md) · [chunking](../../../docs/reference/sources/chunking.md)

## Implementation map

[Crate exports](lib.rs) and [manifest](../Cargo.toml); focused entry points:
[parser.rs](parser.rs) · [registry.rs](registry.rs) · [builtins.rs](builtins.rs) · [facts.rs](facts.rs) · [graph_candidate.rs](graph_candidate.rs) · [code.rs](code.rs) · [manifest.rs](manifest.rs) · [schema.rs](schema.rs) · [tool_schema.rs](tool_schema.rs)

## Change requirements

- Register capability/version and supported input selection. Use existing AST-backed code parsers where available; never manufacture unsupported semantic facts from weak text matches.

- Preserve evidence spans and source identity in facts and GraphCandidate output. Graph persistence, chunk production, fetching, and vectors remain outside this crate.

- Make unsupported or malformed content follow explicit bounded fallback/degradation policy with useful diagnostics; do not silently claim full parsing or expose secrets from env/config inputs.

- Version changes that alter facts must be reflected in preparation/reindex tests and schema/capability references.

## Verification for code changes

parser_tests and code/config/docker/env/manifest/schema/session/tool sidecars; cover invalid formats, unsupported inputs, evidence spans, and deterministic facts.

Use focused `cargo test -p axon-parse` targets. For contract changes, follow
[generated-contract validation](../../../docs/development/documentation.md);
update the linked references and affected transport consumers together.
