# Job Contract
Last Modified: 2026-06-30

## Contract

Axon has one durable job model. It does not have separate infrastructure models
for crawl jobs, embed jobs, ingest jobs, extract jobs, watch jobs, prune jobs,
or research jobs.

Different work is represented by `job_kind`, `job_intent`, stages, attempts,
events, artifacts, and result payloads. The outer lifecycle, status API,
heartbeat behavior, cancellation, retry, recovery, progress shape, and
observability are shared.

```text
Job
  -> JobAttempt
  -> JobStage
  -> JobEvent
  -> JobHeartbeat
  -> JobArtifact
  -> JobResult
  -> JobStatus
```

Every async or detached operation returns a `JobDescriptor`. Foreground CLI
operations still create a job row when they perform source acquisition,
embedding, graph mutation, pruning, extraction, research, or long-running
provider work.

## Design Rules

- One jobs table family owns lifecycle for every long-running operation.
- `job_id` is the primary correlation field across CLI, MCP, REST, logs,
  traces, SourceLedger, SourceGraph, ArtifactStore, VectorStore payloads, and
  progress output.
- Job infrastructure does not know crawl/embed/ingest as separate queue types.
  It knows `job_kind`, stage plan, requirements, provider reservations, and
  result schema.
- A job can contain many stages, but it has one terminal status.
- Attempts are explicit. Retrying a job creates a new attempt under the same
  `job_id` unless the caller explicitly creates a new job.
- Cancellation is cooperative and stage-aware.
- Recovery is lease/heartbeat based and never assumes a process died solely
  because work is slow.
- Progress is event sourced enough for CLI/MCP/REST to render the same story.
- Provider throughput is reserved by the scheduler before stage execution.
- No stage may hot-loop a failing provider. Cooling/backoff belongs to the job
  scheduler plus provider boundary.

## Job Kinds

| Kind | Purpose | Typical Stages |
|---|---|---|
| `source` | Acquire, normalize, embed, publish one source. | resolve, discover, diff, fetch, prepare, embed, upsert, publish, clean |
| `watch_run` | Execute one watch tick/run. | lease, resolve, diff, source/create-child-jobs, complete |
| `map` | Discover items without embedding. | resolve, discover, artifact, complete |
| `extract` | Structured LLM extraction. | fetch, normalize, llm_extract, validate, artifact |
| `research` | Search/fetch/synthesize. | search, fetch, prepare_context, synthesize, optional_source_jobs |
| `ask` | Retrieval + synthesis. | retrieve, prepare_context, synthesize |
| `query` | Retrieval-only async query when needed. | embed_query, retrieve, rank |
| `prune` | Delete vectors/artifacts/ledger rows by selector. | plan, approve, delete, verify |
| `graph` | Graph extraction, merge, repair, or rebuild. | parse, candidate, merge, verify |
| `memory` | Memory lifecycle work. | validate, embed, store, link, decay |
| `provider_probe` | Health/capability check. | probe, classify, publish_status |
| `reset` | Explicit destructive local store reset. | plan, approve, delete, verify |

`source` is the normal ingestion/indexing job kind. Former top-level crawl,
scrape, embed, ingest, sessions, GitHub, crates, YouTube, RSS, Reddit, local
files, CLI tool, and MCP tool ingestion paths become source jobs with different
adapters and scopes.

## Status Model

| Status | Terminal | Meaning |
|---|---:|---|
| `queued` | no | Accepted but not eligible to run yet. |
| `blocked` | no | Waiting on dependency, capacity, cooldown, or explicit approval. |
| `running` | no | Attempt is active and heartbeating. |
| `canceling` | no | Cancellation requested; stages are unwinding. |
| `completed` | yes | Required stages succeeded. |
| `completed_degraded` | yes | Required contract succeeded with declared degradation. |
| `failed` | yes | Required stage failed. |
| `canceled` | yes | User/system cancellation completed. |
| `expired` | yes | Job passed retention/deadline without safe recovery. |

`completed_degraded` is success with warnings. It must include explicit
degradation codes, affected stages, and missing optional capabilities.

## Required Job Fields

| Field | Type | Meaning |
|---|---|---|
| `job_id` | uuid | Durable operation id. |
| `job_kind` | enum | Work family from the registry above. |
| `job_intent` | enum | `index`, `refresh`, `watch`, `map`, `retrieve`, `answer`, `extract`, `prune`, etc. |
| `status` | enum | Current lifecycle status. |
| `phase` | enum | Current observable phase. |
| `request_id` | string | Transport invocation id. |
| `source_id` | string? | Source lifecycle id when applicable. |
| `watch_id` | string? | Watch id when applicable. |
| `parent_job_id` | uuid? | Parent job for fan-out/fan-in. |
| `root_job_id` | uuid | Top-level job tree id. |
| `attempt` | integer | Current attempt number. |
| `priority` | enum | `low`, `normal`, `high`, `interactive`. |
| `created_at` | timestamp | Submission time. |
| `started_at` | timestamp? | Current attempt start. |
| `updated_at` | timestamp | Last status/event/heartbeat update. |
| `deadline_at` | timestamp? | Optional cancellation deadline. |
| `completed_at` | timestamp? | Terminal time. |
| `idempotency_key` | string? | Optional de-dupe key. |
| `auth_snapshot` | object | Immutable caller id, transport, scopes, visibility ceiling, request time, and auth policy version. |
| `config_snapshot_id` | string | Immutable config/provider snapshot used by the job. |
| `stage_plan` | array | Ordered planned stages. |
| `requirements` | object | Provider/capacity/security requirements. |
| `result_schema` | string | Result DTO discriminator. |
| `warnings` | array | Current warning/degradation summaries. |
| `error` | `ApiError`? | Terminal or current blocking error. |

## Stage Model

Stages are declared in the job plan before execution when possible.

Required stage fields:

| Field | Meaning |
|---|---|
| `stage_id` | Stable id within the job. |
| `phase` | Observable phase from `observability-contract.md`. |
| `status` | `pending`, `running`, `skipped`, `completed`, `degraded`, `failed`, `canceled`. |
| `required` | Whether stage failure fails the job. |
| `provider_requirements` | Provider classes and capacity units required. |
| `input_counts` | Planned items/chunks/bytes when known. |
| `output_counts` | Produced items/chunks/bytes. |
| `started_at` / `completed_at` | Timing. |
| `error` | Stage error when failed/degraded. |

Stages may be repeated by batch. Batch-level events must include batch id,
item counts, chunk counts, byte counts, provider reservation ids, and elapsed
time.

## Event Model

Every job event uses the progress shape in `observability-contract.md`.

Additional job-event fields:

| Field | Required | Meaning |
|---|---:|---|
| `event_id` | yes | Durable monotonic event id. |
| `sequence` | yes | Monotonic per job. |
| `attempt` | yes | Attempt number. |
| `stage_id` | no | Stage associated with the event. |
| `batch_id` | no | Batch associated with the event. |
| `reservation_id` | no | Provider capacity reservation. |
| `checkpoint_id` | no | Resume checkpoint. |
| `dedupe_key` | no | Used to avoid repeated noisy events. |

Events are append-only. Status rows may cache latest state but must not be the
only source for progress history once the unified job model lands.

## Heartbeats

Every active job attempt heartbeats at a bounded interval.

Heartbeat fields:

| Field | Meaning |
|---|---|
| `job_id` | Job id. |
| `attempt` | Attempt number. |
| `worker_id` | Worker process/thread identity. |
| `phase` | Current phase. |
| `stage_id` | Current stage. |
| `last_event_sequence` | Last emitted event. |
| `progress_counts` | Latest counts snapshot. |
| `provider_reservations` | Active reservation ids. |
| `heartbeat_at` | Timestamp. |

Recovery uses heartbeat age plus provider reservation state plus stage
checkpointability. A slow embedding batch is not stale if the provider
reservation is still active and the worker heartbeat is fresh.

Heartbeat and watchdog rules:

- active attempts heartbeat at least every `jobs.heartbeat_interval_secs`
- long provider calls emit heartbeat updates before and after each reservation
  state transition
- every worker lane has a bounded input channel; overflow blocks admission or
  returns a structured backpressure error
- panics are caught at the worker boundary, recorded as failed attempts, and
  never leave leases permanently owned
- starvation watchdogs emit warning events when interactive lanes wait longer
  than their configured SLO
- recovery checks job heartbeat, stage checkpoint, provider reservation state,
  and lease expiry before creating a new attempt
- recovery never republishes an already committed generation; it creates a new
  generation or records cleanup debt

## Provider Capacity and Backpressure

The job scheduler owns pipeline backpressure. Stage code asks for capacity; it
does not independently flood providers.

Provider capacity classes:

| Class | Used By | Resource Protected |
|---|---|---|
| `embedding` | document/query embedding | TEI/OpenAI embedding throughput, GPU memory, request tokens |
| `vector_write` | upsert/delete | Qdrant write IO and optimizer pressure |
| `vector_read` | query/retrieve/ask | Qdrant read latency and HNSW pressure |
| `llm` | ask/research/extract/summarize/evaluate | LLM concurrency and cost |
| `fetch` | HTTP/git/registry/social fetch | network/file descriptors/remote limits |
| `render` | Chrome/CDP/screenshot/endpoints | browser tabs, memory, CPU |
| `parse` | AST/schema/session parsing | CPU and memory |
| `graph_write` | graph merge | SQLite/object locks |
| `artifact_write` | output/WARC/screenshots | disk IO |

Embedding bottleneck rule:

- Source jobs must reserve `embedding` capacity before creating embedding
  batches.
- Query/ask jobs use a separate low-latency embedding pool or priority lane.
- Watch refreshes and bulk backfills use background priority by default.
- Provider limits apply across all jobs, not per command family.
- `EmbeddingProvider` only embeds; it does not decide global concurrency,
  fairness, retries across jobs, or whether to starve interactive retrieval.
- Batch size, in-flight inputs, request concurrency, and retry backoff are
  provider capabilities plus config knobs consumed by the scheduler.

Fairness requirements:

- Interactive `ask`, `query`, and `retrieve` must not wait behind unbounded
  bulk source embedding.
- Watch jobs must coalesce duplicate refresh requests for the same source.
- Large source jobs must yield between batches and update progress.
- Provider cooldown blocks new reservations but allows safe cleanup/finalization
  stages to run.

## Cancellation

Cancellation is cooperative:

- queued jobs cancel immediately
- running jobs enter `canceling`
- active stages check cancellation between batches/items
- provider requests are aborted when safe
- committed generations are never half-published
- cleanup debt is recorded for any published partial side effect

Cancellation result includes:

| Field | Meaning |
|---|---|
| `canceled_at` | Timestamp. |
| `canceled_by` | User/system identity. |
| `last_safe_stage` | Last completed safe point. |
| `side_effects` | Published/written side effects. |
| `cleanup_debt_ids` | Cleanup work created. |

## Retry and Recovery

Retry policy is stage-aware.

Retryable examples:

- provider unavailable
- fetch timeout
- rate limited with retry-after
- transient Qdrant write failure before publish
- worker crash before commit

Not retryable without mutation:

- unsupported source scope
- invalid schema
- authorization denied
- redaction failure
- missing required credential

Recovery rules:

- A stale `running` attempt is not modified until its lease is expired and
  heartbeat grace elapsed.
- Recovering a job creates a new attempt.
- Non-idempotent stages require checkpoints before retry.
- Published generations are immutable; retry creates a new generation unless
  the prior generation was never committed.
- Cleanup debt is retried independently and does not block status reads.

## Parent and Child Jobs

Job fan-out is explicit.

Examples:

- `research` may create child `source` jobs for result URLs.
- `watch_run` may create child `source` refresh jobs.
- `source` for an org may create child source jobs for repos/packages.
- `reset` may create child prune or cleanup jobs.

Parent jobs aggregate child status:

| Parent Status | Rule |
|---|---|
| `completed` | All required children completed. |
| `completed_degraded` | Required children completed and optional children failed/degraded. |
| `failed` | Required child failed and policy is fail-fast/fail-final. |
| `canceling` | Cancellation propagating to children. |

## Job API Requirements

Every transport exposes the same job operations:

| Operation | Meaning |
|---|---|
| create | Submit job from service request. |
| get | Latest status. |
| list | Filtered page. |
| events | Durable event page. |
| stream | SSE or streaming event projection. |
| cancel | Cooperative cancellation. |
| retry | New attempt from immutable request/config snapshot. |
| recover | Admin/system stale recovery. |
| cleanup | Retention cleanup of terminal jobs/events. |
| artifacts | Artifacts produced by job. |

## Retention

Default retention:

| Data | Default |
|---|---|
| terminal job rows | 30 days |
| detailed events | 14 days |
| failed job events | 60 days |
| artifacts | source/job policy |
| cleanup debt | until completed |
| config snapshots | at least as long as terminal jobs |

Retention is config-driven, but cleanup must preserve enough evidence for
SourceLedger, SourceGraph, and VectorStore payloads to remain explainable.
