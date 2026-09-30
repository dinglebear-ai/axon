---
title: "Deployment Guide"
created: 2026-02-25
updated: 2026-09-29
---

# Deployment Guide

Last reviewed: 2026-09-29

## Scope

Production Axon runs as native `axon serve` under systemd on bare-metal Linux
or in Incus, with external Qdrant, embedding, and Chrome/CDP providers. The
unified process hosts REST, MCP HTTP, the panel, and in-process workers.
Compose is a local development/reference provider surface, not the supported
production Axon runtime.

## Deployment targets

| Target | Owning instructions | Important distinction |
|---|---|---|
| Bare-metal Linux | [systemd guide](../../deploy/systemd/README.md) and [unit](../../deploy/systemd/axon.service) | The example service is `axon.service` |
| Incus | [Incus guide](../../deploy/incus/README.md) and bootstrap/profile sources | The generated native guest service is `axon-native.service`; current bootstrap defaults may leave it disabled |
| Local development | [Justfile](../../Justfile), [Compose definitions](../../docker-compose.yaml), and [developer setup](../../scripts/dev-setup.sh) | Commands can start/stop local providers and modify developer setup |

Inspect the verified local deployment notes before choosing commands. The
tracked Incus workflow currently defaults to a host-owned queue and external
host TEI, with guest Axon enabled only for guest-exclusive state. This does
not establish another installation's topology. Never start host and guest
writers against the same SQLite/WAL files through a bind mount.

## Prerequisites and target identity

Record the target hostname, user, operating system/architecture, intended
binary path/version, service unit, environment/configuration files, data and
artifact paths, and provider origins. Confirm required access and available
disk/memory before changing an installation.

Axon itself does not require Docker or a GPU. Embedding, rendering, search,
and synthesis operations need their selected providers. TEI GPU requirements
belong to the TEI host/image, not every Axon client. A local developer
bootstrap is not a universal production prerequisite.

## Day-0 bootstrap

For a new developer environment, inspect `scripts/dev-setup.sh` and its
platform-specific helpers before running it. It can install tooling, modify
configuration, install hooks, and start configured provider containers.
Existing secret files and unrelated keys must be preserved. Do not call it
side-effect-free or use it as a routine production upgrade command.

```bash
./scripts/dev-setup.sh --no-docker
# For an intentionally configured development provider stack:
./scripts/dev-setup.sh
```

Use the supported native deployment guide for a service installation. Verify
actual runtime state after setup rather than treating a generated unit or
configuration file as successful deployment.

## Configuration

Precedence is **CLI > environment > TOML > defaults**. Typed configuration
owners and generated references define accepted keys; templates provide
examples, not a safe replacement for an existing configured file.
[Configuration](../guides/configuration.md) documents loading and migration.

The running service environment is authoritative. Editing a workstation
`~/.axon/.env` does not change a remote service. Confirm the unit's executable,
HOME, `AXON_DATA_DIR`, environment-file permissions, and any explicit SQLite
or artifact overrides. Avoid printing secret values when inspecting them.

Key prerequisites include a reachable `QDRANT_URL`, the selected embedding
provider such as `TEI_URL`, and `AXON_CHROME_REMOTE_URL` for browser operations.
Synthesis uses the configured backend, not always Gemini. With
`AXON_LLM_BACKEND=openai-compat`, use `AXON_OPENAI_BASE_URL`,
`AXON_SYNTHESIS_OPENAI_MODEL`, and an endpoint credential when required.
`AXON_OPENAI_MODEL` is removed and rejected, not a supported alias.

Non-loopback HTTP requires `AXON_HTTP_TOKEN` or configured OAuth. Panel
password/session unlock is separate from API/MCP authorization. Configure
allowed origins and source access deliberately. See [security](security.md)
and [MCP auth](auth/mcp-auth.md).

## Standard deploy procedure

Review the versioned artifact and compatibility changes first. Back up a
consistent recovery point before schema/configuration migrations or other
durable-state changes. Follow [backup and restore](../reference/operations/backup-restore.md).

Install the verified binary at the path actually referenced by the service.
Preserve the previous binary and effective non-secret configuration record.
Apply only required key changes, keeping existing credentials and permissions.
Restart the identified native service only after target/effects are approved.
A generic service name copied from another host is not a safe substitute.

For a deliberately selected local provider stack only:

```bash
docker compose --env-file ~/.axon/.env -f docker-compose.prod.yaml up -d axon-tei axon-chrome
```

Qdrant may already be external. Read the current manifests for optional
profiles, service URLs, images, and mounts. Do not start a second Qdrant or
change collections simply because a documentation example assumes one.
Do not use the Compose `axon` service as a supported production deployment.

`just dev` calls `just stop`, builds the debug binary, starts configured
TEI/Chrome containers, and runs `axon mcp`. It is not equivalent to starting
a non-disruptive HTTP service. `axon serve` is the unified HTTP entry point.

## Validation checklist

Verify the actual executable/version and effective storage/provider paths
after restart. Check process health, provider diagnostics, worker liveness,
and an initialized MCP or REST client separately. `/healthz` alone does not
prove embeddings, browser rendering, or queued execution.

Start with read-only status/doctor checks on the intended runtime. The local
CLI runs in-process; it does not forward arbitrary commands to a remote
`axon serve`. Use that server's HTTP/MCP surface for remote checks.

For an authorized isolated source smoke test:

```bash
axon https://example.com --scope page --wait true
axon jobs get <job_id> --json
axon jobs events <job_id> --json
```

Use an owned test collection/input scope and retain the actual returned job
ID. Confirm terminal status, committed generation, provenance, and retrieval.
Include refresh/removal behavior when changing storage or publication. A
returned job ID or a newly written vector is not complete lifecycle evidence.

Check invalid-input and authorization failures after transport changes.
Reconnect MCP clients, compare discovery with the selected projection, and
make a safe call through the intended gateway. Report unavailable providers
and skipped checks explicitly.

## Rollback procedure

Determine whether the change affected only code/configuration or also durable
state. A previous binary is not necessarily compatible with a migrated
database. Preserve failure logs, job IDs, plans, receipts, and the current
state before rollback.

For code-only rollback, restore the recorded compatible binary/configuration
and restart the verified native service. For a stateful recovery, quiesce all
writers and restore a consistent SQLite/artifact/vector recovery point.
Do not mix unrelated generations or use `docker compose down` as a generic
Axon rollback.

Reset is destructive, not an upgrade or backup step. Use its
[reviewed-plan workflow](../reference/operations/reset.md) only for that
explicitly authorized purpose. On partial failure, inspect its receipt and
recovery guidance rather than repeating an unknown write or deleting files.

## Upgrade procedure

Read the release/component contract and verify the platform asset before
installation. `axon update` checks release integrity and has installation
and optional container-sync behavior; inspect its reported destination.
For an update that must not invoke the legacy container-sync path:

```bash
axon update --no-container
```

This does not automatically replace a systemd service binary at a different
path. The destination can be explicitly overridden by
`AXON_UPDATE_INSTALL_PATH`; select it only after verifying the service and
required permissions. Do not claim that updating a PATH copy updated
`/usr/local/bin/axon` or restarted the live service.

Record the selected release, checksum/signature verification, install path,
restart, and observed runtime version. Review migrations, renamed/removed
configuration keys, and generated contract changes. Do not rewrite applied
SQLite migrations or replace the service environment with a fresh template.

## Web panel and source map

The bundled panel lives under `apps/web/`; HTTP runtime routes live under
`crates/axon-web/`, not the removed root `src/web/` tree. REST route/DTO
contracts are generated in [the HTTP reference](../reference/http-api.md)
and [OpenAPI registry](../reference/rest/openapi.json). The MCP catalog also
includes its dashboard, resources, and task behavior.

Use [operations](operations.md), [jobs](../reference/runtime/jobs.md),
[observability](../reference/runtime/observability.md), and
[the repository map](../architecture/repo-structure.md) for focused guidance.
