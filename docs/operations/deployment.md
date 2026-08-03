---
title: "Deployment"
created: 2026-02-25
updated: 2026-08-02
---

# Deployment

Axon runs as a native Rust binary. Qdrant, TEI, and Chrome may run locally,
inside the Incus infrastructure container, or at explicitly configured remote
endpoints.

## Supported topologies

### Native systemd service

The standard service unit runs:

```text
/usr/local/bin/axon serve
```

It binds `127.0.0.1:8001` by default and owns the configured SQLite database.
Install and operate it using [deploy/systemd/README.md](../../deploy/systemd/README.md).

### Incus infrastructure container

[deploy/incus/bootstrap.sh](../../deploy/incus/bootstrap.sh) provisions the
`axon-container-profile` system container and the nested Qdrant, TEI, and
Chrome services. It also installs a validated Axon binary and unit in the guest,
but leaves `axon-native.service` disabled by default when a host Axon process
owns the shared SQLite queue.

Set `AXON_INCUS_RUN_SERVER=true` only when the guest uses an Incus-exclusive
SQLite database that no host process opens. See
[deploy/incus/README.md](../../deploy/incus/README.md) for GPU, storage, proxy,
and guest-base requirements.

### Container images

Release images are published for users who choose containerized execution, and
`docker-compose.prod.yaml` remains the canonical infrastructure image/port
reference. The maintained deployment walkthroughs are native systemd and Incus.

## Configuration

Axon reads:

- `~/.axon/.env` for endpoints, credentials, auth, and process bootstrap
- `~/.axon/config.toml` for non-secret runtime tuning

The defaults and ownership rules are documented in
[Configuration](../guides/configuration.md). Never put secrets in
`config.toml`.

Minimum server configuration normally includes reachable Qdrant and embedding
provider endpoints. Chrome is optional unless rendered acquisition, screenshots,
or browser automation are required.

## Native installation

Build or download the release binary, then install the service:

```bash
cargo build --release --locked --bin axon
sudo install -m 0755 target/release/axon /usr/local/bin/axon
sudo install -m 0644 deploy/systemd/axon.service /etc/systemd/system/axon.service
sudo systemctl daemon-reload
sudo systemctl enable --now axon
```

Use the exact user, paths, and environment layout in the systemd deployment
README rather than copying a unit blindly between hosts.

## Incus provisioning

Run the repository bootstrap on the Incus host:

```bash
deploy/incus/bootstrap.sh
```

The script validates the supported guest base, profile, storage, nested Docker,
GPU/CDI state, service configuration, and binary installation. Review the
Incus README before overriding its defaults.

## Validation

After deployment:

```bash
axon doctor
curl -fsS http://127.0.0.1:8001/healthz
curl -fsS http://127.0.0.1:8001/readyz
axon source https://example.com --scope page --wait true
axon jobs list
```

`/healthz` proves the HTTP process is responsive. `/readyz` includes
runtime dependency readiness. `axon doctor` provides the richer provider and
configuration diagnosis.

## Upgrades

1. Back up the configured SQLite database and any required Qdrant snapshots.
2. Install the new binary atomically.
3. Restart the owning Axon service.
4. Run `axon doctor`, `/readyz`, and a bounded source smoke test.
5. Confirm `axon jobs list` shows no unexpected failed or stranded work.

For the Incus topology, rerun `deploy/incus/bootstrap.sh`; it validates and
refreshes the deployed binary and infrastructure configuration.

## Rollback

Keep the previously validated binary until the new release passes smoke tests.
Rollback by restoring that binary and restarting the service. Restore SQLite or
Qdrant only when the upgrade changed durable data incompatibly; do not copy a
live SQLite database without using a SQLite-safe backup method.

## Related documentation

- [Operations runbook](operations.md)
- [Security](security.md)
- [Performance](performance.md)
- [Configuration](../guides/configuration.md)
- [REST overview](../reference/rest/overview.md)
