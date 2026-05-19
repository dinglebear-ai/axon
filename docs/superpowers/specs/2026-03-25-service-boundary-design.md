# Service Boundary Refactor Design

Date: 2026-03-25

## Goal

Enforce a strict architecture line:

- Consumers: `crates/cli`, `crates/mcp`, `crates/web`
- Owner of runtime/backend policy: `crates/services`
- Implementations: `crates/jobs`, `crates/crawl`, `crates/vector`, `crates/core`

The current codebase partially moved read/status operations into services, but write/start semantics, capability checks, and backend-specific policy still leak into consumers. This refactor removes that drift.

## Problems

Current violations:

- CLI commands still branch on `cfg.lite_mode` and directly use `JobBackend`.
- CLI owns async semantics in lite mode for `crawl` and `embed` by calling `wait_for_job()`.
- CLI and MCP both implement their own `export` policy and direct Postgres access.
- `watch` is in `services`, but still behaves like a facade over separate full/lite implementations and opens its own pool.
- `refresh schedule` policy is still enforced in CLI.
- `graph` capability policy is split between service and consumer layers.
- `lib.rs` still injects `Arc<dyn JobBackend>` into command handlers, which makes bypassing services easy.

## Target Architecture

### Boundary Rule

Consumers may:

- parse input
- call a service
- map typed service results/errors to transport output

Consumers may not:

- branch on `cfg.lite_mode` for runtime semantics
- open Postgres or SQLite directly
- call `jobs::*` directly
- implement backend-specific capability policy
- decide async worker ownership or fire-and-forget semantics

### Service Ownership

Services own:

- backend selection
- execution semantics
- capability policy
- unsupported-feature decisions
- status/progress mapping
- dependency wiring via `ServiceContext`

## ServiceContext

`ServiceContext` is constructed once in `lib.rs` and passed to consumers/services.

Proposed shape:

```rust
pub struct ServiceContext {
    pub cfg: Arc<Config>,
    pub capabilities: ServiceCapabilities,
    pub jobs: Arc<dyn JobRuntime>,
}
```

Optional later additions:

- pooled Postgres handle
- pooled SQLite handle
- shared AMQP publisher handle
- shared Qdrant/Neo4j clients

The important rule is that consumers receive `&ServiceContext`, not `Arc<dyn JobBackend>`.

## Typed Contracts

### Start Outcomes

Services return typed start outcomes instead of forcing consumers to infer behavior from mode:

```rust
pub enum JobStartOutcome<TCompleted> {
    Enqueued(JobEnqueued),
    Completed(TCompleted),
    Unsupported(ServiceError),
}
```

Shared enqueue metadata:

- `job_id`
- `kind`
- `execution_mode`
- optional `output_dir`
- optional predicted paths
- optional follow-up hints

### Status / Progress

Services expose typed status snapshots:

```rust
pub struct JobStatusSnapshot<TDetails> {
    pub state: JobState,
    pub phase: Option<String>,
    pub progress: JobProgress,
    pub message: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
    pub details: Option<TDetails>,
}
```

Shared progress shape:

- `percent: Option<f32>`
- `completed: Option<u64>`
- `total: Option<u64>`
- `unit: Option<String>`

Domain-specific detail payloads remain typed per command family.

### Typed Errors

Services return domain errors, not string-matched policy:

- `UnsupportedInCurrentMode`
- `MissingDependency`
- `BackendUnavailable`
- `ValidationFailed`
- `NotFound`
- `Conflict`
- `Internal`

CLI, MCP, and web map these separately, but the service decides the actual category.

### Capabilities

Services expose resolved capability state:

```rust
pub struct ServiceCapabilities {
    pub export: CapabilityState,
    pub graph: CapabilityState,
    pub refresh_schedule: CapabilityState,
    pub watch_scheduler: CapabilityState,
}
```

This supports:

- consistent help/UX
- consistent transport-layer errors
- future UI state for disabled features

## Refactor Scope

### Phase 1: Foundation

- Add `ServiceContext`
- Add typed service errors
- Add capability resolution
- Replace `Arc<dyn JobBackend>` in CLI dispatch with `&ServiceContext`

### Phase 2: Async Job Start/Status

Move all start semantics into services for:

- `crawl`
- `embed`
- `extract`
- `ingest`
- `refresh`

Consumers stop:

- calling `backend.enqueue(...)`
- calling `wait_for_job(...)`
- deciding lite/full async behavior

### Phase 3: Capability Policy

Move into services:

- `export`
- `refresh schedule`
- `graph`
- remaining `watch` policy

### Phase 4: Consumer Sweep

Sweep `cli`, `mcp`, and `web`:

- no direct `jobs::*`
- no direct `make_pool()`
- no direct `cfg.lite_mode` policy checks

### Phase 5: Watch and Graph Cleanup

- replace `services/watch.rs` facade-style branching with a cleaner backend/repository boundary
- centralize graph capability policy and status/build entrypoints

## Behavioral Decisions

### Async Semantics

The service layer defines whether a request:

- enqueues and returns
- runs synchronously and returns completed data
- is unsupported

This removes the current drift where lite `crawl` and `embed` behave differently only because CLI owns the flow.

### Worker Ownership

Worker ownership is also a service/runtime concern, not a CLI concern.

Consumers may render messages like:

- "enqueued"
- "completed"
- "unsupported"

They do not decide whether workers are in-process or out-of-process.

## Testing Strategy

Primary tests move to the service boundary:

- start outcome parity across lite/full
- capability decisions
- status/progress snapshots
- typed error mapping

Consumer tests narrow to:

- argument parsing
- output rendering
- transport mapping

Guardrails to add:

- tests preventing consumer-owned lite/full drift
- tests for shared `export` behavior across CLI/MCP/web

## Immediate High-Risk Violations To Fix First

1. `crates/cli/commands/crawl.rs`
   - direct lite enqueue
   - direct `wait_for_job()`
   - embed-drain polling in CLI
2. `crates/cli/commands/embed.rs`
   - direct lite enqueue
   - direct `wait_for_job()`
3. `crates/cli/commands/extract.rs`
   - direct lite enqueue path
4. `crates/cli/commands/ingest.rs`
   - direct lite enqueue path
5. `crates/cli/commands/export.rs`
   - direct Postgres access
   - local lite-mode policy
6. `crates/mcp/server/handlers_system.rs`
   - duplicate `export` policy
   - direct Postgres access

## Non-Goals

This refactor does not require:

- rewriting all job implementation modules
- eliminating all full/lite backend differences internally
- changing wire output formats unless needed for correctness

The goal is strict ownership, not flattening every implementation detail.

## Success Criteria

The refactor is complete when:

- consumers no longer branch on backend mode for behavior
- consumers no longer call `jobs::*` directly
- consumers no longer open data stores directly
- services own capability policy and execution semantics
- start/status/progress/error contracts are typed and shared
- `lib.rs` no longer passes `JobBackend` into command handlers
