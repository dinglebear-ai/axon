# Axon Output And Logging Hardening

Date: 2026-05-24

## Summary

The console output regression was in the server-mode stdout renderer, not in
tracing configuration. Server mode receives structured REST JSON and then adapts
it back to CLI output in `src/cli/server_mode/render.rs`; any command without an
explicit human renderer could previously leak JSON by default.

## Applied

- Default stdout rendering now stays human-readable for server-mode `ask`.
- `ask --json` remains structured JSON.
- Server-mode metadata lines now use the shared 120-character display cap for
  URLs, titles, warnings, query snippets, and suggestions.
- Search human output uses the same display-cap helper for title, URL, and
  snippet lines.
- Regression tests cover server-mode `ask` human/default and JSON/explicit
  behavior.

## Logging Audit

`src/core/logging.rs` already uses split sinks:

- Console: human formatter on stderr with ANSI gated by terminal/force-color
  behavior.
- File: rotating JSON formatter with `with_ansi(false)`.
- File path: `AXON_LOG_PATH`, defaulting below the Axon data directory.
- Rotation: `AXON_LOG_MAX_BYTES` and `AXON_LOG_MAX_FILES`.

No tracing change was needed for this pass. The stdout command-rendering path
was the divergent behavior.

## Follow-Up Risk

Freeform command payloads such as retrieved document content and generated
answers remain uncapped so they stay pipeable. Metadata rows around those
payloads are capped.
