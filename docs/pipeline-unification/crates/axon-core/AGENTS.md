# axon-core design-contract maintenance

This directory documents the `axon-core` boundary: Provide shared configuration, paths, HTTP/filesystem safety, redaction, logging, and runtime primitives without owning domain orchestration.
Rust implementations belong in the crate, not in this documentation directory.

## Read together

[Design contract](README.md) · [Current implementation guide](../../../../crates/axon-core/src/AGENTS.md) · [Crate exports](../../../../crates/axon-core/src/lib.rs)

[configuration](../../../../docs/guides/configuration.md) · [config toml](../../../../docs/reference/config/config-toml.md) · [env](../../../../docs/reference/config/env.md) · [local source containment](../../../../docs/reference/runtime/local-source-containment.md)

## Review the actual boundary

- Trace actual loaders and parser wiring under config/ when changing precedence. Distinguish an env file on disk from process environment and CLI overrides; test the effective value.

- Keep shared helpers below domain and transport dependencies. Build deterministic clock/ID/test boundaries rather than reaching into a running deployment from unit tests.

For implementation evidence, inspect [config.rs](../../../../crates/axon-core/src/config.rs), [config/](../../../../crates/axon-core/src/config/), [paths.rs](../../../../crates/axon-core/src/paths.rs).
Check the manifest and actual callers before describing a dependency or API as
shipped. Distinguish current behavior, intended constraints, and remaining work;
historical phase/cutover prose is not authority to restore removed runtime paths
or to assume that existing databases are empty. Preserve dated outcomes.

## Verification and paired edits

Config precedence/parser, HTTP/SSRF, containment, redaction, and SQLite helper sidecars; generated config/env checks when inputs change.

When shapes or behavior change, update this contract and its linked live guide.
Regenerate schema projections from owning inputs rather than hand-editing them;
see [documentation validation](../../../development/documentation.md).
Documentation-only edits need link/structural checks, not provider deployment.
