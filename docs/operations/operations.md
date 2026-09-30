---
title: "Operations Runbook"
created: 2026-02-25
updated: 2026-09-29
---

# Operations Runbook

Last reviewed: 2026-09-29

Operate the verified runtime, not an assumed workstation checkout. Axon uses
native `axon serve` under systemd in Incus or bare-metal Linux with external
providers. SQLite jobs and source state are durable; workers run in-process.
A job descriptor is acceptance, not successful publication.

## Related docs

Use [deployment](deployment.md) for installation and upgrades,
[configuration](../guides/configuration.md) for effective values,
[jobs](../reference/runtime/jobs.md) for execution/recovery, and
[ledger](../reference/runtime/ledger.md) for source generations.
[Security](security.md), [performance](performance.md), and
[observability](../reference/runtime/observability.md) cover their own boundaries.

## Day 0: identify and prepare the target

Record hostname, user, platform, binary path/version, service manager/unit,
effective environment/configuration paths, SQLite and artifact roots, and
provider origins. Do not print credentials while collecting this record.
Verify ownership, permissions, available space, and the intended collection.

The local CLI executes in-process. It does not forward arbitrary commands to
a remote Axon API through `AXON_SERVER_URL`. On an operator workstation, use
the server's real REST/MCP transport for remote state, or explicitly operate
on the runtime host. See [MCP connection](../reference/mcp/connect.md).

Keep unrelated keys and credentials. New templates are examples, not
replacement files. Source access, configured tool execution, auth, and
non-loopback exposure require deliberate authorization.

## Start services

Use the actual installed systemd unit and verified service executable. The
bare-metal example uses `axon.service`; the Incus bootstrap generates
`axon-native.service` and can leave it disabled for a host-owned queue.
Never start a second writer against shared SQLite/WAL files merely because
a guest has a generated service file.

For deliberately selected local development only:

```bash
just services-up
# Foreground unified HTTP runtime with the existing configured providers:
axon serve
```

`just services-up` operates the configured Compose provider stack; it does
not establish a production Axon deployment. `just dev` first invokes
`just stop`, builds, starts TEI/Chrome, then runs `axon mcp`; it is not a
non-disruptive synonym for `axon serve`. Read [deployment](deployment.md)
before choosing either workflow.

## Health checks

On the intended runtime host with its effective configuration:

```bash
axon status --json
axon doctor --json
axon jobs list --json
```

Inspect the actual result and diagnostics. Process health, embedding/browser
readiness, Qdrant availability, worker liveness, and authenticated client
access are separate checks. `/healthz` or a listening port cannot prove all
of them. A provider timeout is not proof that stored source state was lost.

For remote checks, initialize a real MCP client or use documented `/v1` REST
routes with the correct API/OAuth credentials. Panel unlock is separate.
Repeat discovery after a startup-static MCP projection change.

## Submit work

Use the unified source entry point and the scope supported by its adapter:

```bash
axon https://example.com --scope page --wait true
axon https://example.com --scope site --max-pages 25 --wait true
axon sessions --codex --wait true
```

These examples perform writes when run. Select owned test inputs and the
intended collection before smoke tests; session paths resolve on the
executing host and can contain private data. For production submissions,
follow the actual source/access policy and capacity plan.

Without foreground waiting, inspect each returned source job. A single
command can submit more than one selected root. One source job spans its
acquire/prepare/embed/publish/cleanup stages rather than spawning a separate
embedding handoff.

```bash
axon jobs get <job_id> --json
axon jobs events <job_id> --json
```

Keep the same job/source IDs in incident notes. Confirm terminal state,
committed generation, artifact availability, and retrieval provenance before
calling an ingestion complete. Refresh/removal checks must not return old
content from an uncommitted or superseded generation.

## Recover stuck jobs

First determine whether the worker is absent, the provider is slow/cooling,
the job is actively progressing, a lease is stale, or a previous attempt
partially committed. Inspect heartbeat/attempt/stage events and correlated
logs rather than using an unchanged result field as a universal hang timer.

Stale detection uses effective `[jobs]` settings, including
`stale-after-secs` and `stale-grace-secs`; the old fixed 300+60 second table
and claim of an automatic 10-minute kill are not maintained runtime rules.
A live heartbeat does not establish useful progress, and a waiting client
timeout does not cancel a durable job.

Use `axon jobs --help` and the [jobs reference](../reference/runtime/jobs.md)
for recovery/retry selection. Reclaim or retry only the intended eligible
work with its reported prerequisites. Do not submit a duplicate source job
merely because the first response was lost. Unknown publication/commit
status requires inspection, not speculative repetition.

### Cancel a job

```bash
axon jobs cancel <job_id>
```

Cancellation records intent and is observed by workers at safe boundaries.
It is not a guarantee that every network request stops immediately, that the
state instantly becomes canceled, or that prior publication is rolled back.
Inspect events and final status before retrying or removing artifacts.

## Clear and clean up jobs

Job history retention is distinct from source pruning. `jobs cleanup` and
`jobs clear` operate their supported terminal history scope, not an
automatic cancellation of all active work. Review the current CLI/help,
selection, confirmation, and effective retention policy before execution.

The default terminal-job retention window can remove dependent event rows
before a longer failed-event window expires. Preserve incident evidence or
widen both windows deliberately. Never treat retention as a backup policy.
Use reviewed [prune plans](../reference/runtime/pruning.md) for source/derived
cleanup and [reset](../reference/operations/reset.md) only for an authorized
clean-slate operation.

## Backup and restore

Use the [complete backup/restore runbook](../reference/operations/backup-restore.md).
The effective installation can have several SQLite databases, artifacts,
prepared-document caches, secrets/auth state, and Qdrant collections. Copying
only `jobs.db` is not a consistent source recovery point.

A live SQLite WAL can hold **committed** transactions not checkpointed into
the main database. Do not describe WAL files as merely uncommitted data,
copy a live `.db` alone, delete sidecars to repair a backup, or overwrite
an active runtime with a raw copy. Use a supported snapshot method or
quiesce all writers and retain a consistent database/artifact set.

Coordinate Qdrant snapshots with the publication boundary or document a
reindex/reconciliation strategy. Restore into isolation, verify source
identity/generations, artifacts, and representative queries including removed
content, then review pending jobs/watches/cleanup debt before promotion.
Do not assume an old running job should automatically restart from scratch.

## Logs and diagnostics

Use the actual service journal and configured file/event sinks. Correlate
operation/stage, job/source/item IDs, provider, severity, cause, retryability,
and known side effects. Structured command results go to stdout and logs
to stderr; capture them separately when producing evidence.

Provider logs belong to the verified provider host/service. Do not assume
TEI, Qdrant, and Chrome all run in local Docker containers or use a fixed
homelab hostname from an old report. Review connection failures at the exact
hop rather than repeatedly restarting every service.

Keep credentials, session content, raw provider error bodies, and private
paths out of shared logs/screenshots. A degraded result must explain impact
and the actual fallback; a successful empty list must not hide a failed
provider call.

## Performance tuning

Measure a matching build against controlled input and recorded effective
settings. Tune provider admission, batch limits, memory, and query behavior
through their owning configuration, keeping cancellation and visibility
correct. A compile-time Spider feature does not prove an active runtime
optimization. Conditional ETag reuse is currently disabled; record its
warning in warm benchmark evidence.

Use [performance](performance.md),
[pipeline performance boundaries](../guides/pipeline-performance-boundaries.md),
and [ask/RAG](../guides/ask-rag.md). Do not increase request concurrency only
because a provider reports free GPU memory; inspect queueing, output limits,
timeouts, and resource reservations as well.

## Reindex and collection changes

Changing a collection name or embedding model does not migrate source
identities, generations, payload indexes, and existing vector dimensions.
Plan a new compatible collection or a supported reviewed reindex workflow,
retain a rollback point, and explicitly select the affected sources.

Do not rename a live collection and assume first upsert repairs all state.
Keep ledger publication and retrieval visibility consistent; verify source
refresh/removal and filters, not only insertion counts. Model/dimension or
vector-layout changes require corresponding runtime settings and tests.
Prune old state only after validating the new target and approving a plan.

## Common failures

| Observation | Inspect before changing anything | Recovery boundary |
|---|---|---|
| Queued job does not progress | Worker heartbeat, runtime identity, database path, reservations | Start/recover the intended worker, not a duplicate job pipeline |
| Embedding timeout/overload | Provider URL/readiness, model, batch/input limits, retry diagnostics | Correct the named constraint or allow reported cooling; do not blindly widen all timeouts |
| Browser unavailable | CDP endpoint, actual host/service, access boundary | Restore the intended provider; do not expose privileged browser ports publicly |
| Qdrant memory or write failure | Collection/model, service logs, upsert batches, index policy, partial commit evidence | Reduce owned workload or repair provider capacity under a plan; preserve generation consistency |
| HTTP/MCP denied | Caller identity, API/OAuth token, scope, allowed host/origin, negotiated protocol | Fix the specific prerequisite; panel password is not API authorization |
| Wrong/missing content after refresh | Source/job generation, manifest completeness, filters, vector visibility, cleanup debt | Reconcile the affected stage and data boundary; avoid destructive reset as diagnosis |
| Explicit config file not loaded | Startup loader diagnostics, path, permissions, precedence | Repair the intended file/setting without replacing unrelated configuration |

## Safe shutdown

Stop new submissions and recurring scheduling deliberately. Observe active
jobs and choose whether to drain or cancel them, then inspect final states
and cleanup debt. Stop the verified service and any standalone/automatic
workers using their actual lifecycle; stopping a web listener alone may
leave writers alive.

`just stop` and `just services-down` are developer recipes with their own
scope. They are not generic production shutdown or backup commands. Do not
kill all processes with a similar name on a shared host. Preserve job/event
evidence for recovery and verify no unexpected writer remains before copying
or relocating SQLite state.

## Monitoring and alerts

Monitor service/worker liveness, queue age, provider failure/cooling, storage
capacity, publication failures, and unresolved cleanup debt. Use the
installation's configured notification/log stack with redacted, actionable
messages. An alert should identify the failing target and safe next step,
not just say a service is down.

Avoid embedding notification tokens in shell snippets, repository files, or
public diagnostic artifacts. Alert setup is a separate explicit operational
change, not something performed by reading this runbook.

## Source map

The current domain map is [crate ownership](../architecture/crate-ownership.md).
Jobs and source state live in `axon-jobs` and `axon-ledger`; shared
orchestration lives in `axon-services`; transport code lives in `axon-cli`,
`axon-mcp`, and `axon-web`. Owning crate migrations define upgrades. Old
root-level worker/store paths and per-family queues are historical.
