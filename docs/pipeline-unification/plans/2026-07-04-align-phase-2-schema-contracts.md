# Phase 2 Schema Contract Alignment Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring issue #298 Phase 2 into full compliance with `docs/pipeline-unification/schemas/**` by replacing schema-generator skeleton behavior with registry-backed generation for every schema inventory family, real validation, deterministic drift checks, and cross-family parity checks.

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
- Do not mark any schema family `ValidationOnly` or `Deferred` for Phase 2 closeout. Every family listed in `docs/pipeline-unification/schemas/README.md` must be `RegistryBacked`, with fixtures, snapshots, markdown where required, source-input checksums, and aggregate cross-check coverage.
- Do not add an `adapters` schema family in Phase 2. `schemas/README.md` does not define that family.
- Do not edit GitHub issue #298 in this implementation unless Jacob explicitly asks for issue mutation.
- Before broad validation, classify changed files and run the smallest check that proves the slice; schema-generator changes justify targeted xtask tests plus `cargo xtask schemas generate --check`.

## Engineering Review Corrections

The Lavra engineering review found blockers in the original Phase 2 plan. This revised plan applies them directly:

- No hard-coded “real” generators in `xtask`. Static command/action/route/config/provider lists inside `xtask` are prohibited unless they are a short-lived failing test fixture. Runtime registries must live in owning crates.
- No pseudo snapshots. Snapshot fixtures must be actual generated artifacts or golden JSON/Markdown files, not `must_contain` smoke files.
- No substring invalid-fixture checks. Valid fixtures must pass the generated schema; invalid fixtures must fail the generated schema or a family-specific validator with an expected category.
- API DTO generation must be a first-class registry-backed family, not a side effect of ad hoc `schemars` calls. It must cover every DTO family required by `schemas/api-dto-schema.md`, including retrieval/ask/chat/evaluate/suggest/research/summarize/endpoint/brand/diff/screenshot/extract, memory, config/setup/serve/MCP/palette operational DTOs, prune/reset, provider capability DTOs, config projection DTOs, envelopes, enum registry projections, field-level `x-axon` metadata, extension-point bounds, examples, and forbidden fields.
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
- Produces: tests proving no non-registry family is considered complete through skeleton fallback, static `xtask` mirrors, validation-only status, or deferred family status.

- [ ] **Step 1: Add a failing test that rejects skeleton success**

Add a test that generates each family and asserts no generated artifact contains the skeleton `SchemaFamilyContract` definition outside explicit test fixtures.

- [ ] **Step 2: Add family status metadata**

Each family must be:

```text
RegistryBacked - generated from owning crate registry and fully validated
```

The plan may only mark a family `RegistryBacked` when generation consumes a registry from the owning crate. `ValidationOnly` and `Deferred` are allowed only in intermediate commits and must fail the Phase 2 completion check.

- [ ] **Step 3: Drop out-of-contract families**

Remove `Adapters` from Phase 2 family lists and fixtures unless a dedicated schema contract is added first.

- [ ] **Step 4: Verify**

```bash
cargo test -p xtask schema_family_statuses_are_explicit --no-fail-fast
cargo test -p xtask skeleton_artifacts_are_not_contract_complete --no-fail-fast
cargo test -p xtask phase_2_rejects_validation_only_or_deferred_families --no-fail-fast
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
- Create fixture files for every family required by `docs/pipeline-unification/schemas/README.md`.

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

## Task 2A: Xtask Schema CLI Contract And Generator-Level Fixtures

**Files:**
- Modify: `xtask/src/main.rs`
- Modify: `xtask/src/schemas.rs`
- Modify: `xtask/src/schemas/cli.rs`
- Modify: `xtask/src/schemas/report.rs`
- Modify: `xtask/src/schemas/tests.rs`
- Create or update: `xtask/tests/fixtures/schema-generator/{valid,invalid,snapshots,examples}/`

**Interfaces:**
- Produces the command surface required by `schemas/schema-generator-contract.md`.
- Produces machine-readable report output for aggregate and family-specific schema runs.
- Produces generator-level fixtures that prove stale, missing, and dangling artifacts fail before Phase 2 closeout.

- [ ] **Step 1: Implement the required command matrix**

The following commands must exist and share the same generator/check/report implementation:

```bash
cargo xtask schemas generate
cargo xtask schemas generate --check
cargo xtask schemas generate --print
cargo xtask schemas api
cargo xtask schemas api --check
cargo xtask schemas api --print
cargo xtask schemas cli
cargo xtask schemas openapi
cargo xtask schemas mcp
cargo xtask schemas config
cargo xtask schemas events
cargo xtask schemas errors
cargo xtask schemas database
cargo xtask schemas graph
cargo xtask schemas vector-payload
cargo xtask schemas providers
```

- [ ] **Step 2: Implement required flags and exit codes**

Support the flags required by the generator contract:

```text
--check
--print
--json
--family <name>
--update-fixtures
```

Exit codes must be stable and tested:

```text
0 success
1 validation or drift failure
2 bad invocation
3 source-input or artifact manifest failure
4 internal generator error
```

`--update-fixtures` must be refused in CI unless an explicit local-update environment gate is set.

- [ ] **Step 3: Add generator-level fixtures**

Add fixtures named by `schemas/schema-generator-contract.md`:

```text
all-families.valid.json
check-report.valid.json
missing-family.invalid.json
stale-artifact.invalid.json
dangling-ref.invalid.json
missing-source-input.invalid.json
```

These fixtures validate the aggregate generator behavior, not a single schema family.

- [ ] **Step 4: Verify**

```bash
cargo test -p xtask schema_cli_accepts_required_commands_and_flags --no-fail-fast
cargo test -p xtask schema_cli_exit_codes_are_stable --no-fail-fast
cargo test -p xtask schema_generator_contract_fixtures_validate --no-fail-fast
cargo xtask schemas generate --print >/tmp/axon-schema-generator-report.json
cargo xtask schemas generate --check --json
```

## Task 2B: Registry-Backed API DTO Schema Family

**Files:**
- Create or modify: `crates/axon-api/src/schema_registry.rs`
- Modify: `crates/axon-api/src/lib.rs` or `crates/axon-api/src/source.rs` exports as needed.
- Create or modify: `xtask/src/schemas/families/api.rs`
- Modify: `xtask/src/schemas/api_defs.rs`
- Modify: `xtask/src/schemas/tests.rs`
- Create fixtures: `crates/axon-api/tests/fixtures/schema/{valid,invalid,snapshots,examples}/`
- Regenerate: `docs/reference/api/schemas.json`
- Regenerate: `docs/reference/api/dto.md`
- Regenerate: `docs/reference/api/enums.md`

**Interfaces:**
- Produces `axon_api::schema_registry::dto_schema_registry()`.
- Produces `axon_api::schema_registry::enum_schema_registry()`.
- Produces field-level metadata required by `schemas/api-dto-schema.md`.

- [ ] **Step 1: Add API DTO registry completeness tests**

Add failing tests that require the registry to include every family named by `schemas/api-dto-schema.md`:

```text
Envelope
Source
Ledger
Document
Parse/Graph
Embedding/Vector
Retrieval
Discovery/Synthesis
Runtime
Operations
Errors
Memory
Config/setup/serve/MCP/palette operational DTOs
Provider capability DTOs
Config projection DTOs
```

The tests must fail if a public transport-exposed DTO exported by `axon-api` lacks a registry entry, examples, extension-point metadata when applicable, or forbidden-field metadata.

- [ ] **Step 2: Add required DTO and enum registries**

Implement registry records with:

```text
name
rust_type
module
family
transport_exposed
store_exposed
fields
examples
extension_points
forbidden_fields
```

Each field record must include:

```text
name
rust_type
json_type
required
visibility
extension_point
description
```

Every enum from `foundation/types/enum-contract.md` must be projected into the API schema with owner crate metadata. The generated schema must fail if enum values drift from the Rust registry.

- [ ] **Step 3: Emit contract-shaped API schema**

Generate `docs/reference/api/schemas.json` with:

```text
$schema draft 2020-12
$id https://axon.local/schemas/api/schemas.schema.json
title AxonApiSchemas
x-axon.owner_crates ["axon-api", "axon-error", "axon-observe"]
x-axon.generated_by cargo xtask schemas api
x-axon.contract_version
x-axon.source_inputs
$defs for every required DTO and enum
additionalProperties false except explicit extension maps
field-level x-axon.rust_type
field-level x-axon.visibility
field-level x-axon.source_crate
```

- [ ] **Step 4: Reject removed DTO names and fields by schema path**

Removed DTOs and fields from `delivery/surface-removal-contract.md` must be absent and must be rejected by invalid fixtures:

```text
EmbedRequest
IngestRequest
CrawlRequest
ScrapeRequest
CodeSearchRequest
EmbedRequest.input
EmbedRequest.source_type
IngestRequest.target
IngestRequest.source_type
IngestRequest.include_source
CrawlRequest.urls
ScrapeRequest.url
PurgeRequest.target
PurgeRequest.prefix
CodeSearchRequest.cwd
CodeSearchRequest.path_prefix
CodeSearchRequest.no_freshness
```

- [ ] **Step 5: Generate API markdown from the same model**

`docs/reference/api/dto.md` and `docs/reference/api/enums.md` must include:

```text
generated marker
overview
generated artifacts
source inputs
required definitions
field tables
enum tables
extension points
forbidden fields
examples
fixture paths
drift checks
```

Do not maintain markdown tables separately from `schemas.json`.

- [ ] **Step 6: Verify**

```bash
cargo test -p axon-api schema_registry --no-fail-fast
cargo test -p xtask api_dto_registry_covers_every_required_family --no-fail-fast
cargo test -p xtask api_schema_has_field_level_x_axon_metadata --no-fail-fast
cargo test -p xtask removed_legacy_api_request_shapes_are_absent --no-fail-fast
cargo xtask schemas api --check
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

## Task 5: Registry-Backed OpenAPI Generation

**Files:**
- Create or modify: `crates/axon-web/src/schema_registry.rs`
- Create or modify: `xtask/src/schemas/families/openapi.rs`
- Modify: `xtask/src/schemas/tests.rs`
- Regenerate: `docs/reference/rest/openapi.json`
- Regenerate: `docs/reference/rest/openapi.md`
- Regenerate: `docs/reference/rest/schemas.md`

**Interfaces:**
- Produces `RestRouteSpec` from `axon-web`.
- Generates OpenAPI 3.1 refs, auth scopes, envelopes, and removed-route absence from the route registry.

- [ ] **Step 1: Implement the owner route registry**

Implement `axon_web::schema_registry::rest_route_registry()` and generate OpenAPI from it. Do not generate OpenAPI from an `xtask` hard-coded route list. Do not mark OpenAPI as `ValidationOnly`; Phase 2 is incomplete until this family is registry-backed.

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
- Produces `RegistryBacked` status for each family listed below.
- Prevents fake completion through underspecified `xtask` schemas.

- [ ] **Step 1: Config**

Generate from an `axon-core` config registry that includes:

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

Add tests proving generated config/env schemas reject removed keys from `surface-removal-contract.md`, including stale `AXON_MCP_*` names.

- [ ] **Step 2: Provider capabilities**

Generate from provider capability registries that include:

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

Generate graph schema from the graph crate’s node/edge/evidence registries and validate edge kinds plus source-range/evidence requirements. The generated kind registry must be cross-checked against `docs/pipeline-unification/sources/source-graph.md`; no second hand-maintained graph-kind list is allowed.

- [ ] **Step 4: Runtime event schema**

Generate `docs/reference/runtime/events.schema.json` and matching markdown from `axon-observe::event_registry()`. The schema must include event structs, phase/status enum projections, metric descriptors, source inputs, fixtures, snapshots, and examples. Cross-check `PipelinePhase`, `LifecycleStatus`, and `JobKind` projections against `axon-api`.

- [ ] **Step 5: Error schema**

Generate `docs/reference/api/errors.schema.json` and matching markdown from `axon-error::error_registry()`. The schema must include error codes, stages, transport status mappings, redaction-safe examples, fixtures, snapshots, and drift checks. Cross-check every `ErrorStage` against `PipelinePhase` or an explicit contextual-boundary rule.

- [ ] **Step 6: Database schema**

Generate `docs/reference/runtime/database-schema.json` and matching markdown from migrations plus store table metadata. The schema must include table ownership, indexes, migration ids, canonical table inventory, legacy-table rejection, SQLite introspection fixtures, and store-owner parity. Reject legacy names such as `memory_decay`, `watch_events`, and `job_config_snapshots`.

- [ ] **Step 7: Vector payload schema**

Generate `docs/reference/sources/vector-payload.schema.json`, `docs/reference/sources/vector-payload.md`, and the Qdrant index plan from the vector payload registry and metadata field registry. The schema must use `source_generation`, never bare `generation`, and every generated index must target an existing payload field.

- [ ] **Step 8: Verify**

```bash
cargo test -p xtask config_removed_keys_are_rejected --no-fail-fast
cargo test -p xtask provider_schema_requires_contract_fields --no-fail-fast
cargo test -p xtask graph_schema_validates_edge_and_evidence_contracts --no-fail-fast
cargo test -p xtask event_schema_matches_api_enum_projections --no-fail-fast
cargo test -p xtask error_schema_stage_projection_is_explicit --no-fail-fast
cargo test -p xtask database_schema_rejects_legacy_tables --no-fail-fast
cargo test -p xtask vector_payload_schema_requires_source_generation --no-fail-fast
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
App clients: generated web/Palette/Android client schemas reference OpenAPI/API definitions and do not carry stale DTO copies
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

## Task 7A: Per-Crate Docs, App-Client Parity, And Dependency Snapshots

**Files:**
- Modify: `xtask/src/docs_contracts.rs`
- Modify: `xtask/src/layering.rs`
- Modify: `xtask/src/schemas/cross_check.rs`
- Modify: `xtask/src/schemas/tests.rs`
- Regenerate dependency graph snapshots only through the existing xtask workflow.

**Interfaces:**
- Consumes crate contract docs and generated artifact manifests.
- Consumes `cargo metadata` dependency graph data.
- Produces checks that keep checked Phase 2 issue rows from regressing while the unchecked fixture and aggregate-check rows are completed.

- [ ] **Step 1: Verify per-crate public API and generated-artifact docs**

Every crate contract that names generated docs or public API docs must be checked by `cargo xtask check-doc-contracts`. The check must prove each named artifact exists, is generated from the declared schema family when applicable, and has a matching source-input checksum entry.

- [ ] **Step 2: Verify app-client schema parity**

Add aggregate cross-check rows for generated app/client artifacts:

```text
web client schema -> OpenAPI/API DTO refs
Palette client schema -> OpenAPI/API DTO refs
Android client schema -> OpenAPI/API DTO refs
generated client removed-surface absence -> surface-removal contract
```

If a client artifact is intentionally absent in this phase, the check must require an explicit crate-contract exemption and cannot silently skip it in aggregate mode.

- [ ] **Step 3: Verify crate dependency graph snapshots**

Dependency graph snapshots must be generated from `cargo metadata`, not hand-maintained. Forbidden edges must fail CI, including transport crates importing domain internals or old crate/module names after removal.

- [ ] **Step 4: Verify**

```bash
cargo test -p xtask per_crate_generated_artifact_docs_are_checked --no-fail-fast
cargo test -p xtask app_client_artifacts_match_openapi_and_api_schemas --no-fail-fast
cargo test -p xtask dependency_graph_snapshots_reject_forbidden_edges --no-fail-fast
cargo xtask check-doc-contracts
cargo xtask check-layering
```

## Task 7B: Documentation Examples And Markdown Reference Parity

**Files:**
- Modify: `xtask/src/schemas/markdown.rs`
- Modify: `xtask/src/schemas/validate.rs`
- Modify: `xtask/src/schemas/tests.rs`
- Modify generated markdown artifacts under `docs/reference/**` only through generator output.

**Interfaces:**
- Consumes: `ArtifactIndex`, family reports, parsed JSON schemas.
- Produces: validation of every documented example and markdown generated from the same model as JSON.

- [ ] **Step 1: Validate documented examples**

Every `examples/` fixture and every JSON example embedded in generated markdown must validate against the schema it documents. Invalid examples must declare the expected failure category and must fail for that category.

- [ ] **Step 2: Enforce standard markdown sections**

Every generated markdown reference required by a family contract must include:

```text
<!-- generated by cargo xtask schemas <family>; do not edit directly -->
Overview
Generated Artifacts
Source Inputs
Root Shape
Required Definitions
Field Tables
Enum Tables
Extension Points
Forbidden Fields
Examples
Fixture Paths
Drift Checks
```

- [ ] **Step 3: Prevent hand-maintained markdown drift**

Markdown tables must be generated from the same artifact model as JSON. Add a test that mutates a schema field in a fixture registry and proves both JSON and Markdown check mode fail together.

- [ ] **Step 4: Verify**

```bash
cargo test -p xtask generated_markdown_has_required_sections --no-fail-fast
cargo test -p xtask documented_examples_validate_against_generated_schemas --no-fail-fast
cargo test -p xtask markdown_and_json_drift_together --no-fail-fast
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
cargo xtask schemas generate --print >/tmp/axon-schema-generator-report.json
cargo xtask check-doc-contracts
cargo xtask check-doc-links
cargo xtask check-layering
git diff --check
```

- [ ] **Step 4: Prove there are no deferred families**

Phase 2 cannot close while any family from `schemas/README.md` is not `RegistryBacked`. Add the final closeout assertion:

```text
api
cli
openapi
mcp
config
events
errors
database
graph
vector-payload
providers
```

Do not mark Phase 2 complete in issue #298 while any required source-of-truth family remains fake, validation-only, deferred, untracked, missing fixtures, missing snapshots, missing markdown where required, or missing cross-check coverage.

## Not In Scope

- Markdown polish for every generated reference page.
- Issue #298 body mutation.
- Generated client parity beyond proving generated contracts are internally valid and removed surfaces are absent.
- Adding an `adapters` schema family without a dedicated schema contract.

## Final Verification

```bash
cargo test -p xtask schemas:: --no-fail-fast
cargo xtask schemas generate --check
cargo xtask schemas generate --print >/tmp/axon-schema-generator-report.json
cargo xtask check-doc-contracts
cargo xtask check-doc-links
cargo xtask check-layering
git diff --check
```

Expected:

```text
all xtask schema tests pass
schema generate --check exits without writes
schema generate --print emits a valid machine-readable report
doc contract and doc link checks pass
layering check passes
git diff --check prints no whitespace errors
```
