# MCP Tool Sources
Last Modified: 2026-07-15

MCP tool sources let Axon index connected tool contracts as source material.

## Source Shape

Use an `mcp:` source identifier for a specific upstream tool, server, or tool
catalog selection. The adapter owns discovery and schema normalization.

## Behavior

Tool descriptions, JSON schemas, permissions, and examples are prepared as
documents. Vector payloads should preserve upstream identity, tool name,
capability family, and schema fingerprints.

## Execution Policy

Indexing tool contracts must not execute mutating tools. Execute-mode tool
work is a separate design surface and must have explicit auth and idempotency.
