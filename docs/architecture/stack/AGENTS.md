# Stack documentation

[Architecture](arch.md), [technology choices](tech.md), and [prerequisites](pre-reqs.md) must describe the same runtime as the [current crate map](../crate-structure.md) and [source pipeline](../source-pipeline.md).

## Review requirements

Axon has CLI, MCP, and HTTP/web projections; do not call it a two-mode application. Source jobs use a unified SQLite/in-process runtime, not separate crawl/embed/ingest worker services. Provider clients and stores retain their crate boundaries.

Verify build prerequisites against [Cargo.toml](../../../Cargo.toml), [the toolchain](../../../rust-toolchain.toml), and [contributing](../../development/contributing.md). Host-specific linker/cache settings are not universal prerequisites.

Use [deployment](../../operations/deployment.md) for the supported systemd/Incus contract and [configuration](../../guides/configuration.md) for provider options. Describe Compose as a development/reference surface in shared docs; actual local exceptions belong in private deployment notes. Do not assume a particular GPU, LLM backend, or service address.

Link architecture changes to the actual modules and [ownership rules](../crate-ownership.md). Keep historical cutover diagrams labeled as history rather than claiming a live system still uses removed crates.
