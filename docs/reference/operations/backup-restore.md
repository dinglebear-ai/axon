# Backup And Restore
Last Modified: 2026-07-15

Backup and restore covers Axon's local state and external vector data.

## State To Back Up

- SQLite runtime databases
- source ledger and graph data
- artifacts and prepared documents
- configuration files
- Qdrant collections when they are not disposable

## Restore Rule

Restore must preserve source ids, generation ids, vector payload identity, and
job history compatibility for the current schema. Old legacy family job tables
are not part of the target final runtime.
