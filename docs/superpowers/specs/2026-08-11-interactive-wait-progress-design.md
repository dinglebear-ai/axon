# Interactive Wait Progress — Design Spec

Date: 2026-08-11
Status: Approved; implementation plan pending
Branch: `codex/secret-redaction-investigation`
Bead: `axon_rust-kobuh`

## Summary

Replace Axon's opaque foreground waiting experience with a quiet, Aurora-themed
hybrid progress display. While a foreground operation is active, one live block
shows the current phase, useful counters, elapsed time, current item, and a
progress bar when a real denominator exists. Completed major phases, warnings,
retries, degraded outcomes, and failures may leave compact permanent lines.

The service layer continues to own structured progress facts. The CLI owns all
presentation, including wording, color, layout, throttling, aggregation, and
TTY behavior. Progress is written to stderr; the final human or JSON result
keeps its existing stdout contract.

The first implementation covers every current foreground source projection:

- bare source and `source --wait true`
- `sessions --wait true`
- retained `scrape` page projection
- any other CLI path already calling the unified source pipeline synchronously
- `extract --wait true`, using the same renderer with its coarser URL/item facts

Existing specialized views such as `status --watch` keep their command-specific
orchestration, but should reuse the same Aurora formatting primitives and
progress-model helpers where practical.

## Problem

`source --wait` currently executes the unified pipeline inline and renders only
the final `SourceResult`. The pipeline already persists phase-aware
`SourceProgressEvent` values and monotonic `JobStatusUpdate` snapshots, while
the CLI already contains phase-aware source/extract progress summaries and an
`indicatif` live status view. The missing piece is a foreground bridge and one
operator-focused rendering policy.

The current behavior creates three operational problems:

1. Long acquisition, preparation, embedding, or publication phases look hung.
2. Important warnings are mixed with repetitive logging or appear only at the
   end, after the operator could have acted on them.
3. Machine output, redirected output, and interactive output do not share an
   explicit noise and stream-separation contract.

## Goals

- Make a long `--wait` run visibly alive without printing per-item spam.
- Show the active phase, meaningful units, progress, elapsed time, and current
  item using the data Axon already owns.
- Preserve only operationally significant history in the terminal transcript.
- Group repeated warnings, especially redaction-policy holds, without exposing
  payload values or incorrectly asserting that a secret was confirmed.
- Use Axon's Aurora **product CLI** palette, not the separate Claude Code theme.
- Keep `--json`, `--quiet`, redirected stderr, `NO_COLOR`, and
  `--color=never` deterministic and script-safe.
- Keep progress facts transport-neutral and presentation-free outside the CLI.
- Make the renderer reusable across source projections and coarser extract jobs.

## Non-Goals

- No TUI, alternate screen, mouse interaction, or terminal dashboard.
- No per-page, per-file, per-document, or per-chunk transcript.
- No new progress flags or user-configurable themes in v1.
- No estimated overall pipeline percentage. Pipeline phases have different and
  source-dependent costs, so an overall percentage would imply false precision.
- No presentation strings, ANSI codes, terminal widths, or throttling policy in
  `axon-api`, `axon-services`, `axon-jobs`, or `axon-observe`.
- No change to MCP/REST progress schemas.
- No promise that interrupting an inline foreground execution keeps computing
  after the CLI process exits. The durable job remains inspectable/recoverable,
  but "job continues" is shown only when execution is actually worker-backed.

## Locked Decisions

| Question | Decision |
|---|---|
| Interaction model | Hybrid: one live block plus a small permanent transcript. |
| Presentation owner | `axon-cli`; service and API crates expose structured facts only. |
| Live data | Direct foreground progress feed carrying the same structured event/snapshot DTOs persisted by the job runtime. |
| Durable source of truth | Existing job row, events, and terminal result remain authoritative; the foreground feed is an observation path, not a second lifecycle. |
| Render cadence | At most four visible refreshes per second; newer snapshots coalesce. |
| Transcript policy | Major completed phases only when useful; always retain warnings, retries, degraded outcomes, and failures. |
| Warning policy | Aggregate by stable code/category and phase; never print payload values. |
| Redaction wording | Neutral: "secret policy held N chunks", not "found N secrets" or "suspected secrets". |
| Progress bar | Render only when `total > 0`; otherwise use a spinner and counts without invented completion. |
| ETA | Show only after sufficient stable samples; suppress unstable or negative estimates. |
| Streams | Interactive progress to stderr; final result to stdout. |
| Color source | Axon/Aurora product CLI tokens in `axon-core::ui`. |
| Interrupt copy | State only behavior the current execution mode guarantees. |

## Operator Experience

### Active source run

The compact heading is printed once. Permanent phase lines appear above one
live block. The live block redraws in place.

```text
  axon source  gofastmcp.com · site                 job 01b4…

✓ discover     126 pages                               1.2s
✓ acquire      124/126 pages · 2 unavailable           8.7s
✓ prepare      418 documents · 1,936 chunks            2.4s
⚠ policy       held 3 chunks · redaction review available

◐ embed        ━━━━━━━━━━━━━━━━━━━╺━━━━━━  74.5%
               1,442/1,936 chunks · 210/s · ETA 2s
               authentication/index.html
```

The renderer does not reserve three live rows when the terminal is too short;
it collapses in order: current item, rate/ETA, then the bar. Phase and counts
remain visible as long as the terminal can display one line.

### Completion

The live block is cleared and replaced by one terminal phase line. The existing
command-specific final renderer follows on stdout.

```text
✓ indexed      418 documents · 1,933 vectors          25.4s
               3 held · 2 unavailable · generation 7
```

### Important retry

Retries are permanent because they explain latency and provider instability.
Equivalent retry messages update an aggregate rather than adding one line per
attempt.

```text
⚠ embedding    provider timeout · retry 2/3 in 800ms
```

### Actionable warning detail

The live view prints a grouped safe summary and a drill-down command. The
command references the job, never a raw payload.

```text
⚠ policy       held 3 chunks · redaction review available
               axon jobs events 01b4… --warnings
```

If the current CLI does not support a filtered `--warnings` view when this work
is implemented, the renderer prints the existing valid `jobs events` command
instead of inventing a flag. Adding that filter is optional follow-up scope.

## Aurora Product CLI Styling

The implementation reuses or extends `axon-core::ui`; it does not inline raw
ANSI values in command modules.

| Role | Token | Use |
|---|---|---|
| Identity | rose `#f9a8c4` | `axon` and compact heading identity |
| Active | cyan `#29b6f6` | spinner, current phase, percentage, filled bar |
| Secondary active | info `#72c8f5` | supporting active metadata when cyan would dominate |
| Success | teal `#7dd3c7` | checkmarks and terminal success symbol |
| Warning | amber `#c6a36b` | warning symbol, retries, held/degraded counts |
| Failure | rose-red `#c78490` | failure symbol and error label |
| Primary text | `#e6f4fb` | counts and main labels |
| Muted text | `#a7bcc9` | target, current item, elapsed time, help |
| Subtle | deep rose `#c46b88` | job ids and quiet separators |

Completed rows avoid coloring the whole line. The success checkmark is teal;
the phase and counts remain primary text. The active row uses cyan selectively
for hierarchy rather than turning every character cyan.

## Noise Policy

### Live updates

- Accept structured updates as often as the pipeline produces them.
- Coalesce superseded snapshots and render no faster than every 250 ms.
- Render immediately on phase change, first warning/error, retry, or terminal
  transition rather than waiting for the next cadence tick.
- Do not redraw when the formatted visible model is unchanged.
- Normalize and truncate current-item labels before comparison so meaningless
  path churn does not force excessive redraws.

### Permanent lines

A phase completion becomes permanent when at least one condition is true:

- it ran for at least one second;
- it produced a warning, retry, error, or degraded count;
- it established or materially changed a useful total;
- it is acquisition, preparation, embedding, publication, or terminal
  completion and the whole operation ran long enough to need a transcript.

Fast phases may stay live-only. Adjacent phases with the same operator meaning
may collapse into one label (for example batching/embedding/vectorizing as
`embed`) without changing persisted phase values.

### Warning aggregation

- Aggregate key: stable warning/error code, normalized phase, and safe category.
- Keep the first safe example identifier only when policy permits it; never keep
  or render payload values.
- Show one line on first occurrence, then update its count at phase/terminal
  boundaries rather than redrawing for every occurrence.
- Redaction-policy categories distinguish at least `held`, `redacted`, and
  `forbidden`, because they have different operational consequences.
- Terminal output reports counts and an inspection command, not one line per
  affected chunk.

## Rendering Model

`axon-cli` owns a presentation model separate from raw DTOs:

```text
WaitViewModel
  heading: operation + safe target + scope + optional job id
  completed: ordered RenderedMilestone[]
  active: phase + units + fraction + current item + timing estimate
  notices: aggregated warning/retry/failure groups
  terminal: status + final metrics + elapsed time
```

Raw `SourceProgressEvent` and `JobStatusUpdate` values are reduced into this
model before any terminal calls. Formatting functions consume a model and a
terminal width; they do not query services or mutate lifecycle state. This
separation makes snapshot tests deterministic and keeps deduplication testable
without a real terminal.

## Progress Delivery Architecture

### Foreground feed

Add an optional, transport-neutral foreground progress sink to source execution.
It receives two existing fact shapes:

1. Event facts: phase lifecycle, warning, retry, degraded, and failure events.
2. Snapshot facts: the latest monotonic counts/current-item update used to
   update the durable job row.

The same call sites that append events or persist `JobStatusUpdate` snapshots
fan out a clone to the optional foreground sink. The durable store remains the
source of truth. Sink failure, closure, or renderer exit must never fail or
slow the source pipeline.

The CLI implementation uses a short-lived, two-lane feed scoped to one
foreground command. The consumer starts before execution, drains concurrently,
and owns the renderer:

- A Tokio `watch` lane carries the latest job-start/snapshot state. Sending a
  newer snapshot replaces the older one, so high-frequency counts cannot build
  a queue.
- A bounded Tokio `mpsc` lane (initial capacity 256) carries ordered lifecycle,
  warning, retry, degraded, failure, and terminal events.
- Producers use non-blocking sends. A full event lane sets a shared overflow
  flag rather than slowing the pipeline. Because durable append remains the
  source of truth, the renderer responds to that flag by reconciling persisted
  events. Foreground clones retain the stable `event_id` but do not receive the
  SQLite-assigned sequence number, so the first reconciliation pages from the
  start and deduplicates by `event_id`; later reconciliations resume after the
  last durable sequence. The overflow flag clears only after catch-up succeeds.
- If durable append itself failed, the existing service warning remains visible
  in diagnostics; the progress feed still attempts its best-effort send.

This bounds foreground memory, coalesces disposable snapshots, preserves event
order in the normal path, and makes event loss detectable and recoverable under
backpressure rather than silently dropping important operator facts.

Early resolving/routing output may render from the known request before a job
id exists. Once the canonical durable job is created, a `job_started` fact adds
the abbreviated id to the heading. No second job identity is introduced.

### Why not poll SQLite for the interactive path

The job store remains the recovery and detached-monitor source, but continuous
foreground polling would add latency, queries, and duplicated sequence/bookmark
logic. A direct observation feed also exposes the in-memory current item and
counts at the cadence already chosen by the coordinator. `status --watch` and
later resume/attach workflows may continue polling durable state because they
do not own the execution.

### Extract integration

`extract --wait` currently has only a generic spinner around synchronous work.
It adopts the same CLI renderer and presentation model. Its service boundary
emits only the progress facts it genuinely knows: URL count, current URL,
completed URL count, extracted item count, warnings, and terminal status. It
must not synthesize source-pipeline phases or fake percentages.

## Stream and Terminal Contracts

| Mode | Behavior |
|---|---|
| Human + stderr TTY | Animated hybrid renderer on stderr; final command result on stdout. |
| `--json` | No progress renderer; stdout contains only the existing JSON result. Diagnostics remain stderr. |
| `--quiet` | No progress renderer or milestone transcript. Errors retain existing command semantics. |
| Redirected stderr | No animation, cursor control, or progress bar. Print only warnings, retries, failures, and one terminal outcome. |
| `--color=never` / `NO_COLOR` | Same layout and symbols where supported, without ANSI. |
| `--color=always` | Honor existing Axon color override even when the stream is not a TTY; still do not emit cursor-control animation to a redirected stream. |
| Narrow terminal | Drop optional fields and truncate safely; never wrap a live row into an unreadable redraw region. |

Progress remains on stderr even for human mode. This intentionally differs
from some existing helper notes that write muted operational text to stdout;
the new renderer must not compromise the documented stdout data contract.

## Timing, Rate, and ETA

- Phase elapsed time uses a monotonic clock in the CLI process.
- Throughput uses a rolling window, not lifetime average, and appears only
  after at least two distinct count samples spanning at least one second.
- ETA appears only when a denominator exists, progress increased, the rolling
  rate is positive, and the estimate has stabilized across multiple samples.
- Suppress ETA when the estimate changes drastically, the phase has stalled,
  or remaining time is below the renderer cadence.
- Reset rate/ETA samples on phase change or denominator change.
- These estimates are presentation-only and never enter durable job state.

## Responsive Layout

The renderer derives width from stderr's terminal and recomputes on update.
The order of optional-field removal is:

1. current item detail;
2. rate and ETA;
3. graphical bar;
4. verbose target/scope wording;
5. full job id, retaining an unambiguous abbreviated prefix.

The active phase, status symbol, best available count, and elapsed time have
highest priority. Paths/URLs are middle-truncated so their distinctive tail is
retained. ANSI escape sequences are excluded from visible-width calculations.

## Interrupt and Cleanup Behavior

- The renderer always clears its live region on success, error, panic boundary,
  channel close, or Ctrl-C handling path.
- It never leaves a half-filled progress bar or cursor-control sequence in the
  terminal transcript.
- The heading exposes the canonical job id as soon as it exists so the operator
  can inspect recovery state after interruption.
- Inline execution must not print "job continues" because process exit can stop
  the active work. It may say `job retained · inspect with axon jobs get <id>`.
- Worker-backed execution may say `Ctrl-C detaches; job continues` only when
  the worker ownership/liveness check makes that statement true.
- Changing all `--wait` paths to enqueue-and-follow is a separate execution
  semantics decision and is not required by this rendering feature.

## Error Handling

- Progress-sink failure is best-effort and cannot fail indexing/extraction.
- Renderer failure clears the live region, logs a concise diagnostic to
  stderr, disables animation, and allows the operation to finish.
- A malformed or incomplete progress event falls back to phase text rather than
  panicking.
- Counts are clamped for display (`done > total` renders `total/total`) while
  the original structured event remains untouched for diagnosis.
- Invalid UTF-8 cannot enter Rust strings; unsafe control characters in source
  labels are replaced before terminal rendering.
- Warning and error rendering uses already-redacted public fields only.

## Files and Ownership

Expected implementation areas; the implementation plan must verify exact names
against the live tree before editing:

| Area | Responsibility |
|---|---|
| `crates/axon-api` | Reuse existing progress DTOs; add no terminal presentation. |
| `crates/axon-services/src/source/execution.rs` | Optional foreground progress sink/handle. |
| `crates/axon-services/src/source/events.rs` | Fan out structured event facts after/beside durable append. |
| `crates/axon-services/src/source/executor/progress.rs` | Fan out coalescible snapshot facts. |
| `crates/axon-services/src/extract.rs` | Expose genuine coarse extract progress facts. |
| `crates/axon-cli/src/commands/wait_progress.rs` | Presentation model, reducer, renderer orchestration. |
| `crates/axon-cli/src/commands/source.rs` | Attach renderer to foreground source execution. |
| `crates/axon-cli/src/commands/sessions.rs` | Reuse renderer per selected session source. |
| `crates/axon-cli/src/commands/extract.rs` | Replace generic spinner with shared renderer. |
| `crates/axon-cli/src/commands/job_progress.rs` | Reuse/generalize phase-aware unit and count formatting. |
| `crates/axon-core/src/ui.rs` | Reusable Aurora progress styles/token helpers only. |

No `mod.rs` files are introduced. Every new Rust test module follows the
required sibling `_tests.rs` convention.

## Testing

### Pure reducer and formatting tests

- phase event starts and completes one milestone;
- rapid snapshots coalesce to the newest visible state;
- identical formatted models do not redraw;
- completed fast phases stay live-only unless otherwise significant;
- warning aggregation groups by code/category/phase;
- redaction copy says `held`, does not claim a confirmed secret, and never
  includes payload text;
- retry aggregation updates attempt count without transcript spam;
- phase aliases collapse batching/embedding/vectorizing appropriately;
- current source units are correct for web/page, local/file, registry/version,
  feed/entry, YouTube/video, session/transcript, and tool/tool-call sources;
- rate and ETA stabilization/suppression rules;
- narrow-width degradation and ANSI-aware visible width;
- control characters are sanitized;
- color-disabled snapshots contain no escape sequences.

### Renderer behavior tests

- human TTY uses one live region and permanent milestone lines;
- redirected stderr emits no cursor-control sequences;
- `--json` and `--quiet` construct no renderer;
- warnings print above the live block without corrupting it;
- success, degraded completion, failure, channel close, and simulated Ctrl-C
  all clear the active region exactly once;
- progress writes to stderr while final human/JSON result remains stdout;
- Aurora product token helpers color only the intended spans.

### Service integration tests

- source foreground sink receives job-started, phase, count, warning, and
  terminal facts in usable order;
- a dropped/closed sink does not fail the source pipeline;
- snapshot pressure does not create an unbounded queue;
- important warning/failure events are not silently lost;
- durable events and foreground event facts describe the same phase/status;
- extract reports real URL/item progress without invented totals.

### Regression gates

- existing source progress and `job_progress` tests;
- existing `status --watch` tests;
- source pipeline sidecar test compilation (`cargo test --no-run` for touched
  crates, not `cargo check` alone);
- CLI JSON contract tests;
- `cargo fmt --all -- --check` and targeted clippy for changed crates.

## Rollout

1. Land the presentation model and deterministic tests with no command wiring.
2. Add the optional source progress feed and service integration tests.
3. Wire source/scrape/sessions foreground paths.
4. Replace extract's generic spinner with the shared coarse renderer.
5. Validate TTY, redirected, color-disabled, JSON, and quiet modes using a
   local binary with `--local` so testing cannot proxy to a stale server.
6. Deploy only after the existing secret-redaction investigation changes and
   this progress work have an explicitly reviewed integration boundary.

## Acceptance Criteria

- A long source `--wait` run shows useful phase/count/current-item progress
  within one second without per-item transcript spam.
- No more than one active region is redrawn, at no more than four visible
  refreshes per second.
- Important warnings, retries, degraded states, and failures remain readable
  after completion and are aggregated safely.
- Redaction progress never exposes payload values or states that policy holds
  prove the presence of a secret.
- Aurora product CLI tokens are used through shared UI helpers.
- Human progress is stderr-only; JSON stdout remains parseable and unchanged.
- Non-TTY output has no animation/control sequences and is substantially quieter
  than the current warning stream.
- Renderer/sink failure cannot fail the underlying source or extract operation.
- Source, scrape, sessions, and extract foreground paths use the common
  rendering model at the granularity each service genuinely provides.

## Open Items for the Implementation Plan

- Confirm the existing valid CLI drill-down syntax before printing it.
- Decide whether `status --watch` should adopt the new live-row formatter in
  the same PR or a small follow-up.
- Define the precise slow-phase threshold using deterministic test clocks;
  one second is the approved default.
- Determine whether extract can expose per-URL callbacks without broad service
  surgery; if not, ship shared styling with coarse stage transitions first.
