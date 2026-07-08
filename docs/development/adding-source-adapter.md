# Adding a Source Adapter

A source adapter turns a `ResolvedSource` into `SourceDocument` values without
bypassing the shared pipeline. This guide describes the real pattern used by
`axon-adapters` (`crates/axon-adapters/src/`) — the source-family matrix,
the `SourceAdapter` trait, and the onboarding checklist a new family must
satisfy.

See also: crate guide `crates/axon-adapters/src/CLAUDE.md`, behavior contract
`docs/pipeline-unification/sources/adapter-scopes.md`, onboarding contract
`docs/pipeline-unification/sources/new-source-contract.md`.

## Where a new family fits in the pipeline

```text
SourceRequest -> SourceResolver -> SourceRouter -> SourceAdapter
  -> SourceLedger -> SourceDocument -> SourceParseFacts / GraphCandidate
  -> DocumentPreparer -> EmbeddingProvider -> VectorStore -> DocumentStatus
```

Adapters emit `SourceDocument` **only** — they never write prepared
documents, vectors, graph rows, jobs, or transport responses directly. Ledger
persistence, chunking, embedding, and vector writes happen downstream in the
shared pipeline (`axon-services::source::index_source_with_auth` and
friends), not inside the adapter.

## Step 1: Add a `SourceFamily` variant

`crates/axon-adapters/src/spec.rs` defines `SourceFamily` (the enum every
family variant belongs to — `Local`, `Git`, `Web`, `Feed`, `Youtube`,
`Reddit`, `Sessions`, `Registry`, `CliTool`, `McpTool`,
`MemoryIntegration`) and `SourceAdapterSpec`, the struct that fully describes
one family's declared capabilities:

```rust
pub struct SourceAdapterSpec {
    pub family: SourceFamily,
    pub adapter: &'static str,
    pub version: &'static str,
    pub source_kinds: &'static [SourceKind],
    pub vector_namespace: &'static str,
    pub supported_schemes: &'static [&'static str],
    pub shorthand_patterns: &'static [&'static str],
    pub default_scope: SourceScope,
    pub scopes: &'static [SourceScopeCapability],
    pub credential_requirements: &'static [CredentialRequirement],
    pub option_schema: &'static str,
    pub parser_families: &'static [ParserFamily],
    pub metadata_families: &'static [&'static str],
    pub watch_supported: bool,
    pub refresh_supported: bool,
    pub may_access_local_paths: bool,
    pub may_perform_network_fetches: bool,
    pub may_call_render_provider: bool,
    pub may_execute_tools: bool,
    pub is_source_adapter: bool,
    pub degraded_modes: &'static [&'static str],
    pub required_graph_fact_kinds: &'static [&'static str],
    pub optional_graph_fact_kinds: &'static [&'static str],
}
```

Add your new family to `SourceFamily`, then declare a `const <FAMILY>_SCOPES:
&[SourceScopeCapability]` list (each scope marked `required: bool` with a
`notes` string) and add a new `SourceAdapterSpec` entry to the `MATRIX` const
in `crates/axon-adapters/src/family_matrix.rs`, returned by
`source_family_matrix()`. Model the security-relevant booleans honestly —
`may_access_local_paths`, `may_perform_network_fetches`,
`may_call_render_provider`, and `may_execute_tools` gate real security
policy elsewhere (SSRF checks, local-path trust, tool-exec allowlists), so
under-declaring them is a security bug, not just a docs gap.

## Step 2: Implement `SourceAdapter`

`crates/axon-adapters/src/adapter.rs` defines the trait:

```rust
#[async_trait]
pub trait SourceAdapter: Send + Sync {
    fn name(&self) -> &str;
    fn version(&self) -> &str;
    async fn capabilities(&self) -> Result<AdapterCapability>;
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
}
```

- **`discover`** builds the full `SourceManifest` for the current state of the
  source (every item the source currently contains, keyed by
  `source_item_key`) — this is what the ledger diffs against the previously
  committed generation.
- **`acquire`** fetches only the items the manifest diff says changed, given
  the diff computed from `discover`'s output.
- **`normalize`** turns acquired raw content into `SourceDocument` values —
  the shared shape every downstream stage (parsing, chunking, embedding)
  consumes regardless of source family.

Look at an existing family's implementation for the real pattern before
writing a new one — `crates/axon-adapters/src/local.rs` (filesystem),
`crates/axon-adapters/src/git.rs` (repository), `crates/axon-adapters/src/
feed.rs` (RSS/Atom/JSON) are all live, non-stub implementations with sidecar
test files (`local_tests.rs`, `git_tests.rs`, `feed_tests.rs`).

## Step 3: Register the adapter

`crates/axon-adapters/src/registry.rs` (`AdapterRegistry`) is the
registration/lookup boundary the router uses to find the right adapter for a
resolved source. Register your new adapter implementation there.

## Step 4: Satisfy the onboarding checklist

`crates/axon-adapters/src/onboarding.rs` derives a `SourceOnboardingStatus`
from a `SourceAdapterSpec` — 15 rows (`identity`, `resolver`, `router`,
`adapter`, `scopes`, `ledger`, `parsing`, `graph`, `chunking`, `metadata`,
`auth_secrets`, `observability`, `error_handling`, `tests`, `docs`), each
either complete or not based on whether the spec declares the corresponding
fields non-empty. This is a **derived, mechanical check** — it doesn't
validate correctness, only that the spec isn't obviously incomplete
(`source_kinds` non-empty, `parser_families` non-empty, credential
requirements carry a `reason`, etc.). Run
`onboarding_status(spec)`/`onboarding_rows(status)` against your new spec
before considering a family "onboarded"; a family with any incomplete row is
not ready for the family-matrix contract tests.

## Step 5: Add fixtures and tests

Add a sidecar `<family>_tests.rs` per the repo's test convention (declared
via `#[path]` in the family's source file). The Phase 9 family-matrix
contract expects, per family: resolver/adapter/parser/graph/metadata/vector
payload/source-job/degraded/auth/provider-failure fixtures where applicable.
Existing families' fixture trees under `crates/axon-adapters/fixtures/
<family>/` are the concrete pattern to copy.

## Step 6: Update generated docs/schemas

Adapter capability docs and schemas are generated, not hand-written — run
`cargo xtask schemas generate` after adding or changing a
`SourceAdapterSpec` so the generated capability docs and
`schemas/provider-capability-schema.md`/`schemas/vector-payload-schema.md`
stay in sync with the matrix.

## Boundary reminders

- Source id / canonical URI construction belongs to `axon-route`, not the
  adapter.
- Ledger persistence, generation publishing, final chunking, embedding,
  vector writes, and search/RAG do not belong in `axon-adapters`.
- No direct Qdrant upserts, embedding-provider calls, or job-store ownership
  from inside an adapter.
- No CLI/MCP/REST rendering from inside an adapter.
- Allowed dependencies: `axon-api`, `axon-error`, `axon-core`, `axon-route`,
  `axon-authz`, `axon-observe`, and acquisition libraries (HTTP/git/feed/
  transcript/archive/tool clients) hidden behind the adapter implementation.
  Forbidden: `axon-vectors`/`axon-embedding`/`axon-retrieval`/`axon-services`,
  direct job store, transport crates — enforced by
  `cargo xtask check-layering`.
