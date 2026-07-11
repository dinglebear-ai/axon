# axon-adapters Crate Contract
Last Modified: 2026-06-30

## Purpose

`axon-adapters` owns source acquisition implementations. Each adapter turns a
resolved source into manifests and `SourceDocument` values without bypassing the
shared pipeline.

## Owns

- `SourceAdapter` trait implementations
- adapter registry and declared capabilities/scopes
- acquisition for web, local files, Git repos, package registries, feeds,
  YouTube, Reddit, sessions, CLI tools, and MCP tools
- adapter-specific fetch status and source metadata
- acquisition fixtures and fake adapters

## Must Not Own

- source id/canonical URI construction
- ledger persistence, generation publishing, vector writes, final chunking, or
  search/RAG behavior
- CLI/MCP/REST rendering
- direct Qdrant upserts or embedding provider calls

## Public Modules

```text
lib.rs
adapter.rs
registry.rs
capability.rs
acquisition.rs
manifest.rs
web.rs
local.rs
git.rs
registry_sources.rs
feed.rs
youtube.rs
reddit.rs
sessions.rs
cli_tool.rs
mcp_tool.rs
boundary.rs
enrichment.rs
family_matrix.rs
onboarding.rs
spec.rs
testing.rs
```

## Public API

- `SourceAdapter`
- `AdapterRegistry`
- `AdapterCapability`
- `SourceAcquisition`
- `AcquiredItem`
- `AcquisitionManifest`
- `FetchStatus`
- `AdapterVersion`
- `FakeSourceAdapter`

## Dependencies Allowed

- `axon-api`, `axon-error`, `axon-core`, `axon-route`, `axon-authz`,
  `axon-observe`
- acquisition libraries such as HTTP, git, feed, transcript, archive, and tool
  clients when hidden behind adapter implementations

## Dependencies Forbidden

- `axon-vectors`, `axon-embedding`, `axon-retrieval`, `axon-services`
- direct job store ownership
- transport crates

## Generated Artifacts

- adapter capability registry for schemas and help output
- source-specific metadata examples in
  [../../sources/metadata-payload.md](../../sources/metadata-payload.md)

## Fixtures And Fakes

- fake adapter emitting added/changed/removed manifest entries
- static HTML docs fixture
- local repo fixture with code, manifests, schemas, and sessions
- CLI/MCP tool response fixture
- acquisition failure and degraded fetch fixtures

## Tests

- every adapter emits `SourceDocument`, never `PreparedDocument`
- every adapter declares scopes and required auth/secrets
- adapter failures include fetch status and retry/degradation policy
- acquisition does not write to ledger or vector store directly

## Acceptance Criteria

- bringing a new source online means registering an adapter, scope, parser,
  metadata, tests, and docs using
  [../../sources/new-source-contract.md](../../sources/new-source-contract.md)
- all acquired content enters the same source pipeline after acquisition

See [../README.md](../README.md) and
[../../sources/adapter-scopes.md](../../sources/adapter-scopes.md).
