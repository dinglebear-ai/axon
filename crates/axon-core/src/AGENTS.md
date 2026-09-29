# axon-core

Provide shared configuration, paths, HTTP/filesystem safety, redaction, logging, and runtime primitives without owning domain orchestration.

## Read before changing

[configuration](../../../docs/guides/configuration.md) · [config toml](../../../docs/reference/config/config-toml.md) · [env](../../../docs/reference/config/env.md) · [local source containment](../../../docs/reference/runtime/local-source-containment.md)

## Implementation map

[Crate exports](lib.rs) and [manifest](../Cargo.toml); focused entry points:
[config.rs](config.rs) · [config/](config/) · [paths.rs](paths.rs) · [http.rs](http.rs) · [redact.rs](redact.rs) · [logging.rs](logging.rs) · [health.rs](health.rs) · [sqlite.rs](sqlite.rs)

## Change requirements

- Trace actual loaders and parser wiring under config/ when changing precedence. Distinguish an env file on disk from process environment and CLI overrides; test the effective value.

- Change task-required keys while retaining unrelated settings and credentials. Test missing, malformed, renamed, and removed keys; update templates and generated registries.

- Preserve URL preflight and connect-time DNS/SSRF protection, path containment, and redaction in display/debug/error paths. Do not put acquisition, embedding, or service orchestration in generic helpers.

- Keep shared helpers below domain and transport dependencies. Build deterministic clock/ID/test boundaries rather than reaching into a running deployment from unit tests.

## Verification for code changes

Config precedence/parser, HTTP/SSRF, containment, redaction, and SQLite helper sidecars; generated config/env checks when inputs change.

Use focused `cargo test -p axon-core` targets. For contract changes, follow
[generated-contract validation](../../../docs/development/documentation.md);
update the linked references and affected transport consumers together.
