# MCP Tool Contract
Last Modified: 2026-07-15

Axon exposes MCP through one action-dispatched tool.

## Contract

MCP callers send an action and a typed request body. The server maps the request
to `axon-api` DTOs, calls `axon-services`, and returns a typed result.

## Removed Actions

Removed source-family actions must not appear in the MCP schema and must not
dispatch through hidden compatibility branches.

## Source Actions

Source acquisition uses the source action with `SourceRequest`. Page, site,
map, and adapter-specific behavior are expressed through source scope, intent,
limits, and options.

## Generated Schema

The generated tool schema is checked in under `docs/reference/mcp/`.
