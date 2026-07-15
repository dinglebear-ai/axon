# Runtime Jobs
Last Modified: 2026-07-15

Axon jobs are durable records in the unified job store.

## Policy

The final runtime uses one durable job model. Legacy source-family job tables
and legacy family `JobKind` variants are removal targets.

## Job Records

Jobs include kind, status, source identity when applicable, request payload,
progress, result, errors, timestamps, and auth snapshot metadata.

## Workers

Workers claim pending jobs, transition lifecycle status, emit observability
events, and persist terminal results through the unified store.
