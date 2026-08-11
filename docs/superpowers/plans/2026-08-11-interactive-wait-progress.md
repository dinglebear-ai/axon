# Interactive Wait Progress Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give every foreground Axon source/extract operation a quiet, Aurora-themed hybrid progress display with one live region, useful phase/count/timing context, grouped actionable warnings, and unchanged machine-output contracts.

**Architecture:** Add a transport-neutral two-lane foreground feed to `axon-services`: a Tokio `watch` lane coalesces the latest `JobStatusUpdate`, while a bounded Tokio `mpsc` lane carries ordered `SourceProgressEvent` facts and reports overflow for durable reconciliation. `axon-cli` reduces those facts into a pure `WaitViewModel`, formats it responsively with Aurora product CLI tokens, and renders it through one `indicatif::MultiProgress` region on stderr; final command results remain on stdout.

**Tech Stack:** Rust 1.97.1 / edition 2024, Tokio `watch` + bounded `mpsc`, `indicatif` 0.18, existing `axon-api` progress DTOs, unified SQLite `JobStore`, Aurora product CLI token helpers in `axon-core::ui`, sidecar Rust tests.

## Global Constraints

- Work only in `/home/jmagar/workspace/axon/.worktrees/secret-redaction-investigation` on `codex/secret-redaction-investigation`.
- Preserve every pre-existing secret-redaction modification and untracked investigation artifact; stage only files named by the current task.
- Progress facts remain presentation-free outside `axon-cli`; no ANSI, terminal width, copy, throttling, or display policy enters `axon-api`, `axon-services`, `axon-jobs`, or `axon-observe`.
- Interactive progress writes only to stderr. Final human or JSON results keep their existing stdout contract.
- `--json` and `--quiet` construct no renderer. Redirected stderr emits no animation or cursor-control sequences.
- Use Axon's Aurora **product CLI** palette: cyan `#29b6f6`, rose `#f9a8c4`, violet `#a78bfa`, primary `#e6f4fb`, muted `#a7bcc9`, success `#7dd3c7`, warning `#c6a36b`, error `#c78490`, info `#72c8f5`, neutral `#91a8b6`.
- Visible redraw cadence is at most one update per 250 ms, except immediate phase/warning/retry/failure/terminal transitions.
- Redaction copy is neutral: `secret policy held N chunks`; never claim held chunks prove secrets and never print payload values.
- No invented overall pipeline percentage. Show a bar only when the active phase has `total > 0`.
- A full event lane never blocks pipeline work: set overflow, reconcile durable events, deduplicate by stable `event_id`, then resume by durable sequence.
- `status --watch` remains behaviorally unchanged in this plan; reuse of the new formatter there is a follow-up.
- Do not promise `job continues` for inline execution. Show `job retained · axon jobs get <id>` after interruption unless worker-backed continuation is proven.
- Never add `mod.rs`. New tests use sibling `_tests.rs` files with source-side `#[cfg(test)] #[path = "..."] mod tests;` declarations.
- Changed Rust source files must stay at or below 500 lines and functions below the 120-line hard limit.
- Follow strict TDD: failing focused test, observed failure, minimal implementation, passing focused test, then commit.
- Use `--local` for manual CLI verification so a running server cannot hide the newly built binary.

## Context and Existing Contracts

Read before implementation:

- Design: `docs/superpowers/specs/2026-08-11-interactive-wait-progress-design.md`
- CLI ownership: `crates/axon-cli/src/CLAUDE.md`
- Service ownership: `crates/axon-services/src/CLAUDE.md`
- Existing UI tokens/spinner: `crates/axon-core/src/ui.rs`
- Source orchestration: `crates/axon-services/src/source.rs`
- Event persistence: `crates/axon-services/src/source/events.rs`
- Monotonic progress: `crates/axon-services/src/source/executor/progress.rs`
- Existing phase summaries: `crates/axon-cli/src/commands/job_progress.rs`
- Existing live renderer: `crates/axon-cli/src/commands/status/watch.rs`
- Current wait call sites: `crates/axon-cli/src/commands/source.rs`, `sessions.rs`, and `extract.rs`

Verified live interfaces:

- `JobStatusUpdate: Clone` contains `job_id`, `status`, `phase`, optional `counts`, `current`, `message`, and `error`.
- `SourceProgressEvent: Clone` contains stable `event_id`, job/phase/status/severity, counts/current/retry/warning/error, and public visibility.
- `JobStore::append_event` stamps durable sequence internally but returns `Result<()>`; foreground clones therefore retain `sequence = 0` and reconcile by `event_id` before they have a durable cursor.
- `axon_services::jobs::unified_job_events(ctx, JobEventListRequest)` supports `after_sequence`, pagination cursor, severity, visibility, and phase filters.
- `ProgressCoordinator` already normalizes/persists snapshots every 250 ms.
- `source_progress_summary(&ServiceJob)` already owns source-specific unit wording and percentage behavior; extract equivalent exists beside it.
- `extract_sync` executes up to 16 URLs concurrently and currently exposes only a generic spinner.

## File Structure

| File | Responsibility |
|---|---|
| `crates/axon-services/src/source/foreground_progress.rs` | bounded two-lane source progress feed and overflow state |
| `crates/axon-services/src/source/foreground_progress_tests.rs` | feed coalescing/backpressure tests |
| `crates/axon-services/src/source/execution.rs` | carry optional sender through source execution |
| `crates/axon-services/src/source/events.rs` | fan out structured event clones beside durable append |
| `crates/axon-services/src/source/executor.rs` | announce canonical job id and attach sender to emitter/coordinator |
| `crates/axon-services/src/source/executor/progress.rs` | publish normalized snapshots to the foreground feed |
| `crates/axon-cli/src/commands/wait_progress.rs` | module facade, enablement policy, session driver |
| `crates/axon-cli/src/commands/wait_progress/model.rs` | pure reducer, milestones, aggregation, batch summary |
| `crates/axon-cli/src/commands/wait_progress/timing.rs` | rolling rate/ETA estimator |
| `crates/axon-cli/src/commands/wait_progress/format.rs` | responsive plain/Aurora line formatting and sanitization |
| `crates/axon-cli/src/commands/wait_progress/render.rs` | `indicatif` stderr live-region implementation |
| matching `*_tests.rs` sidecars | deterministic tests for each focused unit |
| `crates/axon-core/src/ui.rs` | missing Aurora `info`/`neutral` helpers and stderr color decision |
| `crates/axon-cli/src/commands/source.rs` | attach renderer to foreground source execution and waited batches |
| `crates/axon-cli/src/commands/sessions.rs` | use the same source wait session per selected root |
| `crates/axon-services/src/extract/sync.rs` | emit genuine coarse URL/item snapshots |
| `crates/axon-cli/src/commands/extract.rs` | replace generic wait spinner with shared renderer |

---

### Task 1: Build the bounded foreground progress feed

**Files:**
- Create: `crates/axon-services/src/source/foreground_progress.rs`
- Create: `crates/axon-services/src/source/foreground_progress_tests.rs`
- Modify: `crates/axon-services/src/source.rs`

**Interfaces:**
- Consumes: `JobId`, `JobStatusUpdate`, and `SourceProgressEvent` from `axon_api::source`.
- Produces: `foreground_progress_channel() -> (ForegroundProgressSender, ForegroundProgressReceiver)`, `ForegroundSnapshot::{JobStarted, Status}`, and receiver overflow/cursor state used by Tasks 2, 5, and 6.

- [x] **Step 1: Declare the module and write failing feed tests**

Add `pub mod foreground_progress;` beside the other source modules in `source.rs`. Create the test sidecar with these behavioral assertions:

```rust
use super::*;
use axon_api::source::{
    JobId, JobStatusUpdate, LifecycleStatus, PipelinePhase, Severity,
    SourceProgressEvent,
};
use uuid::Uuid;

fn test_event(event_id: &str) -> SourceProgressEvent {
    let mut event = SourceProgressEvent::minimal(
        JobId::new(Uuid::from_u128(1)),
        0,
        PipelinePhase::Embedding,
        LifecycleStatus::Running,
        Severity::Info,
        "embedding chunks",
    );
    event.event_id = event_id.to_string();
    event
}

fn update(done: u64) -> JobStatusUpdate {
    JobStatusUpdate {
        job_id: JobId::new(Uuid::from_u128(1)),
        source_id: None,
        status: LifecycleStatus::Running,
        phase: PipelinePhase::Embedding,
        stage_id: None,
        counts: Some(axon_api::source::StageCounts {
            chunks_total: Some(10),
            chunks_done: done,
            ..Default::default()
        }),
        current: None,
        message: Some("embedding chunks".into()),
        error: None,
    }
}

#[tokio::test]
async fn snapshot_lane_keeps_only_the_latest_value() {
    let (tx, mut rx) = foreground_progress_channel_with_capacity(2);
    tx.snapshot(update(1));
    tx.snapshot(update(7));
    rx.snapshots.changed().await.unwrap();
    assert_eq!(
        rx.snapshots.borrow().as_ref().unwrap().status().unwrap().counts
            .as_ref().unwrap().chunks_done,
        7,
    );
}

#[tokio::test]
async fn full_event_lane_sets_overflow_without_blocking() {
    let (tx, rx) = foreground_progress_channel_with_capacity(1);
    assert!(tx.event(test_event("evt_1")));
    assert!(!tx.event(test_event("evt_2")));
    assert!(rx.overflowed());
}

#[test]
fn taking_overflow_flag_is_edge_triggered() {
    let (tx, rx) = foreground_progress_channel_with_capacity(1);
    assert!(tx.event(test_event("evt_1")));
    assert!(!tx.event(test_event("evt_2")));
    assert!(rx.take_overflowed());
    assert!(!rx.take_overflowed());
    rx.mark_overflowed();
    assert!(rx.take_overflowed());
}
```

- [x] **Step 2: Run the focused test and observe the missing module/types**

Run: `cargo test -p axon-services source::foreground_progress -- --nocapture`

Expected: FAIL because `foreground_progress.rs` and its types do not exist.

- [x] **Step 3: Implement the two-lane feed**

Create these concrete types and methods:

```rust
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use axon_api::source::{JobId, JobStatusUpdate, SourceProgressEvent};
use tokio::sync::{mpsc, watch};

pub const FOREGROUND_EVENT_CAPACITY: usize = 256;

#[derive(Debug, Clone, PartialEq)]
pub enum ForegroundSnapshot {
    JobStarted(JobId),
    Status(JobStatusUpdate),
}

impl ForegroundSnapshot {
    pub fn job_id(&self) -> JobId {
        match self { Self::JobStarted(id) => *id, Self::Status(update) => update.job_id }
    }

    pub fn status(&self) -> Option<&JobStatusUpdate> {
        match self { Self::Status(update) => Some(update), Self::JobStarted(_) => None }
    }
}

#[derive(Clone)]
pub struct ForegroundProgressSender {
    snapshots: watch::Sender<Option<ForegroundSnapshot>>,
    events: mpsc::Sender<SourceProgressEvent>,
    overflow: Arc<AtomicBool>,
}

pub struct ForegroundProgressReceiver {
    pub snapshots: watch::Receiver<Option<ForegroundSnapshot>>,
    pub events: mpsc::Receiver<SourceProgressEvent>,
    overflow: Arc<AtomicBool>,
}

pub fn foreground_progress_channel() -> (ForegroundProgressSender, ForegroundProgressReceiver) {
    foreground_progress_channel_with_capacity(FOREGROUND_EVENT_CAPACITY)
}

fn foreground_progress_channel_with_capacity(capacity: usize)
    -> (ForegroundProgressSender, ForegroundProgressReceiver)
{
    let (snapshot_tx, snapshot_rx) = watch::channel(None);
    let (event_tx, event_rx) = mpsc::channel(capacity.max(1));
    let overflow = Arc::new(AtomicBool::new(false));
    (
        ForegroundProgressSender {
            snapshots: snapshot_tx,
            events: event_tx,
            overflow: Arc::clone(&overflow),
        },
        ForegroundProgressReceiver {
            snapshots: snapshot_rx,
            events: event_rx,
            overflow,
        },
    )
}

impl ForegroundProgressSender {
    pub fn job_started(&self, job_id: JobId) {
        self.snapshots.send_replace(Some(ForegroundSnapshot::JobStarted(job_id)));
    }

    pub fn snapshot(&self, update: JobStatusUpdate) {
        self.snapshots.send_replace(Some(ForegroundSnapshot::Status(update)));
    }

    pub fn event(&self, event: SourceProgressEvent) -> bool {
        match self.events.try_send(event) {
            Ok(()) => true,
            Err(mpsc::error::TrySendError::Full(_)) => {
                self.overflow.store(true, Ordering::Release);
                false
            }
            Err(mpsc::error::TrySendError::Closed(_)) => false,
        }
    }
}

impl ForegroundProgressReceiver {
    pub fn overflowed(&self) -> bool { self.overflow.load(Ordering::Acquire) }
    pub fn take_overflowed(&self) -> bool { self.overflow.swap(false, Ordering::AcqRel) }
    pub fn mark_overflowed(&self) { self.overflow.store(true, Ordering::Release); }
}

#[cfg(test)]
#[path = "foreground_progress_tests.rs"]
mod tests;
```

Keep `foreground_progress_channel_with_capacity` private. Its sidecar is a
child module and reaches the private helper through `use super::*;`.

- [x] **Step 4: Run the focused tests**

Run: `cargo test -p axon-services source::foreground_progress -- --nocapture`

Expected: PASS for snapshot coalescing, bounded overflow, and edge-triggered overflow clearing.

- [x] **Step 5: Commit only Task 1 files**

```bash
git add crates/axon-services/src/source.rs \
  crates/axon-services/src/source/foreground_progress.rs \
  crates/axon-services/src/source/foreground_progress_tests.rs
git commit -m "feat(progress): add bounded foreground source feed"
```

---

### Task 2: Fan out source job identity, snapshots, and events

**Files:**
- Modify: `crates/axon-services/src/source.rs:74-112`
- Modify: `crates/axon-services/src/source/execution.rs:1-37`
- Modify: `crates/axon-services/src/source/events.rs:10-297`
- Modify: `crates/axon-services/src/source/events_tests.rs`
- Modify: `crates/axon-services/src/source/executor.rs:52-86`
- Modify: `crates/axon-services/src/source/executor/progress.rs:40-180`
- Modify: `crates/axon-services/src/source/executor/progress_tests.rs`

**Interfaces:**
- Consumes: `ForegroundProgressSender` from Task 1.
- Produces: `index_source_with_progress(request, ctx, sender)`, event fan-out, canonical `job_started`, and normalized snapshot fan-out consumed by Task 6.

- [x] **Step 1: Add failing event and snapshot fan-out tests**

Extend `events_tests.rs`:

```rust
#[tokio::test]
async fn emitter_forwards_the_same_safe_event_to_foreground_consumers() {
    let (store, job_id) = store_with_job().await;
    let (tx, mut rx) = crate::source::foreground_progress::foreground_progress_channel();
    emitter(store, job_id)
        .with_foreground(tx)
        .warning(
            PipelinePhase::Preparing,
            SourceWarning {
                code: "secret_redaction_forbidden".into(),
                severity: Severity::Degraded,
                message: "secret policy held a chunk".into(),
                source_item_key: None,
                retryable: false,
            },
            None,
        )
        .await;

    let event = rx.events.recv().await.unwrap();
    assert_eq!(event.phase, PipelinePhase::Preparing);
    assert_eq!(event.warning.unwrap().code, "secret_redaction_forbidden");
    assert!(!event.message.contains("payload"));
}
```

Extend `progress_tests.rs` by adding an optional feed to the test coordinator and asserting the latest normalized snapshot is delivered even when durable persistence fails:

```rust
#[tokio::test]
async fn normalized_snapshot_reaches_foreground_feed_when_store_write_fails() {
    let writer = Arc::new(RecordingWriter { updates: Mutex::new(Vec::new()), fail: true });
    let (tx, mut rx) = crate::source::foreground_progress::foreground_progress_channel();
    let coordinator = ProgressCoordinator::with_writer_and_foreground(
        writer,
        JobId::new(uuid::Uuid::from_u128(1)),
        SourceId::new("src-progress-test"),
        "local",
        Duration::ZERO,
        Some(tx),
    );
    coordinator.checkpoint(
        PipelinePhase::Embedding,
        stage_counts(Some(2), 2, Some(2), 2, Some(10), 3),
        "embedding chunks",
    ).await;

    rx.snapshots.changed().await.unwrap();
    let snapshot = rx.snapshots.borrow().clone().unwrap();
    assert_eq!(snapshot.status().unwrap().counts.as_ref().unwrap().chunks_done, 3);
}
```

- [x] **Step 2: Run both focused suites and observe missing wiring**

Run:

```bash
cargo test -p axon-services source::events::tests -- --nocapture
cargo test -p axon-services source::executor::progress::tests -- --nocapture
```

Expected: FAIL because `with_foreground`, `with_writer_and_foreground`, and source execution progress plumbing do not exist.

- [x] **Step 3: Carry the optional sender through execution and expose the CLI entrypoint**

Add `foreground: Option<ForegroundProgressSender>` to `SourceExecutionContext`; existing `inline` and `existing_job` constructors set it to `None`. Add:

```rust
pub(crate) fn inline_with_progress(
    request: SourceRequest,
    auth_snapshot: Option<AuthSnapshot>,
    foreground: ForegroundProgressSender,
) -> Self {
    let mut execution = Self::inline(request, auth_snapshot);
    execution.foreground = Some(foreground);
    execution
}
```

In `source.rs`, add the public trusted-local wrapper:

```rust
pub async fn index_source_with_progress(
    request: SourceRequest,
    ctx: &ServiceContext,
    foreground: foreground_progress::ForegroundProgressSender,
) -> anyhow::Result<SourceResult> {
    let execution = SourceExecutionContext::inline_with_progress(
        request.clone(),
        Some(AuthSnapshot::trusted_cli(env!("CARGO_PKG_VERSION"))),
        foreground,
    );
    index_source_inner(request, ctx, execution).await
}
```

- [x] **Step 4: Fan out event facts without changing persistence authority**

Add `foreground: Option<ForegroundProgressSender>` to `SourceEventEmitter`, preserve it through all builder methods, and implement `.with_foreground(sender)`. Refactor `emit_source_event` into a pure `build_source_event(...) -> SourceProgressEvent` plus append/send orchestration. Construct `context` from the same fields currently assembled inline, then use this exact persistence/fanout ordering:

```rust
let event = build_source_event(job_id, phase, status, severity, context);
let persisted = jobs.append_event(event.clone()).await;
if let Some(foreground) = &self.foreground {
    foreground.event(event);
}
if let Err(err) = persisted {
    tracing::warn!(job_id = %job_id.0, phase = %phase_label, error = %err,
        "failed to emit source progress event");
}
```

Do not gate foreground delivery on successful persistence. Keep the existing redaction/public visibility fields unchanged.

- [x] **Step 5: Announce the canonical job and fan out normalized snapshots**

In `index_materialized_source`, immediately after the create/existing-id branch:

```rust
if let Some(foreground) = &input.execution.foreground {
    foreground.job_started(job_id);
}
```

Attach the sender to the executor's `SourceEventEmitter`. Add `foreground: Option<ForegroundProgressSender>` to `ProgressCoordinator`; after normalizing `counts`, build one `JobStatusUpdate`, clone it for `writer.update(update.clone())`, then call `foreground.snapshot(update)` regardless of the persistence result.

- [x] **Step 6: Run focused and source compilation tests**

Run:

```bash
cargo test -p axon-services source::events::tests -- --nocapture
cargo test -p axon-services source::executor::progress::tests -- --nocapture
cargo test -p axon-services --no-run --lib
```

Expected: all PASS; the final command proves every touched sidecar path compiles.

- [x] **Step 7: Commit Task 2**

```bash
git add crates/axon-services/src/source.rs \
  crates/axon-services/src/source/execution.rs \
  crates/axon-services/src/source/events.rs \
  crates/axon-services/src/source/events_tests.rs \
  crates/axon-services/src/source/executor.rs \
  crates/axon-services/src/source/executor/progress.rs \
  crates/axon-services/src/source/executor/progress_tests.rs
git commit -m "feat(progress): stream source lifecycle snapshots"
```

---

### Task 3: Build the pure wait view model and warning reducer

**Files:**
- Create: `crates/axon-cli/src/commands/wait_progress.rs`
- Create: `crates/axon-cli/src/commands/wait_progress/model.rs`
- Create: `crates/axon-cli/src/commands/wait_progress/model_tests.rs`
- Modify: `crates/axon-cli/src/commands.rs`

**Interfaces:**
- Consumes: `ForegroundSnapshot`, `JobStatusUpdate`, `SourceProgressEvent`, `PipelinePhase`, and `SourceKind`.
- Produces: `WaitViewModel::apply_snapshot`, `apply_event`, `finish`, `BatchWaitViewModel`, `ActiveProgress`, `RenderedMilestone`, and grouped `OperatorNotice` values for Tasks 4-7.

- [x] **Step 1: Declare the module and write failing reducer tests**

Add `mod wait_progress;` to `commands.rs`. Create tests covering phase aliases, neutral redaction copy, aggregation, and fast-phase suppression:

```rust
use super::*;
use axon_api::source::*;

fn redaction_event(event_id: &str, chunk_id: &str) -> SourceProgressEvent {
    let job_id = JobId::new(uuid::Uuid::from_u128(7));
    let mut event = SourceProgressEvent::minimal(
        job_id,
        0,
        PipelinePhase::Preparing,
        LifecycleStatus::CompletedDegraded,
        Severity::Degraded,
        "secret-redaction-forbidden payload value",
    );
    event.event_id = event_id.to_string();
    event.current = Some(ProgressCurrent {
        source_item_key: None,
        document_id: None,
        chunk_id: Some(ChunkId::new(chunk_id)),
        adapter: Some("web".into()),
        provider: None,
        message: None,
    });
    event.warning = Some(SourceWarning {
        code: "secret_redaction_forbidden".into(),
        severity: Severity::Degraded,
        message: "secret-redaction-forbidden payload value".into(),
        source_item_key: None,
        retryable: false,
    });
    event
}

fn embedding_update(done: u64, total: u64) -> JobStatusUpdate {
    JobStatusUpdate {
        job_id: JobId::new(uuid::Uuid::from_u128(7)),
        source_id: None,
        status: LifecycleStatus::Running,
        phase: PipelinePhase::Embedding,
        stage_id: None,
        counts: Some(StageCounts {
            chunks_total: Some(total),
            chunks_done: done,
            ..Default::default()
        }),
        current: None,
        message: Some("embedding chunks".into()),
        error: None,
    }
}

#[test]
fn embedding_family_collapses_to_one_operator_phase() {
    assert_eq!(operator_phase(PipelinePhase::Batching), OperatorPhase::Embed);
    assert_eq!(operator_phase(PipelinePhase::Embedding), OperatorPhase::Embed);
    assert_eq!(operator_phase(PipelinePhase::Vectorizing), OperatorPhase::Embed);
    assert_eq!(operator_phase(PipelinePhase::Upserting), OperatorPhase::Publish);
}

#[test]
fn repeated_redaction_holds_become_one_neutral_notice() {
    let mut model = WaitViewModel::source("https://gofastmcp.com", Some(SourceScope::Site));
    model.apply_event(redaction_event("evt_1", "chunk_1"));
    model.apply_event(redaction_event("evt_2", "chunk_2"));
    assert_eq!(model.notices.len(), 1);
    assert_eq!(model.notices[0].count, 2);
    assert_eq!(model.notices[0].message, "secret policy held 2 chunks");
    assert!(!model.notices[0].message.contains("chunk_1"));
}

#[test]
fn identical_snapshot_does_not_mark_the_model_dirty_twice() {
    let mut model = WaitViewModel::source("file:///repo", Some(SourceScope::Site));
    let update = embedding_update(3, 10);
    assert!(model.apply_snapshot(update.clone()));
    assert!(!model.apply_snapshot(update));
}

#[test]
fn subsecond_unremarkable_phase_does_not_leave_a_milestone() {
    let mut model = WaitViewModel::source("https://example.com", Some(SourceScope::Page));
    model.start_phase_at(PipelinePhase::Resolving, Duration::ZERO);
    model.complete_phase_at(PipelinePhase::Resolving, Duration::from_millis(200));
    assert!(model.milestones.is_empty());
}
```

- [x] **Step 2: Run the focused test and observe missing model types**

Run: `cargo test -p axon-cli commands::wait_progress::model -- --nocapture`

Expected: FAIL because the wait progress model does not exist.

- [x] **Step 3: Implement the focused presentation model**

Create these model types; keep terminal strings out of service DTOs:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperatorPhase { Resolve, Discover, Acquire, Prepare, Embed, Publish, Clean, Complete }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveProgress {
    pub phase: OperatorPhase,
    pub done: u64,
    pub total: Option<u64>,
    pub unit: &'static str,
    pub current: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedMilestone {
    pub phase: OperatorPhase,
    pub summary: String,
    pub elapsed: Duration,
    pub degraded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NoticeKey {
    pub phase: OperatorPhase,
    pub code: String,
    pub category: NoticeCategory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperatorNotice {
    pub key: NoticeKey,
    pub message: String,
    pub count: u64,
    pub retryable: bool,
}

pub struct WaitViewModel {
    pub target: String,
    pub scope: Option<SourceScope>,
    pub job_id: Option<JobId>,
    pub milestones: Vec<RenderedMilestone>,
    pub active: Option<ActiveProgress>,
    pub notices: Vec<OperatorNotice>,
    seen_event_ids: HashSet<String>,
    phase_started_at: Option<(OperatorPhase, Duration)>,
}
```

Implement `operator_phase`, source-specific unit selection (reuse/refactor the mapping in `job_progress.rs` rather than duplicating wording), `apply_snapshot -> bool`, `apply_event -> bool`, `apply_persisted_event(JobEvent)`, stable `event_id` deduplication, and the one-second milestone rule. Redaction codes containing `redact`, `secret`, or `forbidden` map to `NoticeCategory::PolicyHeld` and safe copy generated only from counts.

Add the batch presentation API explicitly:

```rust
pub struct BatchWaitViewModel {
    total: usize,
    completed: usize,
    failed: usize,
    active_targets: HashMap<usize, BatchTarget>,
}

pub struct BatchTarget {
    pub target: String,
    pub progress: Option<ActiveProgress>,
    updated_at: u64,
}

impl BatchWaitViewModel {
    pub fn new(total: usize) -> Self;
    pub fn running(&mut self, index: usize, target: impl Into<String>);
    pub fn apply_snapshot(&mut self, index: usize, update: JobStatusUpdate);
    pub fn apply_event(&mut self, index: usize, event: SourceProgressEvent);
    pub fn completed(&mut self, index: usize);
    pub fn failed(&mut self, index: usize);
    pub fn summary(&self) -> String;
    pub fn active_detail(&self) -> Option<&BatchTarget>;
}
```

Its active summary is `N/M complete · A active · Q queued`, and
`active_detail` chooses the most recently updated target. It must never create
one progress bar per input.

- [x] **Step 4: Run reducer and existing job-progress tests**

Run:

```bash
cargo test -p axon-cli commands::wait_progress::model -- --nocapture
cargo test -p axon-cli commands::job_progress -- --nocapture
```

Expected: PASS; existing source/extract unit wording remains unchanged.

- [x] **Step 5: Commit Task 3**

```bash
git add crates/axon-cli/src/commands.rs \
  crates/axon-cli/src/commands/wait_progress.rs \
  crates/axon-cli/src/commands/wait_progress/model.rs \
  crates/axon-cli/src/commands/wait_progress/model_tests.rs \
  crates/axon-cli/src/commands/job_progress.rs \
  crates/axon-cli/src/commands/job_progress_tests.rs
git commit -m "feat(cli): model quiet foreground progress"
```

---

### Task 4: Add rolling timing and responsive Aurora formatting

**Files:**
- Create: `crates/axon-cli/src/commands/wait_progress/timing.rs`
- Create: `crates/axon-cli/src/commands/wait_progress/timing_tests.rs`
- Create: `crates/axon-cli/src/commands/wait_progress/format.rs`
- Create: `crates/axon-cli/src/commands/wait_progress/format_tests.rs`
- Modify: `crates/axon-core/src/ui.rs`
- Modify: `crates/axon-core/src/ui_color_tests.rs`

**Interfaces:**
- Consumes: `WaitViewModel`, `ActiveProgress`, and Aurora color state.
- Produces: `RateEstimate`, `TimingEstimator::sample`, `format_wait_view(model, width, timing, color) -> FormattedWaitView`, terminal sanitization/truncation, and shared `ui::info` / `ui::neutral` helpers.

- [x] **Step 1: Write failing timing and formatting tests**

```rust
fn representative_embedding_view() -> WaitViewModel {
    let mut view = WaitViewModel::source("https://gofastmcp.com", Some(SourceScope::Site));
    view.apply_snapshot(JobStatusUpdate {
        job_id: JobId::new(uuid::Uuid::from_u128(9)),
        source_id: None,
        status: LifecycleStatus::Running,
        phase: PipelinePhase::Embedding,
        stage_id: None,
        counts: Some(StageCounts {
            chunks_total: Some(1936),
            chunks_done: 1442,
            ..Default::default()
        }),
        current: Some(ProgressCurrent {
            source_item_key: Some(SourceItemKey::new("authentication/index.html")),
            document_id: None,
            chunk_id: None,
            adapter: Some("web".into()),
            provider: None,
            message: None,
        }),
        message: Some("embedding chunks".into()),
        error: None,
    });
    view
}

fn stable_timing() -> RateEstimate {
    RateEstimate {
        per_second: 210.0,
        remaining: Duration::from_secs(2),
    }
}

#[test]
fn eta_waits_for_two_samples_spanning_one_second() {
    let mut timing = TimingEstimator::default();
    assert_eq!(timing.sample(Duration::ZERO, 0, Some(100)), None);
    assert_eq!(timing.sample(Duration::from_millis(500), 20, Some(100)), None);
    let estimate = timing.sample(Duration::from_secs(1), 40, Some(100)).unwrap();
    assert_eq!(estimate.per_second.round() as u64, 40);
    assert_eq!(estimate.remaining, Duration::from_millis(1500));
}

#[test]
fn phase_or_denominator_change_resets_timing() {
    let mut timing = TimingEstimator::default();
    timing.sample(Duration::ZERO, 0, Some(10));
    timing.sample(Duration::from_secs(1), 5, Some(10));
    timing.reset();
    assert_eq!(timing.sample(Duration::from_secs(2), 1, Some(20)), None);
}

#[test]
fn narrow_layout_drops_current_then_eta_then_bar() {
    let view = representative_embedding_view();
    let wide = format_wait_view(&view, 100, Some(stable_timing()), false);
    assert!(wide.active.join("\n").contains("authentication/index.html"));
    assert!(wide.active.join("\n").contains("ETA"));
    assert!(wide.active.join("\n").contains('━'));

    let narrow = format_wait_view(&view, 42, Some(stable_timing()), false);
    assert!(!narrow.active.join("\n").contains("authentication/index.html"));
    assert!(!narrow.active.join("\n").contains("ETA"));
    assert!(!narrow.active.join("\n").contains('━'));
    assert!(narrow.active.join("\n").contains("1442/1936"));
}

#[test]
fn terminal_text_removes_controls_and_middle_truncates_paths() {
    assert_eq!(sanitize_terminal_text("ok\x1b[31m\n"), "ok[31m");
    let truncated = middle_truncate("authentication/reference/index.html", 20);
    assert_eq!(truncated.chars().count(), 20);
    assert!(truncated.starts_with("auth"));
    assert!(truncated.ends_with("index.html"));
}
```

- [x] **Step 2: Run focused tests and observe missing functions**

Run: `cargo test -p axon-cli commands::wait_progress -- --nocapture`

Expected: FAIL because timing/format modules do not exist.

- [x] **Step 3: Implement timing with a bounded rolling window**

Define `RateEstimate { per_second: f64, remaining: Duration }` and use
`VecDeque<(Duration, u64)>` capped at eight samples. Return `None` until two
distinct counts span at least one second. Suppress estimates when `total` is
absent/zero, count regresses, rate is non-positive, or remaining time is below
250 ms. Reset when phase or total changes. Format rates with at most one
decimal below 10/s and whole units otherwise.

- [x] **Step 4: Add missing Aurora product-token helpers**

In `axon-core::ui`, add:

```rust
const NEUTRAL_ANSI: &str = "\x1b[38;2;145;168;182m"; // #91A8B6

pub fn info(text: &str) -> String { ansi_colorize(INFO_ANSI, text) }
pub fn neutral(text: &str) -> String { ansi_colorize(NEUTRAL_ANSI, text) }

pub fn stderr_color_enabled() -> bool {
    use std::io::IsTerminal;
    color_enabled_for_auto_tty(std::io::stderr().is_terminal())
}
```

Test `--color=never`, forced color, and auto/no-TTY using the existing color test guard and internal `color_enabled_for_auto_tty` helper.

- [x] **Step 5: Implement plain-first responsive formatting**

`FormattedWaitView` contains `heading: String`, `milestones: Vec<String>`, `notices: Vec<String>`, `active: Vec<String>`, and optional `terminal: String`. Build plain visible text first, measure it without ANSI, then apply `primary/accent/info/success/warning/error/muted/subtle/neutral` spans. Use compact labels `discover`, `acquire`, `prepare`, `embed`, `publish`, `clean`, and `indexed`.

At widths below 60 remove current item; below 50 remove rate/ETA; below 46 remove the graphical bar. Always retain symbol, phase, best count, percentage when a real total exists, and elapsed time when it fits.

- [x] **Step 6: Run focused and UI tests**

Run:

```bash
cargo test -p axon-cli commands::wait_progress::timing -- --nocapture
cargo test -p axon-cli commands::wait_progress::format -- --nocapture
cargo test -p axon-core ui -- --nocapture
```

Expected: PASS with no ANSI in color-disabled snapshots.

- [x] **Step 7: Commit Task 4**

```bash
git add crates/axon-core/src/ui.rs crates/axon-core/src/ui_color_tests.rs \
  crates/axon-cli/src/commands/wait_progress/timing.rs \
  crates/axon-cli/src/commands/wait_progress/timing_tests.rs \
  crates/axon-cli/src/commands/wait_progress/format.rs \
  crates/axon-cli/src/commands/wait_progress/format_tests.rs
git commit -m "feat(cli): format Aurora wait progress"
```

---

### Task 5: Render one stderr live region and reconcile overflow

**Files:**
- Create: `crates/axon-cli/src/commands/wait_progress/render.rs`
- Create: `crates/axon-cli/src/commands/wait_progress/render_tests.rs`
- Modify: `crates/axon-cli/src/commands/wait_progress.rs`
- Modify: `crates/axon-cli/Cargo.toml`

**Interfaces:**
- Consumes: `ForegroundProgressReceiver`, `FormattedWaitView`, `JobStore`, and `WaitViewModel`.
- Produces: `WaitProgressSession::source`, `WaitProgressSession::run_until`, `WaitProgressSession::finish`, `BatchProgressUpdate`, `BatchProgressForwarder::run_until`, `ProgressMode::{Interactive, Plain, Silent}`, and overflow reconciliation used by source/session commands.

- [x] **Step 1: Enable the in-memory terminal only for tests and write failing renderer tests**

Add this line to the existing `[dev-dependencies]` section (features merge only
for test builds):

```toml
indicatif = { version = "0.18", features = ["in_memory"] }
```

Write tests using `indicatif::InMemoryTerm`:

```rust
fn active_formatted_view() -> FormattedWaitView {
    FormattedWaitView {
        heading: "axon  source".into(),
        milestones: Vec::new(),
        notices: Vec::new(),
        active: vec!["◐ embed  1442/1936 chunks · embedding chunks".into()],
        terminal: None,
    }
}

fn completed_formatted_view() -> FormattedWaitView {
    FormattedWaitView {
        heading: "axon  source".into(),
        milestones: vec!["✓ indexed  1936 chunks".into()],
        notices: Vec::new(),
        active: Vec::new(),
        terminal: Some("✓ indexed  1936 chunks".into()),
    }
}

#[test]
fn json_and_quiet_modes_are_silent() {
    let mut cfg = Config::default();
    cfg.json_output = true;
    assert_eq!(ProgressMode::for_config(&cfg, true), ProgressMode::Silent);
    cfg.json_output = false;
    cfg.quiet = true;
    assert_eq!(ProgressMode::for_config(&cfg, true), ProgressMode::Silent);
}

#[test]
fn redirected_stderr_uses_plain_important_events_only() {
    let cfg = Config::default();
    assert_eq!(ProgressMode::for_config(&cfg, false), ProgressMode::Plain);
}

#[tokio::test]
async fn interactive_finish_clears_the_live_region_once() {
    let term = InMemoryTerm::new(20, 100);
    let mut renderer = WaitRenderer::for_test(term.clone(), ProgressMode::Interactive);
    renderer.render(&active_formatted_view()).unwrap();
    renderer.finish(&completed_formatted_view()).unwrap();
    renderer.finish(&completed_formatted_view()).unwrap();
    let contents = term.contents();
    assert_eq!(contents.matches("indexed").count(), 1);
    assert!(!contents.contains("embedding chunks"));
}
```

- [x] **Step 2: Run the focused test and observe missing renderer/session types**

Run: `cargo test -p axon-cli commands::wait_progress::render -- --nocapture`

Expected: FAIL because renderer types do not exist.

- [x] **Step 3: Implement mode selection and one `MultiProgress` region**

`ProgressMode::for_config(cfg, stderr_is_tty)` returns `Silent` for JSON/quiet, `Interactive` only for TTY stderr, and `Plain` otherwise. `--color=always` changes colors but never upgrades redirected output to animation.

Use one `MultiProgress` with a header bar, zero-or-more permanent `println` calls, and one active `ProgressBar`. Maintain the last formatted model and last render `Instant`; skip identical frames and throttle ordinary frames to 250 ms. Warning/retry/failure/terminal updates call `render_now` immediately. Make `finish` idempotent and implement `Drop` to `finish_and_clear` the active bar.

- [x] **Step 4: Implement durable event reconciliation**

`WaitProgressSession` owns `seen_event_ids: HashSet<String>`, `last_durable_sequence: Option<u64>`, and `next_cursor: Option<String>`. When `receiver.take_overflowed()` is true:

```rust
let request = JobEventListRequest {
    job_id,
    after_sequence: last_durable_sequence,
    limit: Some(200),
    severity: None,
    visibility: Some(Visibility::Public),
    phase: None,
    since_sequence: None,
    cursor: next_cursor.take(),
};
```

Page until `next_cursor` is `None`, apply only unseen `event_id` values, and set `last_durable_sequence = Some(page.last_sequence)`. On the first catch-up `after_sequence` is `None`, so stable event-id dedup prevents duplicate notices. If reconciliation fails, call `receiver.mark_overflowed()`, emit one muted diagnostic, and retry on the next cadence tick; do not fail the source operation.

- [x] **Step 5: Drive snapshot/event channels concurrently**

Implement the source driver as a generic method that preserves the command
future's exact output:

```rust
pub async fn run_until<T>(&mut self, work: impl Future<Output = T>) -> T {
    tokio::pin!(work);
    let mut cadence = tokio::time::interval(Duration::from_millis(250));
    let mut snapshots_open = true;
    let mut events_open = true;
    loop {
        tokio::select! {
            result = &mut work => {
                self.drain_ready_updates().await;
                self.finish();
                return result;
            }
            changed = self.receiver.snapshots.changed(), if snapshots_open => {
                if changed.is_ok() {
                    self.apply_latest_snapshot();
                } else {
                    snapshots_open = false;
                }
            }
            event = self.receiver.events.recv(), if events_open => {
                if let Some(event) = event {
                    self.apply_event(event);
                } else {
                    events_open = false;
                }
            }
            _ = cadence.tick() => {
                self.reconcile_if_overflowed().await;
                self.render_if_dirty();
            }
        }
    }
}
```

Disable a snapshot/event branch after its channel closes so `select!` cannot
spin. `drain_ready_updates` consumes immediately available facts before the
terminal render. The command retains stdout ownership.

For waited multi-source input, define the tagged hub input:

```rust
pub enum BatchProgressUpdate {
    Started { index: usize, target: String },
    Snapshot { index: usize, update: JobStatusUpdate },
    Event { index: usize, event: SourceProgressEvent },
    Finished { index: usize, failed: bool },
}
```

`BatchProgressForwarder::run_until(index, target, receiver, work)` performs the
same select loop without drawing and sends tagged updates to the one
command-owned batch session. Only the batch session owns `MultiProgress`.

- [x] **Step 6: Run renderer tests**

Run: `cargo test -p axon-cli commands::wait_progress::render -- --nocapture`

Expected: PASS for mode selection, no control animation in plain mode, overflow reconciliation, warning-above-live behavior, and idempotent cleanup.

- [x] **Step 7: Commit Task 5**

```bash
git add crates/axon-cli/Cargo.toml \
  crates/axon-cli/src/commands/wait_progress.rs \
  crates/axon-cli/src/commands/wait_progress/render.rs \
  crates/axon-cli/src/commands/wait_progress/render_tests.rs
git commit -m "feat(cli): render interactive wait progress"
```

---

### Task 6: Wire source, scrape, sessions, and waited batches

**Files:**
- Modify: `crates/axon-cli/src/commands/source.rs:26-187`
- Modify: `crates/axon-cli/src/commands/source_tests.rs`
- Modify: `crates/axon-cli/src/commands/sessions.rs:20-93`
- Create: `crates/axon-cli/src/commands/sessions_tests.rs`
- Modify: `crates/axon-cli/src/commands/wait_progress.rs`

**Interfaces:**
- Consumes: `index_source_with_progress`, `foreground_progress_channel`, and `WaitProgressSession`.
- Produces: `execute_waited_source_request`, one renderer per single/session source, and one aggregate renderer for multi-input waited source runs.

- [x] **Step 1: Write failing command policy tests**

```rust
#[test]
fn waited_source_uses_progress_but_json_and_quiet_do_not() {
    let mut cfg = Config::default();
    cfg.command = CommandKind::Source;
    cfg.wait = true;
    assert_eq!(ProgressMode::for_config(&cfg, true), ProgressMode::Interactive);
    cfg.json_output = true;
    assert_eq!(ProgressMode::for_config(&cfg, true), ProgressMode::Silent);
    cfg.json_output = false;
    cfg.quiet = true;
    assert_eq!(ProgressMode::for_config(&cfg, true), ProgressMode::Silent);
}

#[test]
fn scrape_projection_is_foreground_progress_capable() {
    let mut cfg = Config::default();
    cfg.command = CommandKind::Scrape;
    assert!(!should_detach(&cfg));
}

#[test]
fn waited_batch_uses_one_aggregate_view() {
    let mut batch = BatchWaitViewModel::new(3);
    batch.running(0, "a");
    batch.running(1, "b");
    batch.completed(0);
    assert_eq!(batch.summary(), "1/3 complete · 1 active · 1 queued");
    assert_eq!(batch.active_detail().map(|target| target.target.as_str()), Some("b"));
}
```

- [x] **Step 2: Run source/session command tests and observe missing wait path**

Run:

```bash
cargo test -p axon-cli waited_source -- --nocapture
cargo test -p axon-cli sessions -- --nocapture
```

Expected: FAIL because foreground execution still calls `index_source` directly.

- [x] **Step 3: Implement one source wait helper**

Add:

```rust
async fn execute_waited_source_request(
    cfg: &Config,
    service_context: &ServiceContext,
    request: SourceRequest,
) -> Result<SourceResult, Box<dyn Error>> {
    let (sender, receiver) = axon_services::source::foreground_progress::foreground_progress_channel();
    let mut session = WaitProgressSession::source(
        cfg,
        &request.source,
        request.scope,
        receiver,
        service_context.job_store(),
    );
    let work = axon_services::index_source_with_progress(request, service_context, sender);
    let result = session.run_until(work).await;
    result.map_err(|error| error.into())
}
```

Keep `execute_source_request`'s detached branch unchanged and route only
foreground work through this helper.

- [x] **Step 4: Preserve stdout and scrape behavior**

Keep `render_source_result` and `write_scrape_output_if_requested` after the wait session has cleared stderr. When `ProgressMode::Silent`, call the existing `index_source` path without creating a foreground channel. Interactive and plain modes call `index_source_with_progress` and drain the receiver through `WaitProgressSession`.

- [x] **Step 5: Integrate sessions sequentially**

Declare `#[cfg(test)] #[path = "sessions_tests.rs"] mod tests;` at the end of
`sessions.rs`. Replace the `cfg.wait` direct `index_source` branch with
`execute_waited_source_request`. Prefix the heading with `session N/M` and
provider name. Preserve the existing detached enqueue/auto-worker behavior and
the aggregate JSON envelope exactly.

- [x] **Step 6: Integrate concurrent waited source batches without multiple bars**

Create one `BatchWaitViewModel` and one renderer before `buffer_unordered`. Each input forwards its progress facts tagged by the existing input index into the shared batch model. Display one overall active block:

```text
◐ source       1/3 complete · 1 active · 1 queued
               embed gofastmcp.com · 74.5%
```

Do not create an `indicatif::ProgressBar` per input. Preserve `batch_concurrency`, result ordering, semantic failure counting, and final stdout rendering.

- [x] **Step 7: Run command and compile tests**

Run:

```bash
cargo test -p axon-cli waited_source -- --nocapture
cargo test -p axon-cli sessions -- --nocapture
cargo test -p axon-cli scrape_map_source_projection -- --nocapture
cargo test -p axon-cli --no-run --lib
```

Expected: PASS; sidecars compile and JSON/quiet tests show no progress output path.

- [x] **Step 8: Commit Task 6**

```bash
git add crates/axon-cli/src/commands/source.rs \
  crates/axon-cli/src/commands/source_tests.rs \
  crates/axon-cli/src/commands/sessions.rs \
  crates/axon-cli/src/commands/sessions_tests.rs \
  crates/axon-cli/src/commands/wait_progress.rs
git commit -m "feat(cli): show source wait progress"
```

---

### Task 7: Add genuine coarse extract progress

**Files:**
- Modify: `crates/axon-services/src/extract/sync.rs:32-126`
- Modify: `crates/axon-services/src/extract/sync_tests.rs`
- Modify: `crates/axon-services/src/extract.rs`
- Modify: `crates/axon-cli/src/commands/extract.rs:52-102`
- Modify: `crates/axon-cli/src/commands/extract_tests.rs`
- Modify: `crates/axon-cli/src/commands/wait_progress/model.rs`

**Interfaces:**
- Consumes: shared wait model/renderer from Tasks 3-5.
- Produces: `ExtractProgress`, `extract_sync_with_progress`, and `WaitViewModel::apply_extract_progress`.

- [x] **Step 1: Write failing extract snapshot tests**

Add a pure aggregation helper and test it without network access:

```rust
#[test]
fn completed_extract_run_advances_urls_and_items() {
    let prior = ExtractProgress::new(3);
    let next = prior.completed_url("https://example.com/a", 7);
    assert_eq!(next.urls_total, 3);
    assert_eq!(next.urls_done, 1);
    assert_eq!(next.items_done, 7);
    assert_eq!(next.last_completed_url.as_deref(), Some("https://example.com/a"));
}

#[test]
fn extract_progress_is_monotonic_across_out_of_order_completion() {
    let progress = ExtractProgress::new(2)
        .completed_url("https://example.com/b", 3)
        .completed_url("https://example.com/a", 1);
    assert_eq!(progress.urls_done, 2);
    assert_eq!(progress.items_done, 4);
}
```

Add CLI policy assertion:

```rust
#[test]
fn extract_wait_reuses_shared_renderer_and_json_stays_silent() {
    let mut cfg = Config::default();
    cfg.command = CommandKind::Extract;
    cfg.wait = true;
    assert_eq!(extract_progress_mode(&cfg, true), ProgressMode::Interactive);
    cfg.json_output = true;
    assert_eq!(extract_progress_mode(&cfg, true), ProgressMode::Silent);
}
```

- [x] **Step 2: Run focused tests and observe missing extract progress API**

Run:

```bash
cargo test -p axon-services extract::sync -- --nocapture
cargo test -p axon-cli extract_wait -- --nocapture
```

Expected: FAIL because `ExtractProgress` and `extract_sync_with_progress` do not exist.

- [x] **Step 3: Implement coarse progress without fake current-work claims**

Add:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractProgress {
    pub urls_total: u64,
    pub urls_done: u64,
    pub items_done: u64,
    pub last_completed_url: Option<String>,
}

impl ExtractProgress {
    pub fn new(urls_total: usize) -> Self {
        Self {
            urls_total: urls_total as u64,
            urls_done: 0,
            items_done: 0,
            last_completed_url: None,
        }
    }
    pub fn completed_url(mut self, url: impl Into<String>, items: usize) -> Self {
        self.urls_done = self.urls_done.saturating_add(1).min(self.urls_total);
        self.items_done = self.items_done.saturating_add(items as u64);
        self.last_completed_url = Some(url.into());
        self
    }
}
```

Use a Tokio `watch::Sender<ExtractProgress>` because extract snapshots are coalescible and there is no separate warning stream in this synchronous boundary. `extract_sync` remains a compatibility wrapper calling `extract_sync_with_progress(..., None)`. In `execute_extract_runs`, update/send progress after each completed `ExtractRun`; label the URL as `last completed`, never as the sole active URL because up to 16 runs are concurrent.

In `crates/axon-services/src/extract.rs`, export the new surface explicitly:

```rust
pub use sync::{ExtractProgress, extract_sync, extract_sync_with_progress};
```

- [x] **Step 4: Replace the generic spinner in the CLI**

Delete the `wait_spinner_for` use from `extract.rs`. Start one shared wait session, map `ExtractProgress` into active unit `URL/URLs` plus cumulative items, run `extract_sync_with_progress`, clear the live region, then call the existing `emit_extract_output`. Keep the zero-item error and JSON output unchanged.

- [x] **Step 5: Run focused and sidecar compile tests**

Run:

```bash
cargo test -p axon-services extract::sync -- --nocapture
cargo test -p axon-cli extract_wait -- --nocapture
cargo test -p axon-services --no-run --lib
cargo test -p axon-cli --no-run --lib
```

Expected: PASS; no network is required by the new pure progress tests.

- [x] **Step 6: Commit Task 7**

```bash
git add crates/axon-services/src/extract.rs \
  crates/axon-services/src/extract/sync.rs \
  crates/axon-services/src/extract/sync_tests.rs \
  crates/axon-cli/src/commands/extract.rs \
  crates/axon-cli/src/commands/extract_tests.rs \
  crates/axon-cli/src/commands/wait_progress/model.rs
git commit -m "feat(cli): show extract wait progress"
```

---

### Task 8: Verify contracts, manual UX, and integration boundaries

**Files:**
- Update: `docs/superpowers/plans/2026-08-11-interactive-wait-progress.md` checkboxes during execution

**Interfaces:**
- Consumes: complete implementation from Tasks 1-7.
- Produces: verified interactive/plain/JSON/quiet behavior and an evidence-backed closeout for bead `axon_rust-kobuh`.

- [x] **Step 1: Run formatting and structural checks**

Run:

```bash
cargo fmt --all -- --check
git diff --check
./target/debug/xtask check-layering
```

If `./target/debug/xtask` is not built, run `cargo build -p xtask` once and repeat the direct command.

Expected: all PASS; no transport/domain layering or whitespace drift.

- [x] **Step 2: Run targeted crate gates**

Run:

```bash
cargo test -p axon-services source::foreground_progress -- --nocapture
cargo test -p axon-services source::events::tests -- --nocapture
cargo test -p axon-services source::executor::progress::tests -- --nocapture
cargo test -p axon-services extract::sync -- --nocapture
cargo test -p axon-cli commands::wait_progress -- --nocapture
cargo test -p axon-cli waited_source -- --nocapture
cargo test -p axon-cli extract_wait -- --nocapture
cargo test -p axon-services --no-run --lib
cargo test -p axon-cli --no-run --lib
```

Expected: all PASS. The two `--no-run` commands prove sidecar paths compile.

- [x] **Step 3: Run targeted clippy**

Run:

```bash
cargo clippy -p axon-services -p axon-cli -p axon-core --all-targets --locked -- -D warnings
```

Expected: PASS with no warnings.

- [x] **Step 4: Build the local debug binary**

Run: `cargo build --bin axon`

Expected: PASS and update `target/debug/axon`. Do not stage any tracked plugin binary or unrelated file if the build changes one.

- [x] **Step 5: Verify non-TTY and JSON contracts without live providers**

Use a source that fails during validation before contacting Qdrant/TEI:

```bash
./target/debug/axon source '' --wait true --local 2> /tmp/axon-wait.stderr > /tmp/axon-wait.stdout || true
test ! -s /tmp/axon-wait.stderr || ! rg -n $'\x1b\\[[0-9;?]*[A-Za-z]' /tmp/axon-wait.stderr

./target/debug/axon source '' --wait true --local --json \
  2> /tmp/axon-wait-json.stderr > /tmp/axon-wait-json.stdout || true
jq -e . /tmp/axon-wait-json.stdout
```

Expected: redirected stderr contains no cursor-control animation; JSON stdout parses as one JSON value. If empty input is rejected by clap before JSON rendering, use an unsupported but non-network source and record the exact boundary instead of weakening the assertion.

- [x] **Step 6: Verify one live TTY source run against configured providers**

Run from a real terminal:

```bash
./target/debug/axon https://example.com --scope page --wait true --local
```

Expected evidence:

- one compact Aurora heading on stderr;
- at most one active live block;
- no per-item transcript;
- a real denominator produces a cyan bar/percentage;
- completed major phases and warnings remain readable;
- final `Source Indexed` result appears after the live block clears;
- no malformed cursor sequence remains at the prompt.

If Qdrant, TEI, or Chrome is unavailable, record the exact provider boundary and run the renderer's in-memory integration test as the safe end-to-end verification. Do not claim live success without the provider-backed run.

- [x] **Step 7: Verify color and quiet modes**

Run:

```bash
NO_COLOR=1 ./target/debug/axon https://example.com --scope page --wait true --local
./target/debug/axon https://example.com --scope page --wait true --local --quiet
```

Expected: first run keeps layout without ANSI; second prints no progress renderer.

- [x] **Step 8: Inspect changed-file and commit isolation**

Run:

```bash
git status --short
git diff --name-only HEAD~7..HEAD
git diff --check HEAD~7..HEAD
```

Expected: implementation commits contain only progress-owned files. Pre-existing redaction changes remain unstaged/uncommitted unless the user explicitly authorizes an integration commit.

- [x] **Step 9: Record verification and close the bead only after all required gates pass**

```bash
bd update axon_rust-kobuh --notes="Implemented interactive wait progress. Verified formatting, layering, targeted axon-services/axon-cli tests, targeted clippy, stdout/stderr mode contracts, and the local https://example.com page smoke; remaining provider warnings are recorded in the implementation closeout."
bd close axon_rust-kobuh --reason="Approved spec and TDD plan implemented; targeted tests, clippy, stream contracts, and live/local smoke verified."
```

Do not close the bead if implementation remains unstarted, any required test fails, or the live provider-backed smoke is blocked without an explicitly accepted substitute.

---

## Commit Sequence

1. `feat(progress): add bounded foreground source feed`
2. `feat(progress): stream source lifecycle snapshots`
3. `feat(cli): model quiet foreground progress`
4. `feat(cli): format Aurora wait progress`
5. `feat(cli): render interactive wait progress`
6. `feat(cli): show source wait progress`
7. `feat(cli): show extract wait progress`

Each commit must be independently testable with the focused command listed in its task. Never use `git add -A` in this dirty shared worktree.
