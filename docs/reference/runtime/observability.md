# Observability
Last Modified: 2026-07-15

Observability covers events, pipeline phases, job progress, warnings, and
runtime diagnostics.

## Requirements

- Every source pipeline stage emits structured progress or events.
- Warnings are visible in results and durable job records.
- Provider cooling, retries, and degraded completion are observable.
- Redaction is applied before user-visible logs and persisted events.

## Consumers

CLI, MCP, REST, web, Palette, Android, and Chrome surfaces should all read the
same event and progress contracts.
