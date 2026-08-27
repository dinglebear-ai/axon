---
title: "Adding a Source Adapter"
created: 2026-07-07
updated: 2026-08-24
---

# Adding a Source Adapter

A source adapter turns a resolved source into `SourceDocument` values without bypassing Axon's shared source pipeline. This guide is the practical companion to `docs/pipeline-unification/sources/new-source-contract.md`.

See also `crates/axon-adapters/src/CLAUDE.md`, `docs/pipeline-unification/sources/adapter-scopes.md`, `docs/pipeline-unification/sources/metadata-payload.md`, and `docs/pipeline-unification/sources/source-graph.md`.

## First Decision: Do You Need A New Adapter?

A provider name is not, by itself, a reason to add a new source family. Prefer an existing acquisition boundary when it already matches the source shape.

| Input shape | Preferred Axon boundary |
|---|---|
| web page/site | `web` / `feed` |
| package registry | `registry` |
| local or exported file | `local` / `upload` |
| CLI-generated data | `cli_tool` |
| MCP server/tool data | `mcp_tool` |
| genuinely new acquisition/lifecycle model | new `SourceAdapter` |

Asana and Linear accessed through MCP should therefore start as MCP ingestion profiles, not `AsanaSourceAdapter` / `LinearSourceAdapter`. Promote a provider later only when Axon needs provider-native cursors, webhooks, attachment acquisition, delete/tombstone semantics, or authentication that no longer belongs behind MCP.

## Pipeline Boundary

```text
SourceRequest -> SourceResolver -> SourceRouter -> SourceAdapter
  -> SourceLedger -> SourceDocument -> SourceParseFacts / GraphCandidate
  -> DocumentPreparer -> EmbeddingProvider -> VectorStore -> DocumentStatus
```

Adapters own acquisition and normalization. They do not own ledger persistence, final chunking, embeddings, vector writes, graph persistence, job storage, or transport rendering.

## Prototype Ladder

### Level 0: File proof of value

Export useful data to JSON, NDJSON, or Markdown and ingest it through the local/upload path. This proves retrieval quality quickly, but the source identity remains file-backed and should be treated as disposable.

### Level 1: Contract-shaped export

Even when writing a file, emit deterministic logical record identifiers and canonical external URIs. At minimum preserve `source_item_key`, `canonical_uri`, title, normalized content, update timestamp, provider metadata, and known relationships.

### Level 2: MCP ingestion profile

For data available through MCP, extend the existing `mcp_tool` family so one authorized MCP invocation can return a structured multi-item snapshot. Axon promotes those records into `SourceManifest` items and `SourceDocument` values instead of indexing the entire tool response as one opaque document.

Recommended ownership:

```text
external SaaS
  -> upstream MCP server
  -> Labby gateway
       - upstream discovery
       - OAuth/token refresh
       - route/loadout/tool policy
       - snippet execution
  -> axon.mcp-ingest/v1 snapshot
  -> Axon mcp_tool adapter
       - stable item identity
       - manifest + diff
       - SourceDocument normalization
  -> shared parse/graph/prepare/embed/publish pipeline
```

Axon authenticates to Labby, not to Asana/Linear directly. Upstream OAuth credentials remain owned by Labby and never enter Axon source options, metadata, artifacts, graph facts, or vector payloads.

Labby's upstream OAuth state is subject-scoped. A homelab/single-user deployment may use a dedicated Axon ingestion service identity whose Labby subject has authorized the needed upstreams. Multi-user deployments must preserve or explicitly delegate the correct Labby subject instead of silently sharing one user's OAuth state.

### Level 3: Native provider adapter

Promote a provider only when native acquisition semantics materially improve correctness or operations. Preserve the logical item keys and canonical URIs established by the MCP profile so migration is deterministic.

## MCP Ingestion Envelope

The current `mcp_tool` code supports metadata-only behavior and an explicitly authorized call path, but call output is effectively one document. General ingestion needs a multi-record contract. A proposed shape is:

```json
{
  "schema": "axon.mcp-ingest/v1",
  "source": { "provider": "asana", "scope": "project", "external_id": "123" },
  "complete_snapshot": true,
  "items": [
    {
      "source_item_key": "asana:task:12091234567890",
      "canonical_uri": "https://app.asana.com/0/PROJECT/TASK",
      "content_kind": "structured",
      "title": "Example task",
      "body": "Normalized task content",
      "updated_at": "2026-08-24T20:00:00Z",
      "metadata": { "provider": "asana", "task_id": "12091234567890" },
      "relationships": []
    }
  ],
  "cursor": null
}
```

Required invariants:

- item keys and canonical URIs are deterministic and provider-stable;
- unchanged items hash identically and skip re-embedding;
- removals are inferred only from an explicitly complete snapshot or explicit tombstones;
- secrets and authorization material never enter persisted source content;
- raw provider payloads are optional evidence artifacts, not canonical vector payloads;
- the Labby provider accepts arguments/input, not only a server/tool name;
- large sets use pagination/cursors or another bounded mechanism.

## One-call Materialization For MCP Ingestion

`SourceAdapter::materialize` already runs once before `discover` / `acquire` / `normalize`. Existing registry, Reddit, and feed adapters use it to fetch once into a temporary dump. MCP ingestion should follow that pattern:

```text
materialize: call Labby once -> private temporary axon.mcp-ingest/v1 dump
discover: read dump -> SourceManifest
ledger: diff against prior generation
acquire: select added/modified records from the same dump
normalize: one SourceDocument per selected record
release: discard temporary materialization
```

This gives us the fast 'dump the Asana data and embed it' prototype without a persistent intermediate file and without hitting Asana twice.

Temporary materialization is sensitive storage, not an ordinary cache. Use an
OS-created unpredictable directory with mode `0700`, create the dump
exclusively with mode `0600`, reject/fail rather than follow symlinks, and keep
raw paths and contents out of logs. Cleanup must be owned by an RAII/scope guard
that runs after success, error, cancellation, and unwind; `release` remains the
normal-path lifecycle hook, not the only cleanup mechanism. Tests must verify
private permissions and removal after both provider failure and cancellation.

## Labby-backed MCP Migration: Code Impact

Moving MCP execution behind Labby should remove Axon-specific caller-command plumbing rather than create a second execution path.

### Remove

- `CommandMcpToolCaller` plus its `CliToolSource` / `execute_command` bridge in `crates/axon-adapters/src/mcp_tool.rs`.
- `command_caller(plan)` and `mcp_caller_command` parsing in `crates/axon-adapters/src/mcp_tool/adapter.rs`.
- `AXON_MCP_CALLER_COMMAND` and `AXON_MCP_CALLER_ALLOWLIST`, plus corresponding fields/validation in `crates/axon-services/src/source/dispatch/tool_auth.rs`.
- `mcp_caller_command`, `mcp_caller_allowlist`, and MCP-specific `env_allowlist` route option keys in `crates/axon-route/src/capability.rs`.
- command-caller tests that only prove `/bin/echo` or another local helper is configured/allowlisted.
- generated capability/reference fields for those removed options.

### Keep, with changed semantics

- `SourceKind::McpTool`, `SourceFamily::McpTool`, the `mcp_tool` adapter, canonical `mcp://...` identity, and `scope=tool/api`.
- `axon:execute` and an Axon-side exact MCP target allowlist as defense in depth.
- authorization audit events, but record Labby identity/route/target rather than a local caller command.
- timeout and response-size limits, applied to the Labby request/result in addition to Labby's limits.
- redaction and artifact capture, with artifacts representing bounded/redacted Labby/provider results rather than local stdout/stderr.
- metadata-only tool-contract indexing, but obtain real tool metadata/schema from Labby rather than manufacturing a placeholder document.

### Add or refactor

- Replace `McpToolCaller::call(&target)` with a provider boundary such as `McpSourceProvider` / `McpIngestionProvider` that accepts target + arguments and returns a bounded structured result.
- Inject the provider into `McpToolSourceAdapter` from `crates/axon-services/src/source/adapter_registry.rs`, following the existing upload/memory provider pattern. `axon-adapters` stays Labby-agnostic.
- Implement `McpToolSourceAdapter::materialize` and a temporary dump parser for `axon.mcp-ingest/v1`.
- Change ingestion-mode `discover` from one manifest item per server/tool to one per record.
- Change ingestion-mode `acquire` to select only added/modified records from the materialized snapshot and never call Labby a second time.
- Change ingestion-mode `normalize` to emit one `SourceDocument` per selected record.
- Update `crates/axon-adapters/src/mcp_tool/metadata.rs`: preserve shared MCP/tool provenance while merging approved provider/profile metadata, record an ingestion action distinctly from metadata-only indexing, and keep provider fields bounded/redacted.
- Remove job-id-based `execution_content_hash` for ingestion records; derive hashes from stable normalized record content.
- Define the external `axon.mcp-ingest/v1` DTO/schema in a transport-neutral contract layer, normally `axon-api`, because Labby snippets produce it and Axon consumes it.
- Add Labby endpoint/service-identity configuration in the service/config layer. Secrets belong in environment/credential storage, not source options.
- Add the concrete Labby provider in `axon-services`. Do not make `axon-services` depend on `axon-mcp`: `axon-mcp` already depends on `axon-services`, so that would form a dependency cycle. Reuse the existing service HTTP stack or a lower-level reusable MCP client dependency.

## Native Adapter Recipe

When a genuinely new source family is warranted, follow these steps.

### 1. Define identity before I/O

Specify `SourceKind` / `SourceFamily`, supported URI schemes, source canonical URI, stable source-id behavior, deterministic `source_item_key`, item canonical URI, and mutable/immutable identity rules before implementing network calls.

### 2. Declare the family contract

`crates/axon-adapters/src/spec.rs` defines `SourceFamily`, `ParserFamily`, and `SourceAdapterSpec`. Scope tables and matrix entries are split under `crates/axon-adapters/src/family_matrix/` (`matrix.rs`, `scopes_content.rs`, and `scopes_tooling.rs`). Add or update the appropriate rows there.

Declare scopes, credentials, parser/metadata families, watch/refresh support, network/local/tool capabilities, degradation modes, and required/optional graph facts honestly. Security capability flags are policy inputs, not decorative documentation.

If a new source or scope enum value is truly required, update the transport-neutral DTOs in `axon-api`; do not create adapter-local shadow enums.

### 3. Add resolver/router behavior

Resolver/router changes belong in `axon-route`. Explicit schemes must be deterministic; ambiguous shorthand must fail instead of silently selecting a family. The router selects adapter/scope/options/parser hints but does not perform acquisition.

### 4. Implement `SourceAdapter`

The live trait includes `materialize`, `discover`, `acquire`, `normalize`, optional progress/prefetch/archive hooks, and `release`. Use `materialize` when acquisition needs one-time prepared state before manifest discovery.

Core sequence:

```rust
async fn discover(&self, plan: &SourcePlan) -> Result<SourceManifest>;
async fn acquire(
    &self,
    plan: &SourcePlan,
    diff: &SourceManifestDiff,
) -> Result<SourceAcquisition>;
async fn normalize(
    &self,
    plan: &SourcePlan,
    acquisition: SourceAcquisition,
) -> Result<StageExecutionResult<Vec<SourceDocument>>>;
```

Do not embed, write Qdrant, commit ledger generations, write graph rows, own job storage, or render transport responses from the adapter.

### 5. Wire runtime composition

Concrete source adapters are assembled in `crates/axon-services/src/source/adapter_registry.rs` and validated against the normative family matrix by `SourceAdapterRegistry::validate()`. Add provider dependencies there when an adapter requires an injected runtime service.

### 6. Define parsing, metadata, graph, and chunking

Update `adapter-scopes.md`, `url-normalization.md`, `metadata-payload.md`, and `source-graph.md` before or with the code. Prefer existing parsers and chunk profiles unless new structure materially improves retrieval/graph semantics.

### 7. Add fixture packs and focused tests

Required adapter fixtures live under `crates/axon-adapters/fixtures/<adapter>/` and cover resolution, manifests, source documents, source jobs, auth, degraded behavior, provider failures, and metadata. Add the corresponding parser, graph, and vector-payload fixtures where required.

Prove explicit resolution, ambiguous rejection, added/modified/removed/unchanged diffing, normalization, auth failure, degradation/provider failure, redaction, source-job publication, graph declarations, and payload validity.

### 8. Refresh generated contracts

After changing schema inputs or adapter capabilities, use `cargo xtask generated-contracts refresh`, then `cargo xtask generated-contracts check`. Do not hand-edit generated reference artifacts.

### 9. Verify the smallest sufficient surface

Run focused route/adapter/service/parser/graph/generated-contract checks required by the changed files. Prose-only documentation changes do not require a full Rust build.

## Definition Of Done

A source is online when identity is deterministic, resolver/router behavior is explicit, acquisition/normalization use the shared pipeline, refresh skips unchanged embeddings, removals become durable cleanup work, metadata/graph evidence validate, security fails closed, fixture packs exist, generated capability surfaces match runtime behavior, and user-facing docs describe what is actually supported.
