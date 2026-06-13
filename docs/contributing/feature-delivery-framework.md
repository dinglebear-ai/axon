# Feature Delivery Framework
Last Modified: 2026-06-01

## Purpose

This document is the source of truth for bringing new Axon features online.

Goals:
- Enforce one implementation pattern for all new features.
- Keep business logic out of CLI/MCP/Web adapters.
- Make feature rollout predictable across one or more surfaces (CLI, MCP, Web).
- Define objective quality gates before a feature is considered complete.

Change control:
- If delivery architecture, surface routing rules, or quality gates change, update this file in the same PR.
- If this file and another delivery/process doc conflict, this file takes precedence until harmonized.

## Scope

Applies to all net-new capabilities added after this document.

Does not require immediate refactors of existing legacy command paths. Existing behavior remains valid unless explicitly migrated.

## Architecture Standard (New Features)

### Rule 1: Service-First

All new feature logic must live in `src/services/*`.

Adapters must be thin:
- CLI: argument/flag mapping + output formatting only.
- MCP: schema validation + response envelope only.
- Web: request parsing + stream/event forwarding only.

### Rule 2: Single Orchestrator per Feature

Each feature has one orchestration entrypoint in services (for example, `run_fastlearn(...)`).

That orchestrator owns:
- execution lifecycle,
- timing and metrics,
- streaming events,
- retries/timeouts,
- graceful degradation policy,
- final result payload.

### Rule 3: Shared Contracts

Define feature contracts once in services and reuse across adapters:
- request struct,
- event enum for progress streaming,
- result struct,
- error enum.

Contract stability requirements:
- Keep field names stable across CLI JSON output, MCP payloads, and web events.
- If a breaking contract change is required, include a migration note in the feature PR and docs.

Adapters should map to/from these contracts, not create parallel feature-specific models.

## Surface Decision Matrix

Use this matrix before implementation:

| Surface | Use when | Must include |
|---|---|---|
| CLI only | Operator-first capability, local workflows, scripting | command routing, human output, `--json` output parity |
| MCP only | Tooling/agent integration only | schema enum entry, handler branch, response_mode policy |
| Web only | UI-native interaction not exposed as command | `src/web` route binding, panel UX |
| CLI + MCP | Same capability needed by humans and agents | shared service, thin wrappers, output/schema parity |
| CLI + Web | Feature needs terminal and UI visibility | shared service, consistent progress semantics |
| MCP + Web | Agent and UI workflows, no shell requirement | shared service, consistent event model |
| CLI + MCP + Web | Core platform capability | one service orchestrator, all adapters thin |

Default stance: if unclear, implement `CLI + MCP` first using a shared service. Add Web if a concrete UI flow exists.

## Delivery Lifecycle

### Phase 0: Feature Classification

Classify the feature before coding:
- synchronous vs queue-backed async,
- deterministic vs best-effort,
- read-only vs mutating,
- external dependency requirements,
- surfaces required (CLI/MCP/Web).

### Phase 1: Contract Design

Define service contracts first:
- `FeatureRequest`
- `FeatureEvent`
- `FeatureResult`
- `FeatureError`

Design event taxonomy before implementation to guarantee streamability.

### Phase 2: Service Implementation

Implement only in `src/services` first.

Required in service layer:
- total and phase timing (ms),
- progress event emitter,
- cancellation checks for long-running operations,
- bounded concurrency,
- graceful partial-failure handling where required,
- deterministic final result payload.

### Phase 3: Adapter Wiring

Wire each selected surface as a thin wrapper.

### Phase 4: Validation

Run unit + integration + compile checks, then docs update.

### Phase 5: Rollout

Ship only after Definition of Done passes.

## File-Level Integration Checklist

### Shared Service Layer (Required for all new features)

1. Add module and exports:
- `src/services.rs`
- `src/services/<feature>.rs`

Also wire the module graph:
- `src/lib.rs` / module roots as needed (`pub mod services;`)

2. Add service contracts and orchestrator:
- request/event/result/error types
- `run_<feature>(...)`

3. Keep direct side-effects isolated behind helper functions.

### CLI Integration (if selected)

1. Add command handler:
- `src/cli/commands/<feature>.rs`

2. Export in:
- `src/cli/commands.rs`

3. Route command in:
- `lib.rs` (`run_once` match arm)

4. Add command kind + parser wiring:
- `src/core/config/types/enums.rs` (`CommandKind`)
- `src/core/config/cli.rs` (clap spec)
- `src/core/config/parse.rs` (arg -> `Config` mapping)

5. Ensure CLI output modes:
- human-readable mode
- `--json` mode with stable machine contract

### MCP Integration (if selected)

1. Add schema request shape:
- `src/mcp/schema.rs` (`AxonRequest` + request struct)

2. Add server handler route:
- `src/mcp/server.rs` (`handle_<feature>` and match arm)

3. Follow MCP envelope policy:
- `ok/action/subaction/data`
- `response_mode` behavior (`path|inline|both`)

4. Keep MCP discoverability in sync:
- update `handle_help` action map in `src/mcp/server.rs` for new actions/subactions.

5. Update docs:
- `docs/reference/mcp/tool-schema.md`
- `docs/reference/mcp/overview.md` if behavior/usage changes

### Web Integration (if selected)

1. Route to service from web runtime:
- Axum runtime: `src/web.rs`, `src/web/server.rs`, `src/web/server/routing.rs`, and the REST handlers under `src/web/server/handlers/`
- Static panel assets: `apps/web/app/**` when the browser-facing setup/config UI changes

2. Keep long-running operations server-owned. Browser and CLI clients should
receive job IDs, artifact handles, or action responses rather than relying on
host-local output paths.

3. Keep transport mapping in the web layer; business logic stays in services.

### Docs Integration (Always)

1. Add command/feature doc when user-facing:
- `docs/reference/actions/<feature>.md`

2. Update indexes/references:
- `docs/README.md`
- repository `README.md` feature/command tables if needed

3. For major behavior additions, update:
- `docs/architecture/overview.md`

## Streaming Standard

For long-running work (>3s expected), progress visibility is required.

Minimum stream event types:
- `started`
- `phase_started`
- `progress`
- `phase_completed`
- `warning`
- `error`
- `completed`

Rules:
- For streaming-capable surfaces (CLI, Web), emit heartbeat/progress at a steady cadence while waiting on external systems.
- For non-streaming surfaces (MCP), return phase/timing metadata and artifact pointers so clients can show activity and poll follow-up state when relevant.
- Include elapsed timing per phase and total timing in final result.
- Never leave users with silent waits.

## Reliability and Degradation Standard

Define failure policy explicitly per external dependency:
- hard-fail dependency: abort feature
- best-effort dependency: warn and continue

For best-effort paths:
- track failures in result payload,
- include counts and representative errors,
- keep primary outcome successful when appropriate.

## Testing Standard

Minimum required tests for new features:

1. Service unit tests:
- happy path,
- partial failure path,
- timeout/cancellation path,
- deterministic payload shape.

2. Adapter tests:
- CLI argument mapping and `--json` contract,
- MCP request parsing and response envelope,
- Web transport mapping (if applicable).

3. Regression tests:
- prove existing commands/actions remain intact when feature is additive.

4. Validation commands:
- `cargo fmt --all`
- `cargo check -q`
- targeted `cargo test <feature-or-module>`

## Definition of Done

A feature is complete only when all are true:

1. Core logic is implemented in `src/services`.
2. Selected adapters are thin and wired.
3. Streaming/progress is visible for long-running steps.
4. Timing is captured and surfaced in outputs.
5. Degradation policy is implemented and tested.
6. Existing behaviors remain unchanged unless explicitly intended.
7. Docs are updated across command/MCP/architecture surfaces as needed.
8. Compile and tests pass.

## PR Review Checklist

Use this checklist before merge:

- Is this feature service-first, or did logic leak into adapters?
- Is there exactly one orchestration path reused by all surfaces?
- Are CLI/MCP/Web contracts consistent with the same service result model?
- Is streaming visible and frequent during slow operations?
- Are timing fields present and accurate?
- Is graceful degradation explicit and observable?
- Are docs and help/schema entries updated?
- Are tests covering success + failure + partial-failure paths?

## Migration Guidance for Legacy Paths

Legacy feature paths can remain as-is until scheduled refactor work.

When touching a legacy command significantly:
- prefer extracting new logic into `src/services` instead of expanding legacy adapter logic,
- migrate incrementally (service extraction first, adapter simplification second),
- preserve external command behavior unless a deliberate breaking change is approved.

## Initial Implementation Notes

To establish this pattern immediately:
- Create `src/services/` for all net-new capabilities starting now.
- Keep existing `research` behavior intact while introducing new service-based capabilities.
- Use this framework as the checklist for `fastlearn` and future features.
