# Full-Pipeline Source Progress Specification

## Status

Normative implementation specification for durable source-job progress across the unified Axon source pipeline.

This specification specializes the existing requirements in `foundation/source-pipeline.md`, `foundation/types/stage-result-contract.md`, `runtime/job-contract.md`, `runtime/observability-contract.md`, and the CLI/MCP/REST surface contracts. Those documents already require progress events for stage batches/items and require every stage result to update job progress and remain renderable on all status surfaces. This document makes that requirement executable and unambiguous.

## Problem

The current unified source runner records phase transitions, but most transitions carry no cumulative counts. Web acquisition can publish per-page progress through `AcquisitionProgressSink`, while other adapters only produce a completed batch result. Preparing, embedding, vector construction, vector upsert, and publishing generally replace the active job phase with `counts: None`.

Consequences:

- `axon status` can show a phase name without meaningful progress.
- adapter aliases leak into presentation decisions.
- later stages overwrite acquisition counts instead of replacing them with stage-appropriate cumulative counts.
- durable jobs, events, and heartbeats do not consistently expose the same progress snapshot.
- the implementation does not meet the pipeline-unification stage-result and observability contracts.

## Goals

1. Every measurable source-pipeline stage exposes cumulative, monotonic progress.
2. Every source adapter receives useful progress without requiring adapter-specific instrumentation.
3. Adapters may provide finer-grained progress when work occurs inside one runner batch.
4. Progress remains source-family-neutral in storage and becomes human-specific only at presentation boundaries.
5. `axon status`, JSON status, MCP task status, REST job status, durable events, and heartbeats observe the same canonical snapshot.
6. Progress publication never makes otherwise-successful source work fail.
7. The implementation obeys repository module, sidecar-test, logging, and monolith policies.

## Non-goals

- Predicting an ETA without stable throughput data.
- Adding family counters such as `pages_crawled` to the canonical DTO.
- Treating unchanged or removed manifest entries as acquisition work.
- Reporting byte progress where providers do not expose reliable totals.
- Requiring every provider API to stream partial results internally.

## Canonical progress model

The canonical persisted payload remains `axon_api::source::StageCounts`:

- `items_total/items_done`: units attempted by the active stage.
- `documents_total/documents_done`: source documents completed by the active stage.
- `chunks_total/chunks_done`: chunks completed by the active stage.
- `bytes_total/bytes_done`: optional only when reliably known.

Counts describe the active phase. They are not lifetime totals with mixed semantics.

### Monotonicity

Within one phase:

- each `*_done` value MUST never decrease;
- each known `*_total` MUST remain stable;
- `*_done` MUST be clamped to its known total;
- batch completion MUST publish a final snapshot even when throttling suppressed intermediate writes.

A phase transition MAY replace counts with the next phase's coordinate system.

## Stage progress semantics

| Phase | Canonical denominator | Required live counters | Completion meaning |
|---|---|---|---|
| `discovering` | unknown until discovery returns | phase-only unless adapter streams discovery | manifest complete |
| `diffing` | manifest items | items | diff classified |
| `fetching` | added + modified items | items, documents | acquisition attempt finished; documents count successful outputs |
| `enriching` | acquired items | items | enrichment attempt finished |
| `normalizing` | acquired items | items, documents | normalized source document produced |
| `preparing` | normalized documents | documents, chunks | document prepared; chunks discovered |
| `batching` | prepared chunks | chunks | embedding batch assembled |
| `embedding` | chunks submitted for embedding | chunks | embedding vector returned or terminally skipped by policy |
| `vectorizing` | embedded chunks | chunks | vector payload built or intentionally skipped by redaction policy |
| `upserting` | vector points | chunks | vector points durably accepted by `VectorStore` |
| `publishing` | one generation | items = 0/1 or phase-only | generation atomically published |
| `cleaning` | cleanup debt items | items | cleanup attempt completed |

When `embed=false`, the runner MUST skip `batching`, `embedding`, `vectorizing`, and `upserting` progress and proceed with prepared document status and publishing.

## Source-family display units

Storage MUST NOT contain human nouns such as pages or files. Human display derives its unit from canonical `SourceKind`, never raw adapter name:

| SourceKind | Human unit |
|---|---|
| Web | page/pages |
| Local, Git, Upload | file/files |
| Registry | version/versions |
| Feed | entry/entries |
| Reddit | item/items |
| Youtube | video/videos |
| Session | transcript/transcripts |
| CliTool, McpTool | tool call/tool calls |
| Memory | memory/memories |

For downstream phases, the phase coordinate system takes precedence over the source noun:

- fetching web: `30/300 pages · 10.0%`
- fetching git: `30/300 files · 10.0%`
- preparing: `80/300 docs · 1,840 chunks`
- embedding: `1,800/5,200 chunks · 34.6%`
- upserting: `1,536/5,200 vectors · 29.5%`

## Shared runner behavior

The unified runner MUST guarantee progress for every adapter:

1. Before acquisition, persist `Fetching` with zero completed items and the total changed-item count.
2. Pass an optional fine-grained sink to `SourceAdapter::acquire_with_progress`.
3. After it returns, publish a runner-owned batch completion snapshot whether or not the adapter emitted progress.
4. Accumulate batch-local snapshots into generation-global counts.
5. Validate and clamp adapter snapshots before persistence.
6. Continue the source pipeline when progress persistence fails, logging a structured warning with job, source, phase, and error.

The default `SourceAdapter::acquire_with_progress` remains valid and delegates to `acquire`. This is the graceful-degradation path for adapters that cannot report inside a batch.

## Fine-grained adapter behavior

Adapters SHOULD emit progress only when they can report work meaningfully before returning:

- web concurrent/sequential page acquisition SHOULD report each completed attempt;
- future streaming or concurrent adapters MAY do the same;
- synchronous adapters need not duplicate per-item loops solely for status because the runner guarantees batch-level progress.

Adapter progress is observational. Sink failure cannot be returned as an acquisition failure.

## Downstream pipeline behavior

The service orchestration layer owns cumulative downstream progress because adapters do not own preparation, embedding, vectors, or job storage.

- preparing publishes after each prepared document batch;
- batching publishes when each `EmbeddingBatch` is assembled;
- embedding publishes after each provider batch returns;
- vectorizing publishes after each point batch is built, including skipped-redaction counts in warnings;
- upserting publishes after each `VectorStore::upsert` result;
- publishing emits a start and terminal generation snapshot;
- document-status writes retain bounded batches but MUST NOT masquerade as embedding progress.

Provider reservation heartbeats MUST carry the latest active phase counts instead of `counts: None`.

## Persistence and observability

A canonical progress publication MUST update:

1. the authoritative job row through `JobStore::update_status`;
2. the durable observability event/heartbeat supplement generated by the job store;
3. the source event stream when the event represents a meaningful batch/item checkpoint.

Status writes MAY be throttled to reduce SQLite contention. Event emission MAY use a coarser cadence than in-memory updates, but the final snapshot for every batch and phase MUST be durable.

## Logging and errors

Progress errors are non-fatal unless the authoritative job transition itself is required for correctness, such as terminal publication status.

Structured warning fields MUST include where available: `job_id`, `source_id`, `phase`, `adapter`, attempted counts, and the underlying error.

Logs and messages MUST pass existing redaction boundaries and MUST NOT include source content, credentials, provider payloads, or unredacted URLs with secrets.

## Documentation requirements

Public Rust types and traits added for progress MUST have rustdoc describing ownership, monotonicity, units, and failure behavior. Internal orchestration helpers MUST document whether counts are batch-local or generation-global.

## Acceptance criteria

The feature is complete only when all of the following are proven:

1. A non-web adapter with no custom progress override shows acquisition progress after each runner batch.
2. Web retains finer per-page progress and preserves bounded concurrency.
3. A source job with multiple document/chunk batches advances through preparing, embedding, vectorizing, and upserting.
4. `embed=false` skips embedding/vector stages without stale counters.
5. Failed progress persistence logs a warning and does not fail successful acquisition or embedding.
6. Counters never regress or exceed totals under malformed adapter snapshots.
7. CLI labels use `SourceKind`, including aliases such as `github`, `npm`, and `pypi`.
8. CLI, JSON status, MCP, REST, job events, and heartbeats expose equivalent phase/count snapshots.
9. Existing legacy progress payloads remain renderable while old local rows may exist.
10. Monolith, layering, formatting, clippy, compile, targeted tests, and required repository gates pass without new allowlist entries.
