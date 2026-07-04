# Tool Source Families Matrix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Finish Phase 9 by putting local, git, web, feed, YouTube, Reddit, sessions, registry, CLI tool/script, and MCP tool/call sources behind the unified source-family matrix and adapter fixture contract.

**Architecture:** Implement a source-family matrix first, then enforce resolver, adapter, parser, graph, metadata, vector payload, source-job, degraded, auth, and provider-failure fixtures for families accepted by the public router or changed by this cutover. Exhaustive fixture variants can be release hardening, but SSRF/local/tool-exec/redaction fixtures are non-deferrable for any public source family.

**Tech Stack:** Rust 2024, `axon-api`, `axon-adapters`, `axon-services`, `axon-parse`, `axon-graph`, `axon-vectors`, `axon-authz`, `axon-core` security/redaction, SQLite fakes, Qdrant fakes, schema generator.

## Engineering Review Corrections

The Lavra engineering review found that the original plan could overreach into all-family completion while under-specifying the public-family security gates. This revision makes the matrix and touched/public-family enforcement the Phase 9 scope, requires shared route-time capability data, keeps SSRF/local/tool-exec/redaction gates non-deferrable, adds batching/backpressure proof, and replaces stale schema commands with `cargo xtask schemas generate`.

## Global Constraints

- Source of truth: `docs/pipeline-unification/sources/new-source-contract.md`, `adapter-scopes.md`, `url-normalization.md`, `metadata-payload.md`, `source-graph.md`, `runtime/security-contract.md`, and `delivery/testing-contract.md`.
- Every source enters through `SourceRequest -> SourceResolver -> SourceRouter -> SourceAdapter -> SourceLedger -> SourceDocument -> SourceParseFacts / GraphCandidate -> DocumentPreparer -> EmbeddingProvider -> VectorStore -> DocumentStatus`.
- Adapters emit `SourceDocument` only; they do not write prepared documents, vectors, graph rows, jobs, or transport responses directly.
- New or touched sources must complete onboarding rows for identity, resolver, router, adapter, scopes, ledger, parsing, graph, chunking, metadata, auth/secrets, observability, error handling, tests, and docs.
- Tool execution sources default to metadata-only/no-exec and require explicit opt-in, allowlists, env allowlists, timeout/output caps, audit metadata, and redaction before writes.
- Network/render sources must enforce SSRF checks before HTTP or Chrome access.
- Local sources require `axon:local` or trusted local context and redact absolute paths from public payloads.
- The matrix must share one generated/compiled source of truth with route-time adapter capability data. Do not let `SourceFamilyMatrix` drift from `axon-route` or generated capability docs.
- Keep Phase 9 as matrix plus touched/public-family enforcement. Do not require full porting of every listed family in one PR.
- CLI tool/script and MCP tool/call adapters are separate security-sensitive slices unless this plan explicitly implements their no-exec/redaction/allowlist gates.
- Fixture packs must be minimal required packs plus generated matrix manifests. Exhaustive variants are hardening, but public SSRF/local/tool-exec/redaction gates are mandatory.
- Adapter batching tests must prove streaming/backpressure and must not permit collecting all items before prepare/embed/vector/graph stages.
- Use `cargo xtask schemas generate`, not `cargo xtask generate-schemas`.
- `cargo xtask check` is a final cutover gate, not normal Phase 9 task-loop verification.

---

## File Structure

- Create: `crates/axon-adapters/src/family_matrix.rs`
- Create: `crates/axon-adapters/src/spec.rs`
- Modify: `crates/axon-adapters/src/lib.rs`
- Modify: `crates/axon-adapters/src/{local,git,web,feed,youtube,reddit,sessions,registry,tool,mcp}.rs` as those modules exist or are created by the port.
- Create: `crates/axon-adapters/fixtures/<family>/{resolve,manifest,source-documents,auth,degraded,provider-failure}/*.json`
- Modify: `crates/axon-services/src/source.rs`
- Modify: `crates/axon-core/src/security.rs` or the current security policy module.
- Modify: `crates/axon-parse/fixtures/<family>/*.json`
- Modify: `crates/axon-graph/fixtures/<family>/*.json`
- Modify: `crates/axon-vectors/tests/fixtures/payload/<family>.valid.json`
- Modify: generated adapter capability docs through `cargo xtask schemas generate`.

---

### Task 1: Source-Family Matrix Registry

**Files:**
- Create: `crates/axon-adapters/src/family_matrix.rs`
- Create: `crates/axon-adapters/src/spec.rs`
- Modify: `crates/axon-adapters/src/lib.rs`
- Test: `crates/axon-adapters/src/family_matrix_tests.rs`

**Interfaces:**
- Produces: `SourceFamily`, `SourceAdapterSpec`, `SourceFamilyMatrix`, `source_family_matrix() -> &'static [SourceAdapterSpec]`.

- [ ] **Step 1: Add failing matrix completeness test**

```rust
#[test]
fn matrix_contains_required_source_families() {
    let families = source_family_matrix().iter().map(|spec| spec.family).collect::<BTreeSet<_>>();
    for expected in [
        SourceFamily::Local,
        SourceFamily::Git,
        SourceFamily::Web,
        SourceFamily::Feed,
        SourceFamily::Youtube,
        SourceFamily::Reddit,
        SourceFamily::Sessions,
        SourceFamily::Registry,
        SourceFamily::CliTool,
        SourceFamily::McpTool,
    ] {
        assert!(families.contains(&expected), "missing {expected:?}");
    }
}
```

- [ ] **Step 2: Run the failing test**

Run: `cargo test -p axon-adapters family_matrix --no-fail-fast`

Expected: compile failure if `axon-adapters` or the matrix types are incomplete; otherwise failure for missing family rows.

- [ ] **Step 3: Implement adapter spec shape**

Implement the contract fields:

```rust
pub struct SourceAdapterSpec {
    pub family: SourceFamily,
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
    pub watch_supported: bool,
    pub refresh_supported: bool,
    pub may_access_local_paths: bool,
    pub may_perform_network_fetches: bool,
    pub may_call_render_provider: bool,
    pub may_execute_tools: bool,
    pub degraded_modes: &'static [&'static str],
    pub required_graph_fact_kinds: &'static [&'static str],
    pub optional_graph_fact_kinds: &'static [&'static str],
}
```

- [ ] **Step 4: Add conservative initial rows**

Populate one row per required family. Rows may declare unsupported scopes, but every row must have a stable adapter name/version, source kinds, schemes, default scope, metadata families, security capability flags, degraded modes, and graph fact declarations.

- [ ] **Step 5: Run matrix tests**

Run: `cargo test -p axon-adapters family_matrix --no-fail-fast`

Expected: matrix rows validate and every required family exists.

---

### Task 2: Onboarding Checklist Enforcement

**Files:**
- Create: `crates/axon-adapters/src/onboarding.rs`
- Test: `crates/axon-adapters/src/onboarding_tests.rs`
- Modify: `xtask/src/schemas.rs` or the adapter schema family module if already split.

**Interfaces:**
- Consumes: `SourceAdapterSpec`.
- Produces: `SourceOnboardingStatus` with rows matching `sources/new-source-contract.md`.

- [ ] **Step 1: Add failing onboarding row test**

```rust
#[test]
fn touched_families_have_all_onboarding_rows() {
    for spec in source_family_matrix().iter().filter(|spec| spec.touched_in_cutover) {
        let status = onboarding_status(spec);
        assert!(status.identity.complete);
        assert!(status.resolver.complete);
        assert!(status.router.complete);
        assert!(status.adapter.complete);
        assert!(status.scopes.complete);
        assert!(status.ledger.complete);
        assert!(status.parsing.complete);
        assert!(status.graph.complete);
        assert!(status.chunking.complete);
        assert!(status.metadata.complete);
        assert!(status.auth_secrets.complete);
        assert!(status.observability.complete);
        assert!(status.error_handling.complete);
        assert!(status.tests.complete);
        assert!(status.docs.complete);
    }
}
```

- [ ] **Step 2: Run the failing test**

Run: `cargo test -p axon-adapters onboarding --no-fail-fast`

Expected: touched families without complete rows fail.

- [ ] **Step 3: Implement status derivation**

Derive onboarding status from adapter specs, fixture presence, metadata registry entries, graph declarations, parser family declarations, auth/security flags, and generated capability docs.

- [ ] **Step 4: Record release-hardening exceptions**

Create machine-readable exceptions for untouched families:

```json
{
  "family": "youtube",
  "reason": "not touched by this cutover implementation",
  "follow_up": "full all-family fixture completeness"
}
```

Exceptions for exhaustive variants are allowed only when `touched_in_cutover=false`. Security fixtures for SSRF, local path containment, tool execution, and redaction are mandatory for any family accepted by the public router.

- [ ] **Step 5: Run onboarding tests**

Run: `cargo test -p axon-adapters onboarding --no-fail-fast`

Expected: touched families complete; untouched families have explicit hardening exceptions.

---

### Task 3: Resolver Adapter Parser Graph Fixture Packs

**Files:**
- Create/modify: `crates/axon-adapters/fixtures/<family>/resolve/*.json`
- Create/modify: `crates/axon-adapters/fixtures/<family>/manifest/*.json`
- Create/modify: `crates/axon-adapters/fixtures/<family>/source-documents/*.json`
- Create/modify: `crates/axon-parse/fixtures/<family>/*.json`
- Create/modify: `crates/axon-graph/fixtures/<family>/*.json`
- Create/modify: `crates/axon-vectors/tests/fixtures/payload/<family>.valid.json`
- Test: `crates/axon-adapters/src/fixture_tests.rs`

**Interfaces:**
- Consumes: adapter specs and testing contract fixture layout.
- Produces: fixture validation for every touched family.

- [ ] **Step 1: Add fixture presence test**

```rust
#[test]
fn touched_families_have_required_fixture_packs() {
    for spec in source_family_matrix().iter().filter(|spec| spec.touched_in_cutover) {
        assert_fixture_dir(spec.adapter, "resolve");
        assert_fixture_dir(spec.adapter, "manifest");
        assert_fixture_dir(spec.adapter, "source-documents");
        assert_parse_fixture(spec.adapter);
        assert_graph_fixture(spec.adapter);
        assert_vector_payload_fixture(spec.adapter);
    }
}
```

- [ ] **Step 2: Run test**

Run: `cargo test -p axon-adapters fixture_packs --no-fail-fast`

Expected: missing fixture packs fail.

- [ ] **Step 3: Add fixtures for changed families**

For each touched family, add:

```text
resolve/explicit.valid.json
resolve/shorthand.valid.json
resolve/ambiguous.invalid.json
manifest/added-modified-removed.valid.json
source-documents/minimal.valid.json
auth/missing-scope.invalid.json
degraded/optional-provider.valid.json
provider-failure/required-provider.invalid.json
```

- [ ] **Step 4: Validate fixture content**

Each fixture must include source id, canonical URI, adapter name/version, source item key, item canonical URI, metadata family, graph declarations, redaction status, and expected degraded/error code where applicable.

- [ ] **Step 5: Run fixture tests**

Run: `cargo test -p axon-adapters fixtures --no-fail-fast`

Expected: touched fixture packs parse and validate.

---

### Task 4: Source Adapter Batching

**Files:**
- Modify: `crates/axon-services/src/source.rs`
- Modify: `crates/axon-adapters/src/*`
- Modify: `crates/axon-vectors/src/*`
- Modify: `crates/axon-graph/src/*`
- Test: `crates/axon-services/src/source_batch_tests.rs`

**Interfaces:**
- Produces: bounded prepare/embed/vector/graph write batches for source-family ports.

- [ ] **Step 1: Add failing batching test**

```rust
#[tokio::test]
async fn source_pipeline_batches_prepare_embed_vector_and_graph_writes() {
    let harness = source_pipeline_harness().with_batch_size(3);
    harness.index_fixture_items("web", 8).await.unwrap();
    assert_eq!(harness.prepare_batch_sizes(), vec![3, 3, 2]);
    assert_eq!(harness.embedding_batch_sizes(), vec![3, 3, 2]);
    assert_eq!(harness.vector_upsert_batch_sizes(), vec![3, 3, 2]);
    assert_eq!(harness.graph_write_batch_sizes(), vec![3, 3, 2]);
}
```

- [ ] **Step 2: Run batching test**

Run: `cargo test -p axon-services source_batch --no-fail-fast`

Expected: test fails if item-by-item writes remain in touched source ports.

- [ ] **Step 3: Implement batch boundaries**

Source adapters emit item streams; the service layer chunks them into bounded batches before preparing, embedding, vector upsert, and graph write. Batch events include batch id, item counts, chunk counts, byte counts, provider reservation ids, and elapsed time.

- [ ] **Step 4: Run batching tests**

Run: `cargo test -p axon-services source_batch --no-fail-fast`

Expected: touched source paths use bounded batches.

---

### Task 5: SSRF And Local Policy Fixtures

**Files:**
- Create: `crates/axon-adapters/fixtures/security/ssrf/*.json`
- Create: `crates/axon-adapters/fixtures/security/local/*.json`
- Modify: `crates/axon-core/src/security.rs`
- Modify: `crates/axon-services/src/source.rs`
- Test: `crates/axon-services/src/source_security_tests.rs`

**Interfaces:**
- Consumes: `SecurityPolicy`, `AuthSnapshot`.
- Produces: SSRF/local policy parity for web/feed/video/registry and local sources.

- [ ] **Step 1: Add failing security fixture tests**

```rust
#[tokio::test]
async fn network_sources_deny_private_redirects_for_http_and_chrome() {
    for render_mode in [RenderMode::Http, RenderMode::Chrome] {
        let err = run_source_fixture("security/ssrf/redirect-private-ip.invalid.json", render_mode).await.unwrap_err();
        assert_eq!(err.code.to_string(), "security.ssrf_denied");
    }
}

#[tokio::test]
async fn local_source_denies_secret_paths_without_local_scope() {
    let err = run_local_fixture_without_scope("security/local/env-file.invalid.json").await.unwrap_err();
    assert_eq!(err.code.to_string(), "auth.scope_required");
}
```

- [ ] **Step 2: Run security tests**

Run: `cargo test -p axon-services source_security --no-fail-fast`

Expected: failures identify missing SSRF or local policy enforcement.

- [ ] **Step 3: Add required fixtures**

Network fixtures cover private IPs, redirects, DNS rebinding, loopback, link-local, `file:` schemes, and Chrome/render-provider parity. Local fixtures cover `axon:local`, symlink-resolved containment, default denies for `.env`, SSH/cloud/Codex/Gemini/browser-profile paths, and absolute-path redaction.

- [ ] **Step 4: Enforce before side effects**

Run security policy before fetch, render, local read, tool execution, artifact write, vector write, graph write, or job child creation.

- [ ] **Step 5: Run policy tests**

Run: `cargo test -p axon-services source_security --no-fail-fast`

Expected: fixtures pass and no denied fixture produces side effects.

---

### Task 6: CLI Tool Script Source Adapter

**Files:**
- Create/modify: `crates/axon-adapters/src/tool.rs`
- Create: `crates/axon-adapters/fixtures/tool/*`
- Modify: `crates/axon-services/src/source.rs`
- Test: `crates/axon-adapters/src/tool_tests.rs`

**Interfaces:**
- Produces: metadata-only/no-exec default and explicit execution path for CLI tool/script sources.

- [ ] **Step 1: Add failing CLI tool adapter tests**

```rust
#[tokio::test]
async fn cli_tool_defaults_to_metadata_only() {
    let result = resolve_and_acquire("tool:rg --help", ExecutionMode::MetadataOnly).await.unwrap();
    assert_eq!(result.documents.len(), 1);
    assert_eq!(result.execution_count, 0);
}

#[tokio::test]
async fn cli_tool_exec_requires_execute_scope_and_allowlist() {
    let err = run_tool_without_execute_scope("tool:rg --help").await.unwrap_err();
    assert_eq!(err.code.to_string(), "auth.scope_required");
    let err = run_tool_with_disallowed_command("tool:sh -c env").await.unwrap_err();
    assert_eq!(err.code.to_string(), "tool.command_denied");
}
```

- [ ] **Step 2: Run tool tests**

Run: `cargo test -p axon-adapters tool --no-fail-fast`

Expected: missing adapter or policy behavior fails.

- [ ] **Step 3: Implement adapter behavior**

The adapter stores command, argv, env allowlist, side-effect class, timeout, output cap, redacted stdout/stderr artifact refs, and audit metadata. It never stores shell-expanded strings as authority; shell scripts require an explicit shell-script source class.

- [ ] **Step 4: Run tool adapter tests**

Run: `cargo test -p axon-adapters tool --no-fail-fast`

Expected: metadata-only and explicit execution policy tests pass.

---

### Task 7: MCP Tool Call Source Adapter

**Files:**
- Create/modify: `crates/axon-adapters/src/mcp.rs`
- Create: `crates/axon-adapters/fixtures/mcp/*`
- Modify: `crates/axon-services/src/source.rs`
- Test: `crates/axon-adapters/src/mcp_tests.rs`

**Interfaces:**
- Produces: MCP server/tool schema and optional tool-call source behavior.

- [ ] **Step 1: Add failing MCP adapter tests**

```rust
#[tokio::test]
async fn mcp_tool_source_indexes_schema_without_calling_by_default() {
    let result = resolve_and_acquire("mcp://server/tool", ExecutionMode::MetadataOnly).await.unwrap();
    assert_eq!(result.tool_call_count, 0);
    assert!(result.documents.iter().any(|doc| doc.content_kind == ContentKind::Structured));
}

#[tokio::test]
async fn mcp_tool_call_requires_execute_scope_and_redacts_output() {
    let result = run_mcp_tool_with_execute_scope("mcp://server/tool", secret_output_fixture()).await.unwrap();
    assert_eq!(result.redaction_status, RedactionStatus::Redacted);
    assert!(!result.vector_payload_contains("authorization"));
}
```

- [ ] **Step 2: Run MCP adapter tests**

Run: `cargo test -p axon-adapters mcp --no-fail-fast`

Expected: missing adapter or redaction behavior fails.

- [ ] **Step 3: Implement adapter behavior**

MCP adapter supports server schema discovery, tool metadata, optional tool call execution with explicit opt-in, auth snapshot enforcement, output cap, artifact refs, redacted stdout/stderr-equivalent payloads, and external-resource graph nodes.

- [ ] **Step 4: Run MCP adapter tests**

Run: `cargo test -p axon-adapters mcp --no-fail-fast`

Expected: metadata-only, explicit execution, and redaction tests pass.

---

### Task 8: Capability Schema Regeneration

**Files:**
- Modify: `xtask/src/schemas.rs`
- Modify generated docs under `docs/reference/sources/*`
- Test: `xtask/src/schemas/tests.rs`

**Interfaces:**
- Consumes: adapter matrix and onboarding status.
- Produces: regenerated CLI/MCP/REST capability docs and schemas.

- [ ] **Step 1: Add generated capability drift test**

```rust
#[test]
fn generated_capabilities_include_source_family_matrix() {
    let artifact = generate_adapter_capability_artifact().unwrap();
    assert!(artifact.content.contains("\"adapter\":\"cli_tool\""));
    assert!(artifact.content.contains("\"adapter\":\"mcp_tool\""));
    assert!(artifact.content.contains("\"may_execute_tools\""));
}
```

- [ ] **Step 2: Run generator checks**

Run: `cargo xtask schemas generate`

Expected: generator writes updated artifacts if stale.

- [ ] **Step 3: Run final Phase 9 checks**

Run:

```bash
cargo test -p axon-adapters --no-fail-fast
cargo test -p axon-services source --no-fail-fast
cargo xtask schemas generate --check
```

Expected: all touched source families and generated capability docs pass. `cargo xtask check` remains a final cutover gate, not a Phase 9 task-loop check.
