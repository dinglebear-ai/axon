# axon-adapters — Agent Guide

`axon-adapters` owns **source acquisition**. Each adapter turns a `ResolvedSource`
(from `axon-route`) into an `AcquisitionManifest` and `SourceDocument` values —
without bypassing the shared pipeline. It answers "how do I fetch this source
family, at what declared scope, and what did the fetch return." Full contract
(owns / API / deps / tests):
[../../../docs/pipeline-unification/crates/axon-adapters/README.md](../../../docs/pipeline-unification/crates/axon-adapters/README.md)
· behavior spec:
[../../../docs/pipeline-unification/sources/adapter-scopes.md](../../../docs/pipeline-unification/sources/adapter-scopes.md)
· [../../../docs/pipeline-unification/sources/new-source-contract.md](../../../docs/pipeline-unification/sources/new-source-contract.md).

## Status — PR0 skeleton
Modules below are **markers only**. The adapter framework (trait, registry,
capability, manifest) lands in **Phase 4 (Source Resolver, Router, And Adapter
Registry)**; per-source ports land in **Phase 9 (Port Source Families)**,
migrated one family at a time from the root crate's crawl/ingest logic. Do not
add ledger, embedding, vector, or transport behavior here.

## Module map
| File | Owns |
|---|---|
| `adapter.rs` | `SourceAdapter` trait |
| `registry.rs` | `AdapterRegistry` — registration + lookup |
| `capability.rs` | `AdapterCapability`, `AdapterVersion`, declared scopes |
| `acquisition.rs` | `SourceAcquisition`, `AcquiredItem`, `FetchStatus` |
| `manifest.rs` | `AcquisitionManifest` (added/changed/removed) |
| `web.rs` / `local.rs` / `git.rs` | web page/site, local file/dir, git repo adapters |
| `registry_sources.rs` / `feed.rs` | package-registry, RSS/Atom/JSON feed adapters |
| `youtube.rs` / `reddit.rs` / `sessions.rs` | media/social/session-export adapters |
| `cli_tool.rs` / `mcp_tool.rs` | CLI-tool and MCP-tool call adapters |
| `testing.rs` | `FakeSourceAdapter` + happy/auth/degraded/failure fixtures |

## Boundary — keep OUT of this crate
- Source id / canonical URI construction (that is `axon-route`).
- Ledger persistence, generation publishing, final chunking, embedding, vector writes, search/RAG.
- Direct Qdrant upserts or embedding-provider calls; direct job-store ownership.
- CLI/MCP/REST rendering.

## Dependencies
- **Allowed:** `axon-api`, `axon-error`, `axon-core`, `axon-route`, `axon-authz`, `axon-observe`, and acquisition libs (HTTP/git/feed/transcript/archive/tool clients) hidden behind adapter impls.
- **Forbidden:** `axon-vectors`/`axon-embedding`/`axon-retrieval`/`axon-services`, direct job store, transport crates. Enforced by `cargo xtask check-layering`.

## Invariants (review checklist)
- **Every adapter emits `SourceDocument`, never `PreparedDocument`** or vector points.
- **Every adapter declares scopes and required auth/secrets.**
- **Acquisition never writes to ledger or vector store directly** — all acquired content re-enters the shared pipeline afterward.
- Adapter failures carry `FetchStatus` plus a retry/degradation policy.
- Bringing a new source online = register adapter + scope + parser + metadata + tests + docs per `sources/new-source-contract.md`.

## DTO ownership
Wire DTOs (`SourceDocument`, `AcquisitionManifest`, `AcquiredItem`,
`FetchStatus`, `AdapterCapability`, `AdapterVersion`) are defined in **`axon-api`**;
this crate emits them — it does not redefine transport-facing shapes.

## Keep in sync when shapes change
`README.md` (crate contract) · `sources/new-source-contract.md` ·
`sources/adapter-scopes.md` · `sources/metadata-payload.md` (source-specific
metadata) · the adapter DTO/capability components in `axon-api`.
