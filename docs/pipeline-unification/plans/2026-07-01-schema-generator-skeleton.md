# Schema Generator Skeleton Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add PR3's schema generator skeleton with deterministic JSON/Markdown artifacts, source-input checksums, enum drift checks, removed-surface drift checks, and stale-artifact failure behavior.

**Architecture:** `xtask` owns schema generation and drift checking. API/error schema families use real Rust-owned schemas from `axon-api` and `axon-error`; all remaining required families get explicit skeleton artifacts with owner/source metadata so later PRs can replace internals without changing the command contract.

**Tech Stack:** Rust 2024, `clap`, `serde_json`, `schemars`, `sha2`, `anyhow`, existing `xtask` checks.

## Global Constraints

- Do not edit any `CLAUDE.md` files.
- No compatibility aliases for removed CLI commands, MCP actions, REST routes, DTO fields, or config keys.
- Keep files under 500 LOC.
- Use sibling test files, not inline tests.
- Do not wire public CLI/MCP/REST behavior in this PR.
- Do not start Phase 3 stores/providers/fakes.
- Commit early and keep each commit independently reviewable.

---

### Task 1: Command Contract And Failing Tests

**Files:**
- Modify: `/home/jmagar/workspace/axon/xtask/Cargo.toml`
- Modify: `/home/jmagar/workspace/axon/xtask/src/main.rs`
- Create: `/home/jmagar/workspace/axon/xtask/src/schemas.rs`
- Create: `/home/jmagar/workspace/axon/xtask/src/schemas/tests.rs`

**Interfaces:**
- Produces: `schemas::run(root: &Path, args: SchemasArgs) -> anyhow::Result<()>`
- Produces: `cargo xtask schemas generate --check`

- [ ] Write tests proving check mode fails when artifacts are missing.
- [ ] Write tests proving generation writes all required family JSON and Markdown artifacts.
- [ ] Wire `Command::Schemas(schemas::SchemasArgs)` in `xtask/src/main.rs`.
- [ ] Add `axon-api`, `axon-error`, and `schemars` as `xtask` dependencies.
- [ ] Run `cargo test -p xtask schemas --locked` and commit.

### Task 2: Artifact Model And Source Checksums

**Files:**
- Create: `/home/jmagar/workspace/axon/xtask/src/schemas/artifact.rs`
- Create: `/home/jmagar/workspace/axon/xtask/src/schemas/source_input.rs`
- Modify: `/home/jmagar/workspace/axon/xtask/src/schemas.rs`
- Test: `/home/jmagar/workspace/axon/xtask/src/schemas/tests.rs`

**Interfaces:**
- Produces: `SchemaArtifact { path, content }`
- Produces: `SourceInput { path, sha256 }`
- Produces: deterministic `x-axon.source_inputs` records.

- [ ] Write tests proving generated JSON contains `x-axon.source_inputs` with checksums.
- [ ] Implement source-input hashing from repo-relative paths.
- [ ] Implement write/check behavior where check mode never writes.
- [ ] Run `cargo test -p xtask schemas --locked` and commit.

### Task 3: Family Generators And Drift Checks

**Files:**
- Create: `/home/jmagar/workspace/axon/xtask/src/schemas/families.rs`
- Create: `/home/jmagar/workspace/axon/xtask/src/schemas/registry.rs`
- Modify: `/home/jmagar/workspace/axon/xtask/src/schemas.rs`
- Test: `/home/jmagar/workspace/axon/xtask/src/schemas/tests.rs`

**Interfaces:**
- Produces all required families: `api`, `cli`, `openapi`, `mcp`, `config`, `events`, `errors`, `database`, `graph`, `vector-payload`, `providers`.
- Produces enum drift check for canonical enum values.
- Produces removed-surface drift check for old commands/actions/routes/config keys/DTO fields.

- [ ] Write tests proving enum values appear in the API schema bundle.
- [ ] Write tests proving removed names are absent from generated artifacts.
- [ ] Generate real `SourceRequest`, `SourceResult`, `ResolvedSource`, and `ApiError` schemas.
- [ ] Generate skeleton JSON/Markdown for the remaining families.
- [ ] Run `cargo test -p xtask schemas --locked` and commit.

### Task 4: Generate Artifacts And Gate The Issue

**Files:**
- Create/update: `/home/jmagar/workspace/axon/docs/reference/api/schemas.json`
- Create/update: `/home/jmagar/workspace/axon/docs/reference/api/dto.md`
- Create/update: `/home/jmagar/workspace/axon/docs/reference/api/errors.schema.json`
- Create/update: `/home/jmagar/workspace/axon/docs/reference/api/errors.md`
- Create/update: required family artifacts under `/home/jmagar/workspace/axon/docs/reference/**`

**Interfaces:**
- Produces: `cargo xtask schemas generate`
- Produces: `cargo xtask schemas generate --check`

- [ ] Run `cargo xtask schemas generate`.
- [ ] Run `cargo xtask schemas generate --check`.
- [ ] Run `cargo test -p xtask schemas --locked`.
- [ ] Run `cargo test -p axon-api source --locked`.
- [ ] Run `cargo xtask check-layering`.
- [ ] Run `cargo xtask check-repo-structure`.
- [ ] Review issue #298 Phase 2 and PR3 checklist before PR merge.
- [ ] Commit generated artifacts and code.
