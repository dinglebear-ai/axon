# Adding A Source

Last Modified: 2026-07-19

New source families enter through one pipeline. There is no per-family command,
MCP action, or REST route — every source routes through `SourceRequest`.

> Contract source:
> [`docs/pipeline-unification/sources/new-source-contract.md`](../../pipeline-unification/sources/new-source-contract.md).
> Dev walkthrough: [`docs/development/adding-source-adapter.md`](../../development/adding-source-adapter.md)
> (+ end-to-end checklist at [`adding-source.md`](../../development/adding-source.md)).

## The one pipeline path

```text
SourceRequest → SourceResolver → SourceRouter → SourceAdapter
  → SourceLedger (manifest/diff/generation)
  → SourceDocument → SourceParseFacts / GraphCandidate
  → DocumentPreparer → EmbeddingProvider → VectorStore
  → DocumentStatus → graph write → cleanup debt
```

No source may bypass this by writing `PreparedDocument`, vector points, graph
rows, or transport responses directly.

## What to implement

| Step | Where | What |
|---|---|---|
| Identity | `crates/axon-api/src/source/enums.rs` | add a `SourceKind` variant (closed enum — do not invent a string kind) |
| Resolve + route | `crates/axon-route/src/` | source id, canonical URI, scope selection |
| Family spec | `crates/axon-adapters/src/spec.rs` + `family_matrix.rs` | add a `SourceFamily` variant + a `SourceAdapterSpec` row in `MATRIX` |
| Adapter | `crates/axon-adapters/src/adapter.rs` + a new module | implement `SourceAdapter` |
| Registry | `crates/axon-adapters/src/registry.rs` | register so the router can find it |
| Scopes | `crates/axon-adapters/src/family_matrix.rs` | declare `<FAMILY>_SCOPES: &[SourceScopeCapability]` |
| Fixtures | `crates/axon-adapters/fixtures/<family>/`, `axon-parse/fixtures/`, `axon-graph/fixtures/`, `axon-vectors/tests/fixtures/payload/` | resolve, manifest, source-documents, parse, graph, payload |

## The `SourceAdapter` trait

```rust
#[async_trait]
pub trait SourceAdapter: Send + Sync {
    fn name(&self) -> &str;
    fn version(&self) -> &str;
    async fn capabilities(&self) -> Result<AdapterCapability>;
    async fn discover(&self, plan: &SourcePlan) -> Result<SourceManifest>;
    async fn acquire(&self, plan: &SourcePlan, diff: &SourceManifestDiff) -> Result<SourceAcquisition>;
    async fn normalize(&self, plan: &SourcePlan, acquisition: SourceAcquisition)
        -> Result<StageExecutionResult<Vec<SourceDocument>>>;
}
```

`discover` builds the full manifest (the ledger diffs it against the prior
generation); `acquire` fetches only items the diff marks changed; `normalize`
produces `SourceDocument` values — the single shared shape every downstream
stage consumes. Adapters never emit `PreparedDocument` or vector points.

## The `SourceAdapterSpec` (capability + safety declaration)

Beyond identity, the spec declares the safety-relevant booleans that gate real
policy — **under-declaring is a security bug**:

- `may_access_local_paths`, `may_perform_network_fetches`,
  `may_call_render_provider`, `may_execute_tools`
- `watch_supported`, `refresh_supported`, `vector_namespace`
- per-scope `SourceScopeCapability` rows (18 fields each, including
  `requires_credentials`, `option_schema`, `chunking_hints`,
  `required_graph_fact_kinds`, `degraded_modes`)

`option_schema: &'static str` on both spec and scope is generated and validated
before acquisition.

## Reference adapters to copy

| Family | File | Notes |
|---|---|---|
| `Local` | `crates/axon-adapters/src/local.rs` | filesystem; simplest |
| `Git` | `crates/axon-adapters/src/git.rs` | GitHub/GitLab/Gitea/generic |
| `Feed` | `crates/axon-adapters/src/feed.rs` | smallest network example |

Each has a sidecar `_tests.rs` and fixtures under
`crates/axon-adapters/fixtures/<family>/`.

## Onboarding checklist

`crates/axon-adapters/src/onboarding.rs::onboarding_status()` mechanically
checks: identity, resolver, router, adapter, scopes, ledger, parsing, graph,
chunking, metadata, auth_secrets, observability, error_handling, tests, docs.
It checks declared non-emptiness (e.g. `source_kinds` non-empty, credential
requirements carry a `reason`), not correctness.

## Generated docs

Run `cargo xtask schemas generate` after touching a spec — capability docs and
the vector-payload / provider-capability schemas are generated, not hand-written.

## Rule

Do not add a new top-level command, MCP action, or REST route for each source
family. Route through `SourceRequest` unless the surface is a deliberate
transport projection (`scrape`, `map`).

If a new source family lands, update this file, the family matrix, and the
generated schemas in the same PR.
