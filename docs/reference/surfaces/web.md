# Web Surface
Last Modified: 2026-07-15

The web surface is an Axon client and HTTP host.

## Responsibilities

- serve the web UI
- expose REST routes and MCP HTTP transport
- enforce HTTP auth and security headers
- render shared job, source, query, ask, and memory DTOs

## Rule

The web UI must not bypass services or invent alternate source semantics.
