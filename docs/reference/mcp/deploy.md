# Deployment Guide -- Axon MCP

Last reviewed: 2026-09-29

The supported production contract is **native `axon serve` under systemd**,
either on bare-metal Linux or in Incus. MCP HTTP shares the listener,
authorization boundary, and workers with REST and the web panel. Qdrant,
TEI/embedding, and Chrome/CDP are external providers as required by the
operation. Compose is a development/reference surface, not the production
Axon process manager.

## Choose the verified installation

Use [deployment](../../operations/deployment.md),
[bare-metal systemd](../../../deploy/systemd/README.md), or
[Incus](../../../deploy/incus/README.md), then verify the actual service,
executable, effective environment, data paths, mounts, and ownership. These
examples describe different service names and layouts; do not assume
`axon.service` and `axon-native.service` are interchangeable.

The tracked Incus bootstrap defaults to a host-owned queue and leaves the
guest Axon service disabled unless explicitly enabled for guest-exclusive
state. That default is not proof of how another installation runs. Do not
let host and guest workers open the same SQLite/WAL files through a bind
mount. Confirm the actual writer and storage boundary before enabling a
second service.

## Local development

For a foreground stdio process using existing providers:

```bash
axon mcp
```

For the unified loopback HTTP service:

```bash
AXON_HTTP_HOST=127.0.0.1 AXON_HTTP_PORT=8001 axon serve
```

`just dev` is a broader development recipe: it calls `just stop`, builds the
debug binary, starts the configured Compose TEI/Chrome services, then runs
`axon mcp`. It can interrupt an existing development runtime and does not
mean HTTP was enabled. Inspect the recipe and its environment before use.

Use `just services-up` only for the deliberately selected development
provider stack. Container ports, model caches, image tags, and optional
Qdrant profiles belong to the current Compose manifests, not copied tables
in this MCP guide.

## Configure without replacing existing state

Non-loopback HTTP requires the configured bearer/OAuth policy. Keep panel
unlock separate from API authorization. Set origins and local acquisition
roots deliberately; no client registration authorizes arbitrary server
filesystem access. See [environment](env.md) and
[security](../../operations/security.md).

Retain unrelated configuration and credentials. Verify the running process
loaded the expected file and values, not merely that a template was written.
Document a restart/reconnect requirement for startup-static projection or
auth changes.

## Verify after installation or upgrade

Record the installed binary version and the service executable path. Check
process health, provider readiness, worker liveness, and a real initialized
MCP client connection separately. Discover the full tool/resource catalog
and make an authorized read-only call before testing writes.

For an isolated source smoke test, retain the returned job ID and verify
its terminal status and committed publication. A listening port, successful
configuration write, or detached job ID is not completion. Use
[the connection guide](connect.md) and [jobs](../runtime/jobs.md).

Back up consistent SQLite, artifacts, and compatible vectors before upgrades
that affect durable state. Rollback uses the installation's recorded binary,
configuration, and recovery point, not an automatic `docker compose down`.
Follow [backup/restore](../operations/backup-restore.md).
