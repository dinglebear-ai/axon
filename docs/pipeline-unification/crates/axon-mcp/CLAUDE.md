# axon-mcp Agent Instructions

This file is the agent-facing contract for the `axon-mcp` crate docs.

## When Editing

- Keep MCP server bootstrap, tool model, schema generation, handlers, auth
  extraction, progress mapping, and error mapping here.
- Do not bypass `axon-services`.
- Do not add compatibility aliases for removed actions.
- Update `README.md`, `../../surfaces/tool-contract.md`, and
  `../../schemas/mcp-tool-schema.md` together.

## Review Checklist

- Tool schemas come from shared DTOs.
- Every action maps to one service entrypoint.
- Error envelopes align with REST and CLI JSON output.
