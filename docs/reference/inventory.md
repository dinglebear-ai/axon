# Component Inventory: Axon

Last reviewed: 2026-09-29

This is the navigation map for current component inventories. Exact command,
route, schema, dependency, and table lists are generated from their owners.
Do not treat a hand-copied subset or a historical migration plan as the
complete runtime catalog.

## Authoritative inventories

| Surface | Inventory | Owning source |
|---|---|---|
| CLI | [Commands](cli/commands.md), [JSON](cli/commands.json), [action guides](actions/README.md) | CLI/core parser and exported command registry |
| MCP | [Tool reference](mcp/tool-schema.md), [JSON contract](mcp/tool-schema.json) | `axon-mcp` operation registry, projection metadata, server handlers, and resources |
| HTTP/REST | [Route reference](rest/openapi.md), [JSON](rest/openapi.json), [HTTP guide](http-api.md) | `axon-web` route and schema registries |
| Public DTOs | [DTO reference](api/dto.md), [schema bundle](api/schemas.json) | `axon-api` and exporting owners |
| Configuration | [Configuration reference](config/), [environment reference](config/env.md) | Typed config parsing, environment registry, and defaults |
| Source adapters | [Scope matrix](sources/adapter-scopes.md) | Adapter specs and canonical family/scope declarations |
| Presentation | [Surface reference](surfaces/presentation.md), [generated contracts](generated/presentation.md) | Shared presentation contracts and client projections |
| SQLite | [Database schema](runtime/database-schema.md), [JSON](runtime/database-schema.json) | Owning crate schema/migration implementations |
| Dependencies | [Crate graph](crate-dependency-graph.md) | Cargo manifests |
| Public Rust API | [API surface](public-api-surface.md) | Exported Rust items |

The ordered `cargo xtask generated-contracts refresh` / `check` workflow
regenerates owned artifacts and verifies drift. Some specialist references
have their own generator entry points;
[documentation maintenance](../development/documentation.md) is the owner map.

## MCP tools

The server supports legacy, atomic, and combined projections. Legacy mode
uses the `axon` tool with action/subaction selectors. Atomic mode publishes
canonical leaf names and excludes the fixed routing selectors from their
inputs. The configured projection determines discovery, not a static tool
count in this page.

The auxiliary `axon_status_dashboard` tool is separate from the primary
action catalog. System/watch request types and task handlers also live
outside that primary enum. Inspect
[`crates/axon-mcp/src/server.rs`](../../crates/axon-mcp/src/server.rs) and a
matching live `tools/list` response when changing transport coverage.

Focused source projections reuse the shared source services; they do not
authorize separate crawl/embed/ingest queues. Family-level navigation does
not imply identical request schemas or suboperations on CLI, REST, and MCP.
See [MCP overview](mcp/overview.md).

## MCP resources

The server exposes the legacy schema resource `axon://schema/mcp-tool`, the
canonical operation/projection resource `axon://schema/mcp-operations`, and
the dashboard UI resource `ui://axon/status-dashboard`, alongside its
artifact/resource behavior. Discovery and read-resource authorization must
match the running build. Follow opaque artifact IDs rather than server paths.
The [tool contract](mcp/tool-contract.md) and
[transport guide](mcp/transport.md) cover those boundaries.

## CLI commands

Use the generated CLI registry for exact names and the handwritten action
guides for operational behavior. `axon <source>` is the unified source
entry point; `axon sessions` selects local transcript roots; `axon jobs`
provides canonical durable lifecycle inspection/control. A command can
enqueue work without completing it.

CLI commands run in-process rather than using the removed generic server
forwarding mode. Remote callers use REST or MCP with server-side authorization,
workers, and state. A compatibility helper implemented by a command is not
a separate storage authority.

## Infrastructure services

The supported deployment is native `axon serve` under systemd in Incus or
bare-metal Linux, with external Qdrant, TEI/embedding, and Chrome/CDP
providers as required by the selected operation. Provider health and
availability are runtime facts, not established by a compose file or schema.

[Deployment](../operations/deployment.md) documents the supported contract.
Tracked Compose files are development/reference surfaces; read their current
image tags, ports, and overrides rather than relying on duplicated values
here. Durable jobs use SQLite and in-process workers, not a message broker.

## Client applications and plugins

| Component | Surface/ownership |
|---|---|
| [Web panel](surfaces/web.md) | Bundled web assets and the shared HTTP API |
| [Palette](surfaces/palette.md) | Desktop/Tauri client over shared contracts |
| [Android](surfaces/android.md) | Native Android client and generated API bindings |
| [Chrome extension](surfaces/chrome-extension.md) | Browser client over the same server contracts |
| [Axon usage plugin](../../plugins/axon/README.md) | Usage skills and Labby snippet source; no bundled binary |
| [Install Axon plugin](../../plugins/install-axon/README.md) | Separate installation/setup workflow |

A repository hook is not automatic plugin provisioning. Plugin configuration
and available live gateway tools must be verified in the actual client.

## Worker and source ownership

[Jobs](runtime/jobs.md) owns the durable lifecycle; [ledger](runtime/ledger.md)
owns source identity, manifests, generations, document status, and cleanup
debt. One source job spans acquisition through publication and cleanup.
Adapters return normalized documents; services compose preparation,
embedding, vectors, graph, and cleanup.

Use [crate structure](../architecture/crate-structure.md),
[crate ownership](../architecture/crate-ownership.md), and
[source pipeline](../architecture/source-pipeline.md) for the domain map.
Schema declarations such as adapter onboarding status check completeness of
declarations, not successful provider execution.

## SQLite tables and migrations

Table names and upgrade paths belong to the generated database reference and
the owning crate migrations. Do not assume root-level migrations or old
per-family tables still exist. Existing-state upgrade tests are required when
a schema changes; applied migrations are not rewritten.

## Scripts

[Developer scripts](../development/repo/scripts.md) links the current tooling
entry points. Generator/check failures must identify stale or missing input
and recovery steps. A missing script is not an optional successful smoke test.
Use an inspected, matching build for transport verification, and record
provider-dependent tests that were not run.
