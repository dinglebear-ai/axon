# axon-jobs Agent Instructions

This file is the agent-facing contract for the `axon-jobs` crate docs.

## When Editing

- Keep the single durable job model, attempts, events, heartbeats, leases,
  scheduling, watch triggers, worker runtime, and recovery here.
- Do not depend on `axon-services`; inject worker functions/traits from the
  composition layer so the dependency graph stays acyclic.
- Do not reimplement source, parse, embedding, vector, retrieval, LLM, or prune
  domain behavior inside job runners.
- Update `README.md`, `../../runtime/job-contract.md`,
  `../../runtime/observability-contract.md`, and database schema docs together.
- Preserve one `job_id` across logs, events, ledger rows, graph updates, vector
  payloads, and status output.

## Review Checklist

- Workers call injected boundaries supplied by composition.
- Heartbeats are durable and recoverable.
- Provider reservations prevent overload.
