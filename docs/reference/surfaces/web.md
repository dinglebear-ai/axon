# Web Surface

Last Modified: 2026-07-19

The web surface is the browser control panel served by `axon-web` (Axum) at the
configured bind address (`AXON_HTTP_HOST`/`AXON_HTTP_PORT`, default
`127.0.0.1:8001`). It owns browser UI, panel session UX, setup flows, config
editing, dashboards, and interactive inspection. All authoritative mutations go
through the REST/SSE routes — the panel never bypasses services or invents
alternate source semantics.

> Contract source:
> [`docs/pipeline-unification/surfaces/web-contract.md`](../../pipeline-unification/surfaces/web-contract.md).
> Implementation: [`crates/axon-web/src/`](../../../crates/axon-web/src/).

## What the panel hosts

| Area | Routes |
|---|---|
| First-run setup | `POST /api/panel/first-run/crawl`, `POST /api/panel/first-run/ask`, `GET /api/panel/setup/targets` |
| Config/stack inspection | `GET/PUT /api/panel/config` (config.toml), `GET/PUT /api/panel/env` (.env), `GET /api/panel/stack` (runtime mode, service URLs, compose file, Qdrant/TEI/Chrome reachability), `GET /api/panel/status`, `GET /api/panel/doctor`, `GET /api/panel/ops`, `GET /api/panel/collections` |
| Command runner | `POST /api/panel/command` |
| Artifacts | `GET /api/panel/artifacts/{id}/content` |

First-run setup may help create minimal config but must not hide the two-file
`.env`/`config.toml` boundary; restart/reload requirements are shown before save.

## Panel-password auth

On `init_panel_password()`, the server reads `~/.axon/panel-password`; if absent
it generates a 32-byte URL-safe random token and writes it mode `0600` with
`O_NOFOLLOW`. Verification uses `subtle::ConstantTimeEq`. `POST /api/panel/login`
checks the password and returns the token; panel routes are gated by the
session cookie.

> **Open followup (audit U3-V02, 2026-07-09):** the live web app currently
> stores its bearer token in `localStorage`. The contract target is httpOnly
> `Secure` `SameSite=Strict` session cookies. Tracked as an open code followup.

## Responsibilities

- Serve the web UI.
- Expose REST routes (`/v1/*`) and MCP HTTP transport (`/mcp`) on the same listener.
- Enforce HTTP auth and security headers.
- Render shared job, source, query, ask, and memory DTOs.

## Rule

The web UI must not reimplement source acquisition, retrieval, graph, memory,
vector, provider, or job logic — it consumes shared DTOs and the REST/SSE
routes. Status vocabularies must match CLI/Palette/Android/extension.

If the panel surface changes, update this file and
[`crates/axon-web/src/CLAUDE.md`](../../../crates/axon-web/src/CLAUDE.md) in the
same PR.
