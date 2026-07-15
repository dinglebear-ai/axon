# Runtime Auth
Last Modified: 2026-07-15

Runtime auth covers local CLI trust, HTTP bearer/OAuth modes, and source-policy
checks.

## Principles

- HTTP access requires configured auth outside loopback development.
- Local CLI calls are locally trusted but still go through source safety checks.
- Source execution rechecks auth at execution time.
- Destructive operations require write/admin policy and explicit confirmation.

## Source Auth

Local paths, tool sources, MCP sources, and private network targets require
policy checks before acquisition and before execution.
