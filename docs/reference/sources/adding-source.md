# Adding A Source
Last Modified: 2026-07-15

New source families are added through the adapter pipeline.

## Required Pieces

- source classifier or explicit URI prefix
- adapter implementation
- manifest item model
- document preparation strategy
- metadata and vector payload fields
- tests for auth, limits, refresh, and errors
- docs and generated schema updates

## Rule

Do not add a new top-level command or MCP action for each source family. Route
through `SourceRequest` unless the surface is a deliberate transport projection.
