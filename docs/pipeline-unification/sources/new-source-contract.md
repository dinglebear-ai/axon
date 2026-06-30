# New Source Contract
Last Modified: 2026-06-30

## Contract

This is the implementation contract for bringing a new source online in the
unified Axon pipeline.

Every new source enters through the same path:

```text
SourceRequest
  -> SourceResolver
  -> SourceRouter
  -> SourceAdapter
  -> SourceLedger
  -> SourceDocument
  -> SourceParseFacts / GraphCandidate
  -> DocumentPreparer
  -> EmbeddingProvider
  -> VectorStore
  -> DocumentStatus
```

No source may bypass the shared pipeline by writing `PreparedDocument`, vector
points, graph rows, or transport responses directly. Source-specific behavior is
allowed only behind adapter, parser, chunking, metadata, auth, and graph
contracts.

## Required Source Onboarding Checklist

To add a source, implement all rows in this checklist.

| Area | Required Work | Owning Contract |
|---|---|---|
| Source identity | Define source kind, URI forms, canonical URI, source id strategy, and aliases. | `url-normalization.md` |
| Resolver | Add resolver rules and ambiguity handling. | `source-pipeline.md` |
| Router | Register adapter selection and scope defaults. | `adapter-scopes.md` |
| Adapter | Implement acquisition that emits `SourceDocument` only. | this file |
| Scopes | Declare supported scopes, default scope, map behavior, watch support, and option schema. | `adapter-scopes.md` |
| Ledger | Define item keys, manifest hash inputs, diff behavior, generation semantics, and cleanup debt. | `ledger-contract.md` |
| Parsing | Add parser family or declare parse-free behavior. | `parsing-contract.md` |
| Graph | Declare required and optional graph facts. | `source-graph.md` |
| Chunking | Select chunk profile and routing hints. | `chunking-contract.md` |
| Metadata | Define shared and source-specific metadata fields. | `metadata-payload.md` |
| Auth/secrets | Define credential requirements, scopes, and redaction rules. | `auth-contract.md`, `security-contract.md` |
| Observability | Emit standard phases, counts, heartbeats, warnings, and degradation. | `observability-contract.md` |
| Error handling | Define retryable, degraded, and terminal failure modes. | `error-handling.md` |
| Tests | Add resolver, adapter, parser, metadata, graph, and source-job fixtures. | `testing-contract.md` |
| Docs | Add source help, examples, capability docs, and generated schema coverage. | `documentation-contract.md` |

## Source Definition

Every source registers a `SourceAdapterSpec`.

```rust
pub struct SourceAdapterSpec {
    pub adapter: &'static str,
    pub version: &'static str,
    pub source_kinds: &'static [SourceKind],
    pub supported_schemes: &'static [&'static str],
    pub shorthand_patterns: &'static [&'static str],
    pub default_scope: SourceScope,
    pub scopes: &'static [SourceScopeCapability],
    pub credential_requirements: &'static [CredentialRequirement],
    pub option_schema: &'static str,
    pub parser_families: &'static [ParserFamily],
    pub metadata_families: &'static [&'static str],
}
```

Required rules:

- `adapter` is a stable snake_case name used in logs, payloads, docs, and
  capability output.
- `version` changes when emitted documents, metadata, graph facts, or parser
  behavior materially changes.
- `source_kinds` use closed `SourceKind` enum values.
- `supported_schemes` include explicit URI schemes or prefixes such as
  `github:`, `npm:`, `youtube:`, `mcp:`, or `file:`.
- `shorthand_patterns` must be unambiguous or return a resolution conflict.
- `option_schema` is generated and validated before acquisition.

## Resolver Requirements

The resolver turns user input into a `ResolvedSource`.

Required output:

| Field | Rule |
|---|---|
| `requested_uri` | Original caller input, preserved for diagnostics. |
| `source_canonical_uri` | Stable source root identity. |
| `source_id` | Derived from canonical identity and source kind. |
| `source_kind` | Closed enum value. |
| `adapter` | Selected adapter name. |
| `default_scope` | Adapter-declared default when caller omits scope. |
| `available_scopes` | Adapter-declared scopes. |
| `authority` | `official`, `user_pinned`, `inferred`, `community`, `mirror`, or `unknown`. |
| `confidence` | Resolver confidence from 0.0 to 1.0. |
| `warnings` | Ambiguity, fallback, or degraded resolution notes. |

Resolver behavior:

- Explicit schemes win over shorthand detection.
- Existing local paths win over host shorthand.
- Ambiguous input fails before network acquisition unless the caller provided
  adapter/scope hints.
- Network probes are bounded, observable, and disabled unless the resolver
  declares they are needed for that source kind.
- URL normalization must separate `source_canonical_uri` from
  `item_canonical_uri`.

## Scope Requirements

Each source exposes `SourceScopeCapability` rows.

Required fields:

| Field | Meaning |
|---|---|
| `name` | Scope enum/name. |
| `description` | Human and agent-readable behavior. |
| `embeds_by_default` | Whether vectors are written by default. |
| `watch_supported` | Whether durable watch can keep this scope fresh. |
| `refresh_supported` | Whether refresh can re-run this scope. |
| `requires_credentials` | Whether credentials are needed for private/high-rate access. |
| `may_access_local_paths` | Whether local filesystem reads are possible. |
| `may_perform_network_fetches` | Whether network access is possible. |
| `may_call_render_provider` | Whether browser rendering may be used. |
| `may_execute_tools` | Whether local/remote tools may be executed. |
| `accepts_uploads` | Whether staged uploads can satisfy this source. |
| `output_item_kind` | Primary item kind emitted. |
| `option_schema` | Scope-specific option schema. |
| `chunking_hints` | Preferred chunk profile/parser hints. |
| `required_graph_fact_kinds` | Required graph facts when structures are present. |
| `optional_graph_fact_kinds` | Opportunistic graph facts. |
| `degraded_modes` | Allowed degraded behavior. |

`map` scopes discover source items, links, resources, or members without
embedding. Watched map scopes refresh the candidate manifest and may create
child source jobs only when explicitly configured.

## Adapter Acquisition Contract

Adapters acquire source items and emit `SourceDocument`.

```rust
#[async_trait]
pub trait SourceAdapter {
    async fn resolve(&self, request: ResolveSourceRequest) -> Result<ResolvedSource>;
    async fn plan(&self, request: SourceRequest, resolved: ResolvedSource) -> Result<SourcePlan>;
    async fn acquire(&self, ctx: SourceAcquisitionContext) -> Result<SourceAcquisitionResult>;
}
```

Adapters must:

- emit deterministic `source_item_key` values
- emit `item_canonical_uri` for each item
- emit normalized content or binary metadata only
- emit content hash inputs used by ledger diffs
- populate title, content kind, language, path, MIME, timestamps, and source
  version when available
- report counts for total, done, skipped, failed, bytes, and documents
- classify item failures as retryable, degraded, skipped, or terminal
- avoid direct Qdrant, graph, job, transport, or CLI output writes

Adapters must not:

- build `PreparedDocument` directly
- chunk content directly except through declared parser/chunk hints
- write vector payloads
- persist their own source lifecycle tables
- hide credentials in metadata, logs, payloads, or artifacts
- silently drop malformed items without warnings/counts

## Ledger and Manifest Contract

Every refreshable source defines manifest behavior.

Required manifest fields:

| Field | Rule |
|---|---|
| `source_id` | Same id from resolver. |
| `source_generation` | Generation being written. |
| `source_item_key` | Stable key within source. |
| `item_canonical_uri` | Stable item identity. |
| `source_item_hash` | Hash of normalized item content or manifest payload. |
| `source_item_size_bytes` | Size when known. |
| `source_item_mtime` | Source-reported mtime when known. |
| `source_item_status` | `added`, `modified`, `unchanged`, `removed`, `skipped`, `failed`. |

Diff rules:

- unchanged items are not re-embedded
- added and modified items are prepared and embedded
- removed items create cleanup debt
- failed items keep item-level error state
- commit happens only after publish rules succeed
- cleanup debt is durable and retryable

Immutable scopes, such as a Git commit or specific package version, may skip
generation replacement when the canonical URI includes an immutable version.
They still use ledger rows for accounting, metadata joins, and cleanup.

## Parsing and Chunking Contract

A new source declares one of:

- parser-backed content with `SourceParseFacts`
- graph-only structured content
- plain document content with default markdown/text chunking
- binary metadata content with no embedded body

Parser requirements:

- parser output is deterministic for the same normalized input
- parse facts are stored separately from raw content
- parse failures degrade the item only when parsing is optional
- required parser failures fail or degrade according to the scope contract
- parser facts feed both graph extraction and chunk routing

Chunking requirements:

- adapter emits `SourceDocument`
- `DocumentPreparer` chooses chunker through `ChunkRouter`
- code uses AST/tree-sitter or source-aware parser when available
- manifests and schemas use structured parsers, not raw markdown chunking
- transcripts and sessions chunk by turn/time/message boundaries
- tool outputs chunk by invocation/result sections

## Graph Contract

New sources must declare graph outputs even when the first version emits none.

Required declarations:

| Field | Rule |
|---|---|
| `required_graph_fact_kinds` | Facts that must be extracted when structures exist. |
| `optional_graph_fact_kinds` | Useful enrichment facts. |
| `graph_node_kinds` | Node kinds emitted by parser/adapter. |
| `graph_edge_kinds` | Edge kinds emitted by parser/adapter. |
| `evidence_kinds` | Evidence rows proving each edge. |
| `authority_rules` | How official/user-pinned/inferred evidence is ranked. |

Graph candidates must include:

- source id and job id
- item key and item canonical URI
- parser/adapter name and version
- node/edge kind
- confidence
- evidence value and evidence source
- observed timestamp

Graph extraction must never invent authoritative links without evidence.

## Metadata and Payload Contract

Every new source adds source-specific metadata to
`metadata-payload.md` before implementation.

Required metadata families:

| Family | Required When |
|---|---|
| shared `source_*` | every source |
| shared `document_*` | every source document |
| shared `chunk_*` | every prepared chunk |
| shared `embedding_*` | every embedded chunk |
| shared `vector_*` | every vector point |
| shared `graph_*` | every graph-linked document/chunk |
| source-specific prefix | source family fields |

Rules:

- public vector payload fields must be redacted and bounded
- source-specific fields use approved prefixes
- every payload joins back to ledger by `source_id`, `source_item_key`,
  `document_id`, `chunk_id`, and `generation`
- Qdrant payload indexes are declared before collection creation
- raw adapter response blobs are artifacts, not vector payload fields

## Auth, Credentials, and Security

Every new source declares credential behavior.

Required declarations:

| Field | Meaning |
|---|---|
| `credential_kind` | token, OAuth, basic, cookie, local secret, none. |
| `env_keys` | Required/optional env secret names. |
| `config_keys` | Non-secret config options. |
| `auth_scope` | Required Axon caller scope. |
| `network_policy` | Allowed hosts, redirects, private-network policy. |
| `local_path_policy` | Allowed local roots or path prompts. |
| `tool_execution_policy` | Whether tools/commands may execute. |
| `redaction_policy` | Fields/content that must be scrubbed. |

Secrets stay in `.env` or `CredentialProvider`. Source options may reference a
credential id, but must not include raw secrets in `SourceRequest` unless the
request is a trusted local-only call that is immediately redacted before
persistence.

## Observability Contract

New sources emit the shared progress shape.

Required phases:

```text
resolving
routing
authorizing
planning
discovering
diffing
fetching/rendering/tooling
normalizing
parsing
graphing
preparing
embedding
publishing
cleaning
complete
```

Required metrics/counts:

- items total/done/skipped/failed
- documents total/done/failed
- chunks total/done/failed
- bytes total/done when knowable
- provider wait/cooling time when blocked
- retries and degraded item count

Every warning/error includes source id, job id, phase, item key when available,
retryability, severity, and visibility.

## Testing Contract

Every new source ships fixtures and tests before being considered online.

Required fixtures:

```text
crates/axon-adapters/fixtures/<adapter>/resolve/*.json
crates/axon-adapters/fixtures/<adapter>/manifest/*.json
crates/axon-adapters/fixtures/<adapter>/source-documents/*.json
crates/axon-parse/fixtures/<adapter>/*.json
crates/axon-graph/fixtures/<adapter>/*.json
crates/axon-vectors/tests/fixtures/payload/<adapter>.valid.json
```

Required tests:

- resolver accepts explicit URI and shorthand forms
- resolver rejects ambiguous shorthand
- adapter capability schema validates
- map scope discovers items without embedding
- source scope emits `SourceDocument`
- manifest diff detects added, modified, removed, unchanged
- parser emits expected `SourceParseFacts`
- graph candidates validate against graph schema
- metadata payload validates and redacts secrets
- source job emits progress events and heartbeats
- provider failures degrade or fail according to policy
- watch refresh reuses ledger state and avoids full re-embed for unchanged items

## Documentation Contract

Every new source updates:

- `adapter-scopes.md`
- `url-normalization.md`
- `metadata-payload.md`
- `source-graph.md`
- generated adapter capability docs
- CLI help examples when the source has human-facing shorthand
- REST/MCP capability output fixtures
- source-specific guide under final `docs/guides/` when user-facing behavior is
  non-obvious

## Acceptance Criteria

- source resolves from explicit URI and documented shorthand
- `axon <source> --scope map --no-embed` discovers items without vectors
- `axon <source> --wait` produces committed documents and vector payloads
- `axon <source> --watch` creates a durable watch when supported
- unchanged refresh skips re-embedding
- removed items create cleanup debt
- graph candidates are emitted or explicitly unsupported
- all emitted metadata validates against the payload schema
- secrets are redacted in logs, events, artifacts, and vector payloads
- generated CLI/MCP/REST capability docs include the new source
- source-specific fixtures cover success, degraded, auth failure, and provider
  failure paths
