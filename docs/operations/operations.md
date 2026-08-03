---
title: "Operations Runbook"
updated: 2026-08-02
---

# Operations Runbook

This runbook covers the current Axon runtime: one native server process, one
SQLite durable-job database, one unified source pipeline, and configured
Qdrant/embedding/Chrome providers.

## Start and inspect the service

Native systemd:

```bash
sudo systemctl status axon
sudo systemctl restart axon
journalctl -u axon -f
```

Development shell:

```bash
axon serve
```

Incus infrastructure and optional guest-server commands are documented in
[deploy/incus/README.md](../../deploy/incus/README.md).

## Health checks

```bash
axon doctor
curl -fsS http://127.0.0.1:8001/healthz
curl -fsS http://127.0.0.1:8001/readyz
curl -fsS http://127.0.0.1:8001/metrics
```

Use `healthz` for process liveness and `readyz` for dependency readiness.
Use `axon doctor` when a provider, database, auth, or configuration problem
needs an actionable diagnosis.

## Submit work

All indexable sources enter through the unified source pipeline:

```bash
axon source https://docs.example.com --scope site --wait true
axon source /home/user/project --wait true
axon source https://github.com/owner/repo --wait true
axon source https://www.youtube.com/watch?v=ID --wait true
axon source session:claude:/path/to/session.jsonl --wait true
```

Use `axon scrape <url>` for the retained one-page projection and `axon map
<url>` for URL discovery without indexing a full site.

Without `--wait true`, source work is detached and returns a durable job
descriptor.

## Durable jobs

```bash
axon jobs list
axon jobs get <job-id>
axon jobs events <job-id>
axon jobs stream
axon jobs cancel <job-id>
axon jobs retry <job-id>
axon jobs recover
axon jobs cleanup
axon jobs clear
axon jobs worker
```

There are no crawl/embed/ingest job stores. Source, extract, watch, map,
research, ask, query, retrieve, memory, graph, prune, probe, and reset work use
the same durable lifecycle. See [Runtime Jobs](../reference/runtime/jobs.md).

`jobs cleanup` and `jobs clear` remove terminal rows only. Active work must
be canceled or recovered first.

## Watches

```bash
axon watch create https://docs.example.com --every-seconds 3600
axon watch list
axon watch get <watch-id>
axon watch exec <watch-id>
axon watch pause <watch-id>
axon watch resume <watch-id>
axon watch history <watch-id>
axon watch delete <watch-id>
```

Watch executions create normal durable jobs and are observable through the
same jobs surface.

## Logs and diagnostics

- Native service logs: `journalctl -u axon`
- Structured Axon files: under the configured `AXON_DATA_DIR`/logging paths
- Provider logs: inspect the owning Qdrant, TEI, or Chrome service/container
- Job evidence: `axon jobs get` and `axon jobs events`
- Runtime diagnosis: `axon doctor`

Do not infer health from a process alone; a responsive server can still have an
unready Qdrant or embedding provider.

## SQLite backup and restore

The default SQLite path is `~/.axon/jobs.db`; `AXON_SQLITE_PATH` may
override it.

For an online backup, use SQLite's backup command rather than copying the main
file while WAL writes are active:

```bash
sqlite3 "${AXON_SQLITE_PATH:-$HOME/.axon/jobs.db}"   ".backup '$HOME/.axon/backups/jobs-$(date +%Y%m%d-%H%M%S).db'"
```

For a cold backup, stop the single process that owns the database, then copy the
database and any associated WAL/SHM files together. Never let host and Incus
guest Axon processes open the same SQLite database concurrently.

Restore only while the owning Axon process is stopped, then run `axon doctor`
and inspect the jobs list before accepting new work.

## Qdrant backup and restore

Use Qdrant's snapshot API or the operational tooling for the configured Qdrant
instance. A Qdrant snapshot and SQLite backup serve different purposes:

- SQLite preserves jobs, source ledger state, graph/memory metadata, and runtime
  control data.
- Qdrant preserves vector collections and searchable payloads.

Coordinate both when creating a disaster-recovery checkpoint.

## Safe shutdown

1. Stop submitting new work.
2. Inspect `axon jobs list` and wait for important jobs to reach terminal
   state, or cancel them deliberately.
3. Stop the one Axon process that owns SQLite.
4. Stop providers only after Axon has stopped using them.

## Common failure patterns

| Symptom | Check | Action |
|---|---|---|
| `readyz` fails | `axon doctor` | Repair the reported provider/config dependency |
| Job remains running after a crash | `jobs get`, events | Restart workers and run `jobs recover` after the stale threshold |
| Provider saturation | job events/cooling fields | Let cooldown expire or reduce concurrency |
| Chrome acquisition fails | Chrome endpoint and provider logs | Repair Chrome/CDP, or use HTTP rendering when appropriate |
| No vectors produced | source result and embedding provider | Confirm embedding is enabled and provider readiness is green |
| Unauthorized REST/MCP call | auth mode and scopes | Fix token/OAuth configuration; do not bypass authorization |

## Related documentation

- [Deployment](deployment.md)
- [Configuration](../guides/configuration.md)
- [Runtime Jobs](../reference/runtime/jobs.md)
- [Backup and restore](../reference/operations/backup-restore.md)
- [Troubleshooting](../reference/operations/troubleshooting.md)
