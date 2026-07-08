# Surface Drift Generated Artifacts Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Finish Phase 10 by deleting removed public surfaces only after generated absence checks, schema fixtures, help output, and generated clients prove the clean-break contract.

**Architecture:** Build the removal checks first, then delete stale CLI/MCP/REST/DTO/config/generated-client surfaces. Every schema family is generated from declared inputs and includes source input manifests/checksums; `--check` mode fails without writing.

**Tech Stack:** Rust 2024, clap, RMCP schema, Axum/OpenAPI, `xtask` schema generator, generated web/Palette/Android assets, JSON Schema fixtures, golden snapshots.

## Engineering Review Corrections

The Lavra engineering review found that broad generated-output churn and string-based absence checks would give weak evidence. This revision requires structural removed-surface checks across clap, MCP registries, OpenAPI, generated clients, config schemas, and DTO schemas; covers all REST methods and operation IDs; uses synthetic secrets only; limits regeneration to declared changed inputs; and treats `cargo xtask check` as a final gate rather than the normal edit loop.

## Global Constraints

- Source of truth: live issue #298 Phase 10, `docs/pipeline-unification/delivery/surface-removal-contract.md`, `schemas/schema-generator-contract.md`, `delivery/docs-generator-contract.md`, `delivery/documentation-contract.md`, `delivery/testing-contract.md`, `surfaces/command-contract.md`, `surfaces/rest-contract.md`, `surfaces/tool-contract.md`, `surfaces/web-contract.md`, `surfaces/android-contract.md`, `surfaces/palette-contract.md`, `surfaces/chrome-extension-contract.md`, and `surfaces/presentation-contract.md`.
- Transports (`axon-cli`, `axon-mcp`, and `axon-web`) must consume only `axon-services`/`axon-api` for source operations before old surfaces are deleted. No transport may import domain `ops`, `internal`, store schema, or provider-client internals.
- `axon <source>`, `axon watch <source>`, and `axon watch exec <source>` must exist as canonical surfaces. `extract` remains top-level structured LLM extraction, and `map` remains first-class across CLI/MCP/REST.
- Removed CLI commands must be absent and cannot dispatch: `embed`, `ingest`, `scrape`, `crawl`, `code-search`, `code-search-watch`, `purge`, `dedupe`, `refresh`, and `fresh`. This list is confirmed correct against `surfaces/command-contract.md`'s Removed Commands section (previously stale/missing `refresh`/`fresh` there; fixed) — the open question was never *whether* refresh/fresh are removed, only *when*. `crates/axon-services/src/refresh.rs` (bulk re-enqueue-by-origin) and `crates/axon-services/src/freshness/` (CLI-created recurring schedules) are real, load-bearing features with **no drop-in replacement** in the current `crates/axon-jobs/src/watch.rs` URL-diff scheduler. Do not delete them as part of a generated-artifact/absence-check sweep until the target `watch <source>` / `SourceLedger`-backed freshness lifecycle (see `foundation/source-pipeline.md`) actually exists and covers their semantics — treat that migration as separate, sequenced follow-up work, not a Phase 10 surface-drift cleanup. See `delivery/contradiction-review.md` ("`refresh`/`fresh` Removal Was Missing From `command-contract.md`") for the full resolution note.
- Removed MCP actions must be absent and cannot dispatch: `embed`, `ingest`, `scrape`, `crawl`, `code_search`, `code_search_watch`, `vertical_scrape`, `purge`, and `dedupe`.
- Removed REST routes must be absent from router, OpenAPI, and generated clients: `/v1/embed`, `/v1/ingest`, `/v1/scrape`, `/v1/crawl`, `/v1/purge`, `/v1/dedupe`, and `/v1/watch/{id}/run`.
- REST OpenAPI must include the full end-state route inventory from `surfaces/rest-contract.md`, use the shared success/error envelope, and exclude every removed route/operation id from router, OpenAPI, docs, and generated clients.
- MCP must expose exactly one operation tool named `axon`, a strict action/subaction schema, shared response envelopes, action auth metadata, machine-readable capabilities/help, and no removed action variants.
- Removed DTO fields and config keys from `surface-removal-contract.md` must be absent from generated schemas and fail validation with known replacements.
- There are no compatibility aliases, hidden shims, or remap dispatchers.
- Removed-surface absence checks must be structural. Parse clap command registries, MCP action registries, OpenAPI route registries, generated clients, config schema fields, and DTO schemas. Do not use broad string containment across generated docs.
- REST removed-route tests must check all methods, router paths, OpenAPI paths, generated operation IDs, and generated clients. Do not only test POST examples.
- Generated docs and invalid/secret fixtures must use synthetic secrets only; never copy real secrets into generated artifacts or fixtures.
- Complete only schema families needed to prove removed-surface absence in this phase. Full all-family schema completion belongs to Phase 2/10 hardening unless the family is touched by surface removal.
- Regenerate only declared outputs whose inputs changed. Do not churn web/Palette/Android/generated client assets unless their generator input changed.
- Generated client updates must cover web, Palette, Android, and Chrome extension consumers when OpenAPI/API DTO inputs change. Clients must use REST/SSE source/job/retrieval flows and must not call removed routes.
- Presentation/token generation must emit shared CLI, web, Palette, Android, and Chrome extension token outputs when presentation inputs change. Add token snapshot/accessibility/status-color parity checks instead of app-local palettes.
- App/client fixture tests must cover source submission, job progress/SSE, ask/query/retrieve, artifacts, redaction, and removed-route absence.
- Treat `cargo xtask check` as release/final-gate evidence, not every artifact iteration.

---

## File Structure

- Modify: `xtask/src/schemas.rs`
- Modify/create: `xtask/src/schema/{mod.rs,args.rs,artifact.rs,check.rs,manifest.rs,validate.rs}`
- Modify/create: `xtask/src/schema/families/{api,cli,openapi,mcp,config,events,errors,database,graph,vector_payload,providers}.rs`
- Modify: `xtask/src/schemas/tests.rs`
- Modify: `crates/axon-cli/src/**`
- Modify: `crates/axon-mcp/src/**`
- Modify: `crates/axon-web/src/**`
- Modify: `crates/axon-api/src/**`
- Modify: `crates/axon-core/src/config/**`
- Modify generated artifacts under `docs/reference/**`, `apps/web/**`, and Palette/Android generated client locations.
- Modify generated presentation token outputs only through the presentation generator when touched: CLI token constants, web CSS, Palette CSS, Android Compose tokens, Chrome extension CSS, and `docs/reference/surfaces/presentation/**`.

---

### Task 1: Generated Removed-Surface Checks

**Files:**
- Modify: `xtask/src/schemas/tests.rs`
- Modify/create: `xtask/src/schema/families/removed.rs` if the removal checker is not already part of the aggregate schema module.
- Test: `xtask/src/schemas/tests.rs`

**Interfaces:**
- Produces: `RemovedSurfaceRegistry`, `assert_removed_surface_absent(report)`.

- [ ] **Step 1: Add failing removal registry test**

```rust
#[test]
fn removed_surface_registry_matches_contract() {
    let registry = removed_surface_registry();
    assert!(registry.cli_commands.contains("embed"));
    assert!(registry.cli_commands.contains("code-search-watch"));
    assert!(registry.mcp_actions.contains("vertical_scrape"));
    assert!(registry.rest_routes.contains("POST /v1/embed"));
    assert!(registry.config_keys.contains("AXON_MCP_HTTP_TOKEN"));
    assert!(registry.generated_clients.contains("web"));
    assert!(registry.generated_clients.contains("palette"));
    assert!(registry.generated_clients.contains("android"));
    assert!(registry.generated_clients.contains("chrome-extension"));
}
```

- [ ] **Step 2: Run failing test**

Run: `cargo test -p xtask removed_surface_registry --no-fail-fast`

Expected: failure if the registry is incomplete or absent.

- [ ] **Step 3: Implement removal registry**

Encode removed CLI commands, MCP actions, REST routes, DTO fields, config keys, generated-client operations, and docs examples from `surface-removal-contract.md`, including replacement strings for validation errors.

- [ ] **Step 4: Add absence scan over generated artifacts**

Check CLI command registry, MCP action registry, OpenAPI route registry, generated clients, config schema, DTO schemas, generated docs, and app/client operation maps. Fail if a removed spelling appears as a public operation, accepted field, generated example, client method, or dispatchable route.

- [ ] **Step 5: Run removed-surface tests**

Run: `cargo test -p xtask removed_surface --no-fail-fast`

Expected: current stale surfaces are reported with exact artifact paths.

---

### Task 2: Remove CLI Commands After Absence Checks Exist

**Files:**
- Modify: `crates/axon-cli/src/**`
- Modify: `docs/reference/cli/commands.json` through generator
- Test: `crates/axon-cli/src/removed_surface_tests.rs`

**Interfaces:**
- Consumes: removal registry.
- Produces: CLI parser/help/completion absence, canonical source/watch parser support, and negative dispatch tests.

- [ ] **Step 1: Add failing CLI negative dispatch tests**

```rust
#[test]
fn removed_cli_commands_are_absent_from_help_and_parser() {
    let help = render_cli_help();
    for removed in ["embed", "ingest", "scrape", "crawl", "code-search", "code-search-watch", "purge", "dedupe", "refresh", "fresh"] {
        assert!(!help.contains(removed), "{removed} leaked into help");
        assert!(parse_cli(["axon", removed]).is_err(), "{removed} parsed");
    }
    assert!(parse_cli(["axon", "https://example.com"]).is_ok());
    assert!(parse_cli(["axon", "watch", "https://example.com"]).is_ok());
    assert!(parse_cli(["axon", "watch", "exec", "watch_123"]).is_ok());
}
```

- [ ] **Step 2: Run CLI tests**

Run: `cargo test -p axon-cli removed_cli --no-fail-fast`

Expected: current removed commands fail until deleted from parser/help.

- [ ] **Step 3: Delete command variants and handlers**

Remove parser variants, command dispatch arms, help examples, shell completions, parse fixtures, old auth mappings, and old handler reachability for removed commands. Keep canonical replacements only: source operation, query with filters, watch/watch exec, map, extract, and prune.

- [ ] **Step 4: Run CLI tests**

Run: `cargo test -p axon-cli removed_cli --no-fail-fast`

Expected: removed commands cannot parse or dispatch, canonical `axon <source>`/watch/watch-exec parse, and generated help/completions contain only canonical commands.

---

### Task 3: Remove MCP Actions

**Files:**
- Modify: `crates/axon-mcp/src/**`
- Modify: `crates/axon-api/src/mcp_schema.rs`
- Test: `crates/axon-mcp/src/server/tool_schema_tests.rs`

**Interfaces:**
- Produces: MCP schema absence and negative dispatch tests.

- [ ] **Step 1: Add MCP negative dispatch test**

```rust
#[test]
fn removed_mcp_actions_are_absent_and_rejected() {
    let schema = generated_mcp_tool_schema();
    for removed in ["embed", "ingest", "scrape", "crawl", "code_search", "code_search_watch", "vertical_scrape", "purge", "dedupe"] {
        assert!(!schema.contains(removed), "{removed} leaked into schema");
        let err = dispatch_mcp_action_for_test(removed).unwrap_err();
        assert_eq!(err.code, "action.unknown");
    }
    assert!(schema.contains("\"source\""));
    assert!(schema.contains("\"watches\""));
    assert!(schema.contains("\"reset\""));
    assert!(schema.contains("\"auth\""));
}
```

- [ ] **Step 2: Run MCP tests**

Run: `cargo test -p axon-mcp removed mcp_schema --no-fail-fast`

Expected: stale actions fail until deleted from schema and dispatcher.

- [ ] **Step 3: Delete removed MCP action variants**

Remove action enum variants, schema entries, dispatcher arms, old handler modules, examples, and generated docs references. Ensure the single `axon` tool exposes canonical `action`/`subaction`, shared envelopes, capabilities/help, and action auth metadata. Do not remap old action names to canonical actions.

- [ ] **Step 4: Run MCP tests**

Run: `cargo test -p axon-mcp removed mcp_schema --no-fail-fast`

Expected: removed actions are absent and cannot reach old handlers; canonical actions use shared `axon-api` DTOs and service entry points.

---

### Task 4: Remove REST Routes And Generated Clients

**Files:**
- Modify: `crates/axon-web/src/**`
- Modify: `apps/web/openapi/axon.json` through generator
- Modify generated clients for web, Palette, and Android through generator
- Test: `crates/axon-web/src/removed_route_tests.rs`

**Interfaces:**
- Produces: router/OpenAPI/client absence.

- [ ] **Step 1: Add REST negative route tests**

```rust
#[tokio::test]
async fn removed_rest_routes_return_not_found_and_are_absent_from_openapi() {
    let app = test_router().await;
    for route in ["/v1/embed", "/v1/ingest", "/v1/scrape", "/v1/crawl", "/v1/purge", "/v1/dedupe", "/v1/watch/test/run"] {
        for method in [Method::GET, Method::POST, Method::PUT, Method::PATCH, Method::DELETE] {
            let response = request_json(&app, method, route, serde_json::json!({})).await;
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
        }
    }
    let openapi = generated_openapi_json();
    assert!(!openapi.contains("/v1/embed"));
    assert!(!openapi.contains("/v1/watch/{id}/run"));
    assert!(openapi.contains("/v1/sources"));
    assert!(openapi.contains("/v1/watches/{watch_id}/exec"));
    assert!(openapi.contains("\"ok\""));
}
```

- [ ] **Step 2: Run REST tests**

Run: `cargo test -p axon-web removed_route --no-fail-fast`

Expected: stale routes fail until removed.

- [ ] **Step 3: Delete routes and generated operations**

Remove router registrations, OpenAPI route registry entries, generated clients, docs examples, and frontend calls for removed routes. Replace frontend usage with canonical `/v1/sources`, `/v1/jobs/*`, `/v1/prune/*`, `/v1/reset/*`, `/v1/watches/{watch_id}/exec`, and REST/SSE retrieval APIs from `rest-contract.md`.

- [ ] **Step 4: Run REST tests**

Run: `cargo test -p axon-web removed_route --no-fail-fast`

Expected: removed routes are absent from router/OpenAPI/generated clients, all methods 404 before side effects, generated operation IDs are canonical, and every end-state route required by `rest-contract.md` is represented.

---

### Task 5: Removed DTO Fields And Config Keys

**Files:**
- Modify: `crates/axon-api/src/**`
- Modify: `crates/axon-core/src/config/**`
- Modify: `xtask/src/schema/families/{api,config}.rs`
- Test: `crates/axon-api/src/removed_field_tests.rs`
- Test: `crates/axon-core/src/config/removed_key_tests.rs`

**Interfaces:**
- Produces: schema rejection for removed DTO fields/config keys with known replacements.

- [ ] **Step 1: Add failing removed field/key tests**

```rust
#[test]
fn removed_dto_fields_fail_validation_with_replacement() {
    let err = validate_source_request_json(json!({"input": "/tmp/repo"})).unwrap_err();
    assert_eq!(err.code, "schema.removed_field");
    assert_eq!(err.replacement.as_deref(), Some("SourceRequest.source"));
}

#[test]
fn removed_config_keys_fail_with_replacement() {
    let err = validate_env_key("AXON_MCP_HTTP_TOKEN").unwrap_err();
    assert_eq!(err.code, "config.removed_key");
    assert_eq!(err.replacement.as_deref(), Some("AXON_HTTP_TOKEN"));
}
```

- [ ] **Step 2: Run API/config tests**

Run:

```bash
cargo test -p axon-api removed_field --no-fail-fast
cargo test -p axon-core removed_key --no-fail-fast
```

Expected: removed fields or stale config acceptance fails.

- [ ] **Step 3: Remove DTO/config acceptance**

Delete removed DTO fields from schemas and parser paths. Remove stale `AXON_MCP_*` docs/config references when the clean-break contract requires renamed envs. Validation errors use the replacement registry, not hidden aliases.

- [ ] **Step 4: Run API/config tests**

Run:

```bash
cargo test -p axon-api removed_field --no-fail-fast
cargo test -p axon-core removed_key --no-fail-fast
```

Expected: removed fields/keys are absent from generated schemas and fail validation.

---

### Task 6: Complete Schema Generator Families

**Files:**
- Modify/create: `xtask/src/schema/families/{api,cli,openapi,mcp,config,events,errors,database,graph,vector_payload,providers}.rs`
- Modify: `xtask/src/schema/{artifact,manifest,validate,check}.rs`
- Test: `xtask/src/schemas/tests.rs`

**Interfaces:**
- Produces: complete generator coverage for API, CLI, OpenAPI, MCP, config, events, errors, database, graph, vector-payload, and providers.

- [ ] **Step 1: Add aggregate family test**

```rust
#[test]
fn schema_generator_lists_every_required_family() {
    let families = schema_families();
    for family in ["api", "cli", "openapi", "mcp", "config", "events", "errors", "database", "graph", "vector-payload", "providers"] {
        assert!(families.iter().any(|f| f.name() == family), "missing {family}");
    }
}
```

- [ ] **Step 2: Run xtask tests**

Run: `cargo test -p xtask schemas --no-fail-fast`

Expected: missing families or incomplete fixtures fail.

- [ ] **Step 3: Implement source input manifests and check mode**

Each generated artifact includes:

```json
{
  "x-axon": {
    "source_inputs": [
      { "path": "crates/axon-api/src/source.rs", "kind": "rust_module", "checksum": "sha256:..." }
    ]
  }
}
```

`--check` generates in memory, compares output, validates fixtures, validates documented examples, and exits nonzero without writing when stale.

- [ ] **Step 4: Add fixtures and golden snapshots**

For every family, add valid/minimal, valid/full, invalid/missing-required, invalid/unknown-field, invalid/bad-enum when enums exist, invalid/secret when public/redacted data exists, golden snapshots, and aggregate cross-schema parity checks that prove CLI/MCP/REST/config/API schemas agree on canonical DTOs/enums and removed-surface absence.

- [ ] **Step 5: Run schema generation**

Run:

```bash
cargo xtask schemas generate
cargo xtask schemas generate --check
```

Expected: generation is deterministic, check mode reports no writes needed, every generated artifact has source input manifests, and every generated example validates.

---

### Task 7: Generated Clients, Presentation Tokens, And App Fixtures

**Files:**
- Generated: web, Palette, Android, and Chrome extension clients
- Generated: `docs/reference/surfaces/presentation/**`
- Generated: `apps/web/src/styles/axon-tokens.css`
- Generated: `apps/palette-tauri/src/styles/axon-tokens.css`
- Generated: `apps/chrome-extension/src/styles/axon-tokens.css`
- Generated: Android Compose token files
- Modify: app/client tests when generated route/DTO inputs change.

- [ ] **Step 1: Add app/client parity tests**

Cover source submission, job progress/SSE, ask/query/retrieve, artifacts, redaction, and removed-route absence for web, Palette, Android, and Chrome extension generated clients.

- [ ] **Step 2: Add presentation parity tests**

Assert every required semantic token from `surfaces/presentation-contract.md` exists for CLI, web, Palette, Android, and Chrome extension; status-color mappings match; accessibility/contrast snapshots exist for primary status/control components.

- [ ] **Step 3: Regenerate client and token artifacts only when inputs changed**

Run the relevant generated-client command(s) plus:

```bash
cargo xtask presentation generate
cargo xtask presentation generate --check
```

Expected: generated clients use REST/SSE source/job/retrieval flows; no removed route operation is exposed; token outputs are reproducible and include source hashes/contract versions.

---

### Task 8: Regenerate All Public Artifacts Together

**Files:**
- Generated: `docs/reference/**/*`
- Generated: web/Palette/Android assets
- Test: `xtask/src/schemas/tests.rs`

- [ ] **Step 1: Regenerate public artifacts**

Run:

```bash
cargo xtask schemas generate
```

Expected: generated docs, CLI help, MCP schema, REST OpenAPI, generated clients, and presentation token artifacts update together when their declared inputs changed.

- [ ] **Step 2: Run absence checks**

Run:

```bash
cargo xtask schemas generate --check
cargo test -p xtask removed_surface --no-fail-fast
cargo test -p axon-cli removed_cli --no-fail-fast
cargo test -p axon-mcp removed --no-fail-fast
cargo test -p axon-web removed_route --no-fail-fast
cargo xtask presentation generate --check
```

Expected: removed surfaces are absent across generated artifacts and dispatch.

- [ ] **Step 3: Run docs generator check**

Run:

```bash
cargo xtask docs generate --check
cargo xtask check-doc-links
cargo xtask check-doc-contracts
```

Expected: generated markdown headers/source manifests exist, links validate, examples validate, and docs do not duplicate removed public surfaces.

- [ ] **Step 4: Run final Phase 10 schema check**

Run: `cargo xtask schemas generate --check`

Expected: generated artifacts, schema fixtures, docs fixtures, token outputs, generated clients, and removal checks pass. `cargo xtask check` remains final cutover evidence, not the normal Phase 10 artifact iteration.
