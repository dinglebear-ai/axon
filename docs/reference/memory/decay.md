# Memory Decay
Last Modified: 2026-07-15

Memory decay governs when durable memories should be reviewed, superseded, or
excluded from retrieval.

## Principles

- Newer observations can supersede older memories.
- Memories should preserve provenance and source links.
- Decay should reduce stale retrieval without deleting useful evidence.

## Signals

Useful decay signals include age, supersession edges, failed validation,
low-confidence extraction, and explicit review outcomes.

## Ownership

Memory lifecycle DTOs live in `axon-api`; persistence and retrieval behavior
live in memory and retrieval services.
