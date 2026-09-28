# axon-parse design-contract maintenance

This directory documents the `axon-parse` boundary: Convert SourceDocument content into parser facts and evidence-backed graph candidates before preparation/persistence.
Rust implementations belong in the crate, not in this documentation directory.

## Read together

[Design contract](README.md) · [Current implementation guide](../../../../crates/axon-parse/src/AGENTS.md) · [Crate exports](../../../../crates/axon-parse/src/lib.rs)

[adding parser](../../../../docs/development/adding-parser.md) · [parsing](../../../../docs/reference/sources/parsing.md) · [source graph](../../../../docs/reference/sources/source-graph.md) · [chunking](../../../../docs/reference/sources/chunking.md)

## Review the actual boundary

- Register capability/version and supported input selection. Use existing AST-backed code parsers where available; never manufacture unsupported semantic facts from weak text matches.

- Version changes that alter facts must be reflected in preparation/reindex tests and schema/capability references.

For implementation evidence, inspect [parser.rs](../../../../crates/axon-parse/src/parser.rs), [registry.rs](../../../../crates/axon-parse/src/registry.rs), [builtins.rs](../../../../crates/axon-parse/src/builtins.rs).
Check the manifest and actual callers before describing a dependency or API as
shipped. Distinguish current behavior, intended constraints, and remaining work;
historical phase/cutover prose is not authority to restore removed runtime paths
or to assume that existing databases are empty. Preserve dated outcomes.

## Verification and paired edits

parser_tests and code/config/docker/env/manifest/schema/session/tool sidecars; cover invalid formats, unsupported inputs, evidence spans, and deterministic facts.

When shapes or behavior change, update this contract and its linked live guide.
Regenerate schema projections from owning inputs rather than hand-editing them;
see [documentation validation](../../../development/documentation.md).
Documentation-only edits need link/structural checks, not provider deployment.
