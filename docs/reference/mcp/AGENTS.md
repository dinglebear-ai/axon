# MCP documentation maintenance

Read [overview](overview.md), [tool reference](tools.md), [transport](transport.md), [environment](env.md), and [client connection guide](connect.md).

For implementation work, pair [MCP action development](../../development/adding-mcp-action.md), [patterns](patterns.md), [developer notes](dev.md), and the [actual transport guide](../../../crates/axon-mcp/src/AGENTS.md).

## Catalog and routing accuracy

The primary action schema is not the entire tool catalog. Inspect [server registration](../../../crates/axon-mcp/src/server.rs), [runtime schema assembly](../../../crates/axon-mcp/src/server/tool_schema.rs), and [system/watch requests](../../../crates/axon-mcp/src/server/system_requests.rs). Include the dashboard tool, resources, and task protocol behavior when relevant.

Do not infer operation absence from AxonRequest alone, document unmerged projection work as shipped, or claim a schema snapshot proves a live call works. Keep examples aligned with current accepted input, auth, artifact IDs, task IDs, and error envelopes.

## Verification

Wire changes need real discovery/calls and failure paths, not only mock fixtures. Use the [MCP qualification harness](../../../tests/e2e/mcp/) and [task wire test](../../../scripts/test-mcp-tasks-wire.py) with the matching build and explicitly configured test target.

Regenerate [tool schema](tool-schema.md) from owning inputs; follow [documentation validation](../../development/documentation.md). Deployment examples follow [the supported deployment contract](../../operations/deployment.md); private topology is not a shared default.
