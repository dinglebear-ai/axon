# Runtime Jobs
Last Modified: 2026-07-16

Axon jobs are durable records in the unified job store.

## Policy

The runtime uses one durable job model. Source acquisition, extraction,
watch-triggered runs, memory work, pruning, reset, and other operations differ
by canonical job kind while sharing lifecycle, attempt, stage, event,
heartbeat, artifact, reservation, and recovery tables. Legacy source-family
job tables and `JobKind::{Crawl, Embed, Ingest}` variants are absent.

## Job Records

Jobs include kind, status, source identity when applicable, request payload,
progress, result, errors, timestamps, and auth snapshot metadata.

## Workers

Workers claim pending jobs, transition lifecycle status, emit observability
events, and persist terminal results through the unified store.
