# axon-observe Agent Instructions

This file is the agent-facing contract for the `axon-observe` crate docs.

## When Editing

- Keep observable events, heartbeats, progress updates, spans, metrics, and
  structured log conventions here.
- Do not add durable job storage, CLI rendering, REST SSE routing, or MCP
  response formatting.
- Update `../../../docs/pipeline-unification/crates/axon-observe/README.md`, `../../../docs/pipeline-unification/runtime/observability-contract.md`, and
  `../../../docs/pipeline-unification/schemas/event-schema.md` together.
- Preserve `job_id`, `source_id`, phase, status, counts, timings, degradation,
  provider, and current-item fields across surfaces.

## Review Checklist

- Events are redaction-safe and schema-backed.
- Metrics avoid high-cardinality labels.
- Long-running operations have heartbeat/progress coverage.
