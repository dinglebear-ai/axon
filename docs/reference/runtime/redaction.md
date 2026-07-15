# Redaction
Last Modified: 2026-07-15

Redaction prevents secrets and sensitive local data from leaking into logs,
events, artifacts, generated docs, or user-visible errors.

## Scope

Redaction applies to:

- config and environment values
- HTTP headers and tokens
- local paths and secret-looking filenames
- provider errors
- tool and MCP source metadata

## Rule

Every logging and diagnostic path must use shared redaction helpers instead of
ad hoc string formatting for sensitive values.
