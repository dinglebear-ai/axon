# Phase 2 Schema Contract Alignment Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring issue #298 Phase 2 into compliance with `docs/pipeline-unification/schemas/**` by replacing schema-generator skeleton behavior with registry-backed generation, real validation, deterministic drift checks, and cross-family parity checks.

**Architecture:** `xtask` orchestrates schema generation and validation. It must not become the source of truth for commands, REST routes, MCP actions, config keys, provider capabilities, graph kinds, or public DTO contracts. Each family generator consumes a Rust-owned registry from the crate that owns the runtime surface, then emits deterministic JSON/Markdown artifacts with source-input checksums.

**Tech Stack:** Rust 2024, Cargo `xtask`, `serde_json`, `schemars`, `jsonschema`, `utoipa`/OpenAPI JSON where already used, Axon workspace crates under `/home/jmagar/workspace/axon`.

## Global Constraints

- Source of truth is the docs packet under `docs/pipeline-unification/`, especially `schemas/README.md`, `schemas/schema-generator-contract.md`, and each per-family schema contract.
- `cargo xtask schemas generate` and `cargo xtask schemas generate --check` are the aggregate commands.
- Check mode never writes generated artifacts.
- Generation mode writes only declared output paths.
- JSON output is deterministic and pretty-printed with stable key ordering.
- Every generated schema artifact must include `x-axon.source_inputs` with stable SHA-256 checksums.
- Public wire DTOs stay in `axon-api`; transport schemas reference `axon-api` DTOs through success/error envelopes instead of private copies or raw result DTOs.
- Removed commands/actions/routes/config keys/DTO fields must be absent from generated references and rejected by schema-aware tests.
- Keep `extract` and `map` as canonical surfaces; do not remove them as legacy commands/actions/routes.
- Do not treat current skeleton artifacts as contract-complete.
- Do not add an `adapters` schema family in Phase 2. `schemas/README.md` does not define that family.
- Do not edit GitHub issue #298 in this implementation unless Jacob explicitly asks for issue mutation.
- Before broad validation, classify changed files and run the smallest check that proves the slice; schema-generator changes justify targeted xtask tests plus `cargo xtask schemas generate --check`.

## Engineering Review Corrections

The Lavra engineering review found blockers in the original Phase 2 plan. This revised plan applies them directly:

- No hard-coded “real” generators in `xtask`. Static command/action/route/config/provider lists inside `xtask` are prohibited unless they are a short-lived failing test fixture. Runtime registries must live in owning crates.
- No pseudo snapshots. Snapshot fixtures must be actual generated artifacts or golden JSON/Markdown files, not `must_contain` smoke files.
- No substring invalid-fixture checks. Valid fixtures must pass the generated schema; invalid fixtures must fail the generated schema or a family-specific validator with an expected category.
- OpenAPI generation must include route registry metadata: required scope, mutability, async/streaming state, success/error envelopes, and `401`/`403` responses.
- MCP schema generation must include action/subaction discriminator rules, DTO refs, grouped-action validation, and required scope metadata before dispatch.
- Config/env validation must include removed-key registry coverage, especially stale `AXON_MCP_*` keys named in `delivery/surface-removal-contract.md`.
- Source-input hashing must use explicit registry/module paths, memoized checksums, and streaming hashing. Do not hash broad crate source directories repeatedly.
- Cross-checking must parse artifacts once per run and define behavior for targeted single-family runs.

## Source Of Truth

- `docs/pipeline-unification/schemas/schema-generator-contract.md`
- `docs/pipeline-unification/schemas/api-dto-schema.md`
- `docs/pipeline-unification/schemas/cli-schema.md`
- `docs/pipeline-unification/schemas/openapi-schema.md`
- `docs/pipeline-unification/schemas/mcp-tool-schema.md`
- `docs/pipeline-unification/schemas/config-schema.md`
- `docs/pipeline-unification/schemas/event-schema.md`
- `docs/pipeline-unification/schemas/error-schema.md`
- `docs/pipeline-unification/schemas/database-schema.md`
- `docs/pipeline-unification/schemas/graph-schema.md`
- `docs/pipeline-unification/schemas/vector-payload-schema.md`
- `docs/pipeline-unification/schemas/provider-capability-schema.md`
- `docs/pipeline-unification/delivery/surface-removal-contract.md`
- `docs/pipeline-unification/surfaces/command-contract.md`
- `docs/pipeline-unification/surfaces/rest-contract.md`
- `docs/pipeline-unification/surfaces/tool-contract.md`

## Task 0: Lock Registry Ownership And Scope

**Files:**
- Modify: `xtask/src/schemas/families.rs`
- Modify: `xtask/src/schemas/families/family_specs.rs`
- Modify: `xtask/src/schemas/tests.rs`
- Modify owner crates only when adding registry exports required by this phase.

**Interfaces:**
- Consumes: `SchemaFamily`, existing schema family dispatch.
- Produces: tests proving no non-registry family is considered complete through skeleton fallback or static `xtask` mirrors.

- [ ] **Step 1: Add a failing test that rejects skeleton success**

Add a test that generates each family and asserts no artifact contains the skeleton `SchemaFamilyContract` definition unless the family is explicitly marked `Deferred`.

- [ ] **Step 2: Add family status metadata**

Each family must be one of:

```text
RegistryBacked - generated from owning crate registry and fully validated
ValidationOnly - existing artifact is checked for removed surfaces, dangling refs, and source inputs while registry work is pending
Deferred - not claimed as Phase 2 complete; has owner follow-up plan and reason
```

The plan may only mark a family `RegistryBacked` when generation consumes a registry from the owning crate.

- [ ] **Step 3: Drop out-of-contract families**

Remove `Adapters` from Phase 2 family lists and fixtures unless a dedicated schema contract is added first.

- [ ] **Step 4: Verify**

```bash
cargo test -p xtask schema_family_statuses_are_explicit --no-fail-fast
cargo test -p xtask skeleton_artifacts_are_not_contract_complete --no-fail-fast
```

## Task 1: Source Inputs, Artifact Index, And Targeted-Run Semantics

**Files:**
- Modify: `xtask/src/schemas/source_input.rs`
- Modify: `xtask/src/schemas/artifact.rs`
- Create: `xtask/src/schemas/artifact_index.rs`
- Modify: `xtask/src/schemas.rs`
- Modify: `xtask/src/schemas/tests.rs`

**Interfaces:**
- Produces memoized streaming source-input checksums.
- Produces a parsed artifact index shared by validation and cross-checking.
- Defines cross-check behavior for aggregate versus single-family runs.

- [ ] **Step 1: Replace broad directory hashing**

Do not declare inputs such as `crates/axon-api/src` or `crates/axon-cli/src`. Use explicit registry/module files and contract docs:

```text
crates/axon-cli/src/schema_registry.rs
crates/axon-web/src/schema_registry.rs
crates/axon-mcp/src/schema_registry.rs
crates/axon-core/src/config/schema_registry.rs
crates/axon-api/src/source/*.rs only where API DTO generation directly reads those modules
docs/pipeline-unification/schemas/<family>.md
```

If a registry file does not exist yet, add it in the owner crate rather than mirroring data in `xtask`.

- [ ] **Step 2: Stream and memoize checksums**

Implement a per-run checksum cache keyed by repo-relative path. Directory inputs are allowed only for fixture roots and must be streamed file-by-file without concatenating all bytes into one `Vec<u8>`.

- [ ] **Step 3: Parse artifacts once**

Create an `ArtifactIndex` that stores each generated artifact path, raw content, parsed JSON when applicable, and source family. Validation and cross-checks must reuse this index rather than reparsing or cloning artifacts.

- [ ] **Step 4: Define targeted-family behavior**

For `cargo xtask schemas <family> --check`, run:

```text
that family generator
that family source-input check
that family fixture validation
cross-check rows whose required artifacts are present
```

For `cargo xtask schemas generate --check`, run all family checks and all cross-check rows.

- [ ] **Step 5: Verify**

```bash
cargo test -p xtask source_inputs_are_registry_scoped --no-fail-fast
cargo test -p xtask check_mode_does_not_write_any_schema_artifact --no-fail-fast
cargo test -p xtask targeted_family_checks_do_not_require_hidden_aggregate_generation --no-fail-fast
```

## Task 2: Real Fixture Validation And Golden Snapshots

**Files:**
- Create: `xtask/src/schemas/report.rs`
- Create: `xtask/src/schemas/validate.rs`
- Modify: `xtask/src/schemas/artifact.rs`
- Modify: `xtask/src/schemas/families/family_specs.rs`
- Modify: `xtask/src/schemas/tests.rs`
- Create fixture files only for families implemented or validated in this phase.

**Interfaces:**
- Produces `FamilyReport` with fixture and snapshot counts.
- Produces `validate_family(root, set, index, mode) -> Result<FamilyReport>`.
- Uses JSON Schema validation for valid/invalid fixtures.

- [ ] **Step 1: Add report-shape test**

`FamilyReport` must include:

```text
family
ok
artifacts_checked
fixtures_validated
snapshots_checked
drift
warnings
```

- [ ] **Step 2: Add standard fixture categories**

Each `RegistryBacked` family must have, at minimum:

```text
valid/minimal.json
valid/full.json
invalid/missing-required.json
invalid/unknown-field.json
invalid/bad-enum.json
invalid/secret-or-removed-field.json where applicable
snapshots/<artifact-name>.json or .md
```

Do not create one flat fixture per family as a substitute for these categories.

- [ ] **Step 3: Validate fixtures against generated schemas**

Compile each generated JSON Schema once per family. Valid fixtures must pass. Invalid fixtures must fail and must record the expected failure category. Missing declared fixtures are hard failures, never warnings.

- [ ] **Step 4: Validate snapshots as real goldens**

Snapshot files must be exact generated artifacts or exact reduced goldens with documented normalization. Do not use `must_contain` arrays as snapshots.

- [ ] **Step 5: Add secret/redaction fixtures**

For public API, config, MCP, OpenAPI, events, errors, and provider families, include invalid fixtures that prove raw bearer tokens, API keys, `client_secret`, `Authorization`, and stale removed env keys cannot appear in generated public schemas/examples.

- [ ] **Step 6: Verify**

```bash
cargo test -p xtask family_report_includes_fixture_and_snapshot_counts --no-fail-fast
cargo test -p xtask every_registry_backed_family_has_standard_fixture_categories --no-fail-fast
cargo test -p xtask invalid_fixtures_fail_real_schema_validation --no-fail-fast
```

## Task 3: Registry-Backed CLI Schema Family

**Files:**
- Create or modify: `crates/axon-cli/src/schema_registry.rs`
- Modify: `crates/axon-cli/src/lib.rs` or crate root exports as needed.
- Create or modify: `xtask/src/schemas/families/cli.rs`
- Modify: `xtask/src/schemas/families.rs`
- Modify: `xtask/src/schemas/tests.rs`
- Regenerate: `docs/reference/cli/commands.json`
- Regenerate: `docs/reference/cli/commands.md`
- Regenerate: `docs/reference/cli/axon-help.md`

**Interfaces:**
- Produces `axon_cli::schema_registry::command_registry()`.
- `xtask` consumes that registry; it does not duplicate command lists.

- [ ] **Step 1: Add owner-crate command registry**

The CLI registry must include:

```text
name
path
aliases
summary
usage
args
flags
env_overrides
maps_to_dto
mutates
async
requires_auth_scope
examples
removed/replacement metadata where applicable
```

- [ ] **Step 2: Add dispatch parity test**

Add a test proving every generated CLI command maps to a real CLI dispatch path and every removed command from `surface-removal-contract.md` is absent:

```text
embed
ingest
scrape
crawl
code-search
code-search-watch
purge
dedupe
refresh
fresh
```

Keep `map` and `extract`.

- [ ] **Step 3: Generate CLI artifacts from registry**

`xtask/src/schemas/families/cli.rs` may format JSON and Markdown, but all command facts must come from `axon_cli::schema_registry::command_registry()`.

- [ ] **Step 4: Verify**

```bash
cargo test -p axon-cli schema_registry --no-fail-fast
cargo test -p xtask cli_schema_is_registry_backed_and_contains_command_records --no-fail-fast
cargo xtask schemas cli --check
```

## Task 4: Registry-Backed MCP Schema Family

**Files:**
- Create or modify: `crates/axon-mcp/src/schema_registry.rs`
- Modify: `crates/axon-mcp/src/lib.rs` or crate root exports as needed.
- Create or modify: `xtask/src/schemas/families/mcp.rs`
- Modify: `xtask/src/schemas/tests.rs`
- Regenerate: `docs/reference/mcp/tool-schema.json`
- Regenerate: `docs/reference/mcp/pipeline-tool-schema.md`
- Regenerate: `crates/axon-mcp/tests/golden/tool-schema.json`

**Interfaces:**
- Produces `axon_mcp::schema_registry::action_registry()`.
- Produces action/subaction discriminator rules and DTO refs.

- [ ] **Step 1: Add owner-crate MCP action registry**

The registry must include:

```text
action
subaction rules
request DTO ref
result DTO ref
required auth scope
mutates
async/job behavior
degraded/error behavior
removed/replacement metadata where applicable
```

- [ ] **Step 2: Generate discriminator schema**

The generated schema must enforce:

```text
unknown action rejected before dispatch
grouped actions require valid subaction
ungrouped actions reject subaction
request payload validates against selected DTO branch
result envelope is documented separately from REST envelope
```

- [ ] **Step 3: Add removed-action tests**

Removed MCP actions must be absent and non-dispatchable in generated schema:

```text
embed
ingest
scrape
crawl
code_search
code_search_watch
vertical_scrape
purge
dedupe
```

- [ ] **Step 4: Verify**

```bash
cargo test -p axon-mcp schema_registry --no-fail-fast
cargo test -p xtask mcp_schema_is_registry_backed_and_validates_action_branches --no-fail-fast
cargo xtask schemas mcp --check
```

## Task 5: OpenAPI Validation Or Registry-Backed Generation

**Files:**
- Create or modify: `crates/axon-web/src/schema_registry.rs`
- Create or modify: `xtask/src/schemas/families/openapi.rs`
- Modify: `xtask/src/schemas/tests.rs`
- Regenerate only when registry-backed generation is implemented.

**Interfaces:**
- Produces or consumes `RestRouteSpec` from `axon-web`.
- Validates OpenAPI 3.1 refs, auth scopes, envelopes, and removed routes.

- [ ] **Step 1: Choose the implementation mode**

Use one of these modes, and record it in `family_specs.rs`:

```text
RegistryBacked - implement axon_web::schema_registry::rest_route_registry() and generate OpenAPI from it
ValidationOnly - validate the current generated OpenAPI for removed routes, dangling refs, envelopes, and required auth metadata
```

Do not generate OpenAPI from an `xtask` hard-coded route list.

- [ ] **Step 2: Enforce route metadata**

Every route in the generated or validated OpenAPI must include:

```text
operation_id
request DTO ref where applicable
success envelope ref
error envelope ref
required auth scope
mutates
async/streaming status
401 and 403 responses for protected routes
```

- [ ] **Step 3: Enforce removed route absence**

Removed routes must be absent from router/OpenAPI/generated clients:

```text
/v1/embed
/v1/ingest
/v1/scrape
/v1/crawl
/v1/purge
/v1/dedupe
/v1/watch/{id}/run
```

Keep `/v1/map` and `/v1/extract`.

- [ ] **Step 4: Verify**

```bash
cargo test -p xtask openapi_has_no_dangling_refs --no-fail-fast
cargo test -p xtask openapi_routes_have_auth_scope_and_envelopes --no-fail-fast
cargo test -p xtask removed_rest_routes_are_absent --no-fail-fast
cargo xtask schemas openapi --check
```

## Task 6: Config, Provider, Graph, And Runtime Families Without Fake Completion

**Files:**
- Modify owner crate registries where already available.
- Modify: `xtask/src/schemas/families/config.rs`
- Modify: `xtask/src/schemas/families/providers.rs`
- Modify: `xtask/src/schemas/families/graph.rs`
- Modify: `xtask/src/schemas/runtime_defs.rs`
- Modify: `xtask/src/schemas/tests.rs`

**Interfaces:**
- Produces `RegistryBacked`, `ValidationOnly`, or `Deferred` status for each family.
- Prevents fake completion through underspecified `xtask` schemas.

- [ ] **Step 1: Config**

If registry-backed, generate from an `axon-core` config registry that includes:

```text
server
sources
pipeline
watch
jobs
providers
retrieval
memory
graph
observability
auth/admin settings
env key mapping
removed-key registry and replacements
```

If not registry-backed, mark `Config` as `ValidationOnly` and add tests proving generated config/env schemas reject removed keys from `surface-removal-contract.md`, including stale `AXON_MCP_*` names.

- [ ] **Step 2: Provider capabilities**

If registry-backed, generate from provider capability registries that include:

```text
provider_kind
health
limits
reservation policy/state
degraded modes
cooling/retry fields
capability names
```

Do not ship a provider schema that only has `kind`, `status`, and `capabilities`.

- [ ] **Step 3: Graph**

If registry-backed, generate graph schema from the graph crate’s node/edge/evidence registries and validate edge kinds plus source-range/evidence requirements. Otherwise mark `Graph` as `ValidationOnly` and check current artifacts for dangling refs and removed/invalid edge kinds.

- [ ] **Step 4: Runtime event/error/database/vector families**

Keep existing real generators where they already exist, but ensure they expose the same family API and fixture validation path as new modules.

- [ ] **Step 5: Verify**

```bash
cargo test -p xtask config_removed_keys_are_rejected --no-fail-fast
cargo test -p xtask provider_schema_requires_contract_fields --no-fail-fast
cargo test -p xtask graph_schema_validates_edge_and_evidence_contracts --no-fail-fast
cargo xtask schemas generate --check
```

## Task 7: Cross-Family Parity Checks

**Files:**
- Create: `xtask/src/schemas/cross_check.rs`
- Modify: `xtask/src/schemas.rs`
- Modify: `xtask/src/schemas/tests.rs`

**Interfaces:**
- Consumes: `ArtifactIndex`.
- Produces: aggregate cross-family drift report.

- [ ] **Step 1: Add dangling-ref checks**

All local `$ref` and relative artifact refs must resolve. This includes API, OpenAPI, MCP, events, errors, graph, vector payload, provider, config, and database artifacts.

- [ ] **Step 2: Add removed-surface checks**

Generated artifacts must reject removed commands/actions/routes/config keys/DTO fields using schema-aware checks, not raw global substring scans.

- [ ] **Step 3: Add DTO/envelope parity**

Transport schemas must reference `axon-api` DTOs and the correct transport envelopes:

```text
REST: success/error envelope components
MCP: MCP action result envelope and action-specific DTO branches
CLI: command maps_to_dto exists in API schema or is explicitly internal
```

- [ ] **Step 4: Add auth/scope parity**

CLI/MCP/OpenAPI public operations must agree on required scope for the same operation family:

```text
read
write
admin
execute/local/tool where applicable
```

- [ ] **Step 5: Add targeted-run policy**

Cross-check rows that need missing family artifacts must be skipped with a warning in targeted single-family mode and hard-fail in aggregate mode.

- [ ] **Step 6: Verify**

```bash
cargo test -p xtask cross_checks_detect_dangling_refs --no-fail-fast
cargo test -p xtask cross_checks_detect_removed_surface_drift --no-fail-fast
cargo test -p xtask cross_checks_detect_scope_mismatch --no-fail-fast
cargo xtask schemas generate --check
```

## Task 8: CI And Closeout

**Files:**
- Modify CI only if current workflows do not already run schema checks.
- Modify docs only through generated commands or explicit source-of-truth updates.

- [ ] **Step 1: Do not duplicate broad CI jobs**

Inspect current CI first. If `schema-contract-sync` already runs `cargo xtask schemas generate --check`, do not add another broad job. Add only targeted tests missing from the current job.

- [ ] **Step 2: Generate artifacts**

Run:

```bash
cargo xtask schemas generate
```

Review generated artifacts for expected families only.

- [ ] **Step 3: Final verification**

Run:

```bash
cargo test -p xtask schemas:: --no-fail-fast
cargo xtask schemas generate --check
cargo xtask check-layering
git diff --check
```

- [ ] **Step 4: Record deferred families**

Any family not `RegistryBacked` must have:

```text
status
reason
owner follow-up plan
minimal validation still enforced in Phase 2
```

Do not mark Phase 2 complete in issue #298 while required source-of-truth families remain fake or untracked.

## Not In Scope

- Full all-family fixture matrix expansion beyond the families implemented or validation-owned in this phase.
- Markdown polish for every generated reference page.
- Issue #298 body mutation.
- Generated client parity beyond proving generated contracts are internally valid and removed surfaces are absent.
- Adding an `adapters` schema family without a dedicated schema contract.

## Final Verification

```bash
cargo test -p xtask schemas:: --no-fail-fast
cargo xtask schemas generate --check
cargo xtask check-layering
git diff --check
```

Expected:

```text
all xtask schema tests pass
schema generate --check exits without writes
layering check passes
git diff --check prints no whitespace errors
```
