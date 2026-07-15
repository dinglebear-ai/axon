# Runtime Schema
Last Modified: 2026-07-15

Runtime schema covers SQLite migrations, durable job tables, source ledger
tables, graph tables, memory tables, and observability tables.

## Source Of Truth

SQLite migrations are the source of truth. Generated schema references should
be regenerated after migration changes.

## Final State

The final runtime schema uses unified jobs and source-pipeline stores. Legacy
source-family job tables are not retained as active compatibility surfaces.
