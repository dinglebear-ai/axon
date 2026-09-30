---
title: "Technology Choices -- Axon"
created: 2026-04-04
updated: 2026-09-29
---

# Technology Choices -- Axon

Last reviewed: 2026-09-29

## Language and runtime

Axon is a Rust workspace exposing CLI, MCP, and HTTP projections over shared
services. Tokio hosts in-process workers and asynchronous I/O. SQLite owns
durable jobs/source state; external Qdrant owns published retrieval vectors.
See [architecture](../overview.md) and [crate ownership](../crate-ownership.md).

The pinned toolchain is in [rust-toolchain.toml](../../../rust-toolchain.toml),
and workspace metadata/dependencies are in [Cargo.toml](../../../Cargo.toml).
Resolved versions belong to [Cargo.lock](../../../Cargo.lock), not copied
version ranges in this page. App manifests/lockfiles own their JavaScript
toolchains and standalone native dependencies.

## Key dependencies

| Responsibility | Technology and owner |
|---|---|
| CLI parsing | clap through core/CLI contracts |
| Serialization | serde/serde_json and typed public DTO/schema owners |
| HTTP server | Axum in `axon-web`, with shared authorization |
| MCP | rmcp in `axon-mcp`; canonical dispatch, projections, resources, tasks |
| HTTP acquisition | Shared guarded clients and Spider in `axon-adapters` |
| Parsing and preparation | `axon-parse` and `axon-document`, including syntax-aware code handling |
| Durable state | SQLx/SQLite in jobs, ledger, and other owning stores |
| Embeddings | Provider requests and retry/cooling in `axon-embedding` |
| Publication/retrieval | Qdrant writes in `axon-vectors`, queries/ranking in `axon-retrieval` |
| Synthesis | Configured backends in `axon-llm`, composed by services |
| Codex control | Typed app-server integration in `axon-codex` |

Exact workspace edges are generated in
[the dependency graph](../../reference/crate-dependency-graph.md). A dependency
listed by another crate is not necessarily a direct root dependency.

## Infrastructure and clients

The supported production runtime is native `axon serve` under systemd in
Incus or bare-metal Linux. Qdrant, TEI/other embedding providers, and
Chrome/CDP run externally as required. Compose is a development/reference
surface; its manifests own image tags, ports, and provider profiles.
A repository example is not a deployed-state inventory.

Clients include `apps/web`, `apps/palette-tauri`, `apps/android`, and
`apps/chrome-extension`. They share generated API/DTO contracts rather
than independent business logic. Read each app manifest for build/lint/test
commands instead of applying one package-manager assumption to every app.
The Palette Tauri backend is a standalone workspace.

## Embedding and document pipeline

The shared source pipeline resolves/acquires documents, diffs the ledger,
prepares chunks, requests embeddings, publishes a committed generation, then
updates graph/cleanup. Adapters emit normalized documents, not vector writes.
Stable hashes let unchanged content skip re-embedding; partial snapshots
cannot silently delete unseen items.

Chunk profiles, embedding batches, retry behavior, and provider concurrency
are operation- and configuration-dependent. There is no universal
2,000-character chunk, five-attempt retry policy, or implicit GPU-to-CPU
fallback for every source/provider. Use [source pipeline](../source-pipeline.md),
[chunking](../../reference/sources/chunking.md), and
[configuration](../../guides/configuration.md).

## Retrieval and synthesis

Qdrant collection layout must match model/vector dimensions and payload
contracts. Filters and committed-generation visibility remain required with
dense, sparse, hybrid, or re-ranked retrieval. Do not assume collection
renaming or first upsert migrates old state. Follow
[ask/RAG](../../guides/ask-rag.md) and [reindexing](../../guides/reindexing.md).

Synthesis is not Gemini-only. Configured backends include Gemini headless,
OpenAI-compatible HTTP, and Codex app-server. Credentials, isolation,
concurrency, and timeouts belong to the selected backend and effective
settings. `AXON_OPENAI_MODEL` is removed; consult current configuration.

## Crawl engine

Spider belongs to `axon-adapters`. Render selection uses HTTP and browser
paths; the browser remains an external privileged provider. Compiled
features are not proof of runtime optimization: conditional ETag reuse is
presently disabled despite compiled cache support.

Use [Spider flags](../../reference/spider-feature-flags.md),
[web crawls](../../guides/web-crawls.md), and shared HTTP safety. Do not copy
old upstream claims about unreachable features, silent throttling, or fixed
fallback thresholds without checking the locked code.

## Build tooling and maintenance

The Justfile, lefthook, Cargo tooling, xtask, and Python maintenance scripts
define actual gates. Caches affect build performance, not correctness.
Root [feature flags](../../reference/cargo-features.md) include placeholders
that do not implement their named capability by themselves.

Use [testing](../../development/testing.md) and
[documentation maintenance](../../development/documentation.md) for generation
and drift checks. Dated benchmarks describe the recorded revision/environment,
not a current dependency or latency contract.
