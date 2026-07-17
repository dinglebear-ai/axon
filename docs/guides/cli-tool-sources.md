# CLI Tool Sources
Last Modified: 2026-07-15

CLI tool sources let Axon document local command surfaces through the unified
source pipeline.

## Source Shape

Use a `cli:` source identifier for tool documentation targets, for example
`cli:rg --help`. The router canonicalizes the source to `cli://<command>`, then
the service dispatch layer runs the `cli_tool` adapter.

## Pipeline Behavior

The default dispatch path is metadata-only. It records a ledger generation and
one adapter-owned metadata document, but it does not spawn the command and it
writes zero vector points.

Execution mode is available only on `scope=api` with `execution_mode=execute`.
The service layer re-checks `axon:execute`, requires an exact
`command_allowlist`, denies shell expansion, applies the local secret-path
denylist to the command and argv, clears the child environment except
`env_allowlist`, enforces timeout/output caps, and stores only redacted output
artifacts.

## Safety

Tool sources require trusted local execution or the `axon:execute` scope at the
source routing boundary, then re-check `axon:execute` at dispatch before any
process is spawned. Missing scope, missing allowlist, unsupported shell forms,
or secret-like local paths fail closed before side effects.
