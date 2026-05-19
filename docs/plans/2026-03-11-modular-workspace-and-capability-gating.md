# Modular Workspace + Capability Gating Plan
Last Updated: 2026-03-11

## Table of Contents
- [Goal](#goal)
- [Non-Goals](#non-goals)
- [Current Constraints (Observed)](#current-constraints-observed)
- [Target Architecture](#target-architecture)
- [Installation/Enablement UX](#installationenablement-ux)
- [Capability Gating Behavior](#capability-gating-behavior)
- [Key Design Decisions](#key-design-decisions)
- [Migration Phases](#migration-phases)
- [Testing Strategy](#testing-strategy)
- [Risks and Mitigations](#risks-and-mitigations)
- [Immediate Next Tasks](#immediate-next-tasks)

## Goal
Convert `axon_rust` from a single-package module tree into a true Cargo workspace with explicit capability gating so users can deploy only what they need:

- MCP crawler only
- MCP + RAG
- Web UI without jobs stack
- Full stack

## Non-Goals
- Runtime hot-plug crate installation (true plugin system)
- Backward-compatible support for every old env var and command shape in phase 1
- Zero-code movement migration (import churn is expected)

## Current Constraints (Observed)
- Repo is one package (`Cargo.toml` has `[package]`, no `[workspace]`).
- CLI config parser currently hard-requires `AXON_PG_URL`, `AXON_REDIS_URL`, `AXON_AMQP_URL` for normal command parsing.
- MCP exposes a broad unified action contract and handlers assume all capabilities are potentially available.
- `crawl.start` service path is job-enqueue-based today, not direct crawl-first.
- Web jobs endpoints read Postgres directly.
- Terminal already has separable wiring (`/ws/shell` via Node `node-pty` server rewrite).

## Target Architecture

### Workspace Layout
```text
axon_rust/
  Cargo.toml                      # [workspace]
  crates/
    axon-core/                    # config models, shared errors, logging, http safety
    axon-crawl/                   # scrape/map/sync crawl/screenshot (no jobs infra)
    axon-rag/                     # qdrant/tei/query/retrieve/ask/evaluate/suggest
    axon-jobs/                    # postgres+amqp+redis job system/workers
    axon-services/                # typed service facade over crawl/rag/jobs
    axon-mcp/                     # MCP server + schema + capability-aware routing
    axon-web-server/              # rust ws bridge, output/download, optional shell route
    axon-cli/                     # clap + command dispatch
  apps/web/                       # next.js UI (capability-aware panes/routes)
  bins/
    axon/                         # full/flexible binary
    axon-mcp/                     # optional slim mcp binary target
```

### Capability Model
Introduce a runtime struct in `axon-core`:

```rust
pub struct Capabilities {
    pub crawl_core: bool,
    pub rag_core: bool,
    pub jobs: bool,
    pub mcp: bool,
    pub web_server: bool,
    pub terminal_surface: bool,
    pub workspace_fs_surface: bool,
    pub docker_logs_surface: bool,
}
```

Resolution inputs:
- Compile-time feature flags
- Runtime env/config toggles

Single source of truth:
- CLI command availability
- MCP action availability
- Web API route registration
- Web pane visibility

## Installation/Enablement UX

### Build/Install-time (primary)
Users install a profile by feature set:

```bash
# crawler-only mcp
cargo install axon --no-default-features --features "mcp,crawl-core"

# mcp + rag + jobs
cargo install axon --no-default-features --features "mcp,crawl-core,rag-core,jobs"

# full
cargo install axon --features "full"
```

### Runtime toggles (secondary)
If compiled in, modules can still be disabled:

```bash
AXON_DISABLE_TERMINAL=true
AXON_DISABLE_WORKSPACE_FS=true
AXON_DISABLE_DOCKER_LOGS=true
```

## Capability Gating Behavior

### CLI
- Hidden or rejected commands when capability is absent.
- Deterministic error:
  - `module_not_enabled`
  - install hint (feature/profile needed)

### MCP
- `help/status` show only enabled actions.
- Disabled actions return typed error (`module_not_enabled`).
- Keep single `axon` tool initially; optional later split to per-domain tools.

### Web
- Route registration based on capability map.
- UI panes controlled by capabilities endpoint.
- Jobs pages unavailable without `jobs` capability.
- RAG panels unavailable without `rag_core` capability.
- Terminal/file panes independently togglable.

## Key Design Decisions

1. Keep jobs infra strictly coupled to jobs module.
- No attempt to make queue/db optional inside job code.

2. Add direct non-job crawl path for crawler-only mode.
- `mcp crawl.start` executes sync/direct path when `jobs == false`.
- Job lifecycle subactions unavailable without jobs capability.

3. Decouple config validation by command/capability.
- Required env vars validated only when command path uses that capability.

4. Prefer typed service boundaries over module reach-through.
- Prevent future cross-crate coupling regression.

## Migration Phases

### Phase 1: Workspace Bootstrap (No Behavior Change)
- Add root `[workspace]` and workspace deps.
- Create crate packages mirroring current module domains.
- Move code with compatibility re-exports where useful.
- Keep current runtime behavior intact.

Exit criteria:
- `cargo check`
- existing tests pass
- same CLI/MCP behavior as baseline

### Phase 2: Capability Registry + Config Decoupling
- Add `Capabilities` resolver.
- Replace global hard env requirements with capability-scoped validation.
- Wire CLI command guards.

Exit criteria:
- Non-job commands run without PG/Redis/AMQP configured.
- Job commands still fail fast with precise infra missing errors.

### Phase 3: MCP Gating + Crawler-Only Mode
- Add MCP action gating and help/status capability output.
- Implement direct crawl execution path for no-jobs profile.
- Gate ingest/embed/extract/refresh lifecycle actions by capability.

Exit criteria:
- `mcp+crawl-core` works without PG/Redis/AMQP.
- Disabled actions return deterministic module errors.

### Phase 4: Web Modular Surfaces
- Add backend `/api/capabilities` endpoint.
- Gate web API routes and panes:
  - jobs
  - terminal
  - workspace file explorer
  - logs
  - rag widgets

Exit criteria:
- Web assistant mode operates without Postgres/jobs.
- Terminal/file/log surfaces can be independently disabled.

### Phase 5: Packaging Profiles
- Ship profile docs and release artifacts:
  - `crawler-only`
  - `mcp-rag`
  - `web-assistant`
  - `full`

Exit criteria:
- Reproducible install docs for each profile.
- `doctor` and `status` report active capabilities clearly.

## Testing Strategy
- Unit tests for capability resolver and guard logic.
- Integration tests per profile:
  - crawler-only
  - mcp-rag
  - web-assistant
  - full
- MCP contract tests for gated/ungated action behavior.
- Web route tests verifying capability-based availability.

## Risks and Mitigations
- Import churn during crate split.
  - Mitigation: phase 1 compatibility shims and incremental moves.

- Hidden runtime assumptions in services.
  - Mitigation: profile-based integration tests before each phase cut.

- Drift between backend capabilities and frontend mode list.
  - Mitigation: backend-driven capabilities endpoint + UI mapping tests.

## Immediate Next Tasks
1. Create workspace `Cargo.toml` with member skeleton crates.
2. Define `Capabilities` in `axon-core` and add resolver scaffolding.
3. Move config parsing into command-scoped validation functions.
4. Implement MCP capability gate wrappers around existing handlers.
5. Add direct crawl service function for no-jobs mode.

