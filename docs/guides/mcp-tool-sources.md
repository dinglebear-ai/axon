# MCP Tool Sources
Last Modified: 2026-07-15

MCP tool sources let Axon index connected tool contracts as source material.

## Source Shape

Use an `mcp:` source identifier for a specific upstream tool, for example
`mcp:labby/search`. The router canonicalizes the source to
`mcp://<server>/tools/<tool>`, then the service dispatch layer runs the
`mcp_tool` adapter.

## Behavior

The default dispatch path is metadata-only. It records a ledger generation and
one adapter-owned schema/metadata document, but it does not call the upstream
MCP tool and it writes zero vector points.

Call mode is available only on `scope=api` with `execution_mode=call`. The
service layer re-checks `axon:execute`, requires an exact `mcp_allowlist` entry
for `server/tool`, and requires an explicit `mcp_caller_command` plus exact
`mcp_caller_allowlist`. That caller command is run without a shell, with a
cleared environment except `env_allowlist`, timeout/output caps, and redacted
artifact capture. If no caller command is configured, call mode fails closed.

## Execution Policy

Tool sources require trusted local execution or the `axon:execute` scope at the
source routing boundary, then re-check `axon:execute` at dispatch before any
caller command is spawned. Missing scope, missing target allowlist, missing
caller allowlist, or absent caller command fail closed before side effects.
