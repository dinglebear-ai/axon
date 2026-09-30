# Backup And Restore

Last reviewed: 2026-09-29

A recoverable Axon backup keeps durable source/job state consistent with its
artifacts and published vectors. Copying only the executable or `config.toml`
is not a data backup. This is an operator workflow, not a built-in
`axon backup` command.

## State to inventory

Before copying anything, record the running Axon version/revision, effective
configuration paths, SQLite paths, artifact/document-cache roots, selected
Qdrant collections and embedding model/dimensions. Use the installation's
verified service/container definitions, not an assumed `~/.axon` layout.
`AXON_DATA_DIR` and explicit storage overrides may place state elsewhere.

| State | Why it matters |
|---|---|
| SQLite runtime databases | Source identity, committed generations, manifests, document status, jobs, watches, graph, memory, cleanup debt, and other owning-crate state |
| Artifacts and prepared documents | Content referenced by durable records; a database-only restore can leave handles unresolved |
| Configuration | Endpoint/model/collection and path settings needed to interpret the restored state |
| Secrets and authorization state | Required access and identity continuity; back up separately with restricted access |
| Qdrant snapshots | Published vectors and payload identity when reindexing is not an acceptable restore strategy |
| Original acquisition inputs | Needed when providers, exports, or local source files cannot be reacquired later |

An installation may have more than one SQLite database or application state
root. Include the stores used by enabled features, not only a filename copied
from an example. See [runtime storage](../runtime/storage.md),
[database schema](../runtime/database-schema.md), and
[configuration](../../guides/configuration.md).

## Make a consistent recovery point

Choose a maintenance window and explicitly authorize any service interruption.
Pause new submissions and recurring source/watch work, then let active jobs
finish or record their cancellation/recovery state. Stop all writers using
the actual installation lifecycle, including standalone or automatically
spawned workers. Stopping the HTTP listener alone may not stop every writer.

Use a supported SQLite backup/snapshot method, or copy a verified quiescent
database set. Do not copy a live `.db` file alone while committed transactions
may still reside in its WAL. Keep database and WAL state consistent; do not
delete sidecars as a backup preparation step. Preserve ownership, permissions,
and artifact relative layout.

Take the corresponding Qdrant snapshot through its supported administrative
interface when preserving vectors. Record the collection, model/dimensions,
source-generation boundary, snapshot identifiers, timestamps, and hashes of
backup artifacts. Independently timed SQLite and vector snapshots may not
represent one coherent publication point; quiesce publishing or document a
reconciliation/reindex strategy.

Keep secret material out of PRs, logs, public object storage, and ordinary
backup manifests. Verify backup readability and completeness before resuming
writes. Do not delete the source copy merely because a copy command returned
success.

## Restore into isolation first

Provision an isolated restore target with the recorded Axon version and
compatible providers. Prevent restored workers, watches, and cleanup debt
from contacting production collections while validation is incomplete.
Restore SQLite and artifact state together, retain opaque source/generation
identities, and either restore compatible vectors or explicitly reindex from
retained acquisition inputs. Do not combine unrelated ledger and Qdrant
generations and label the result a successful restore.

Restore credentials using the approved secret process and check restrictive
permissions. Update only the intended target paths/endpoints; do not replace
an existing configured file with the repository template. Upgrade an older
backup through the owning crate migrations and tested version path. Never
rewrite an applied migration to make the restored schema appear current.

## Validate before promotion

Check the actual service revision and effective storage/provider settings,
then inspect source counts, committed generations, representative job history,
and artifact retrieval. Run read-only queries with source/path/content filters
and verify expected provenance. Include refreshed or removed content in the
sample so stale vectors do not pass an insertion-only check.

Inspect pending jobs, stale leases, watches, reservations, and cleanup debt
before re-enabling workers. A queued job or old heartbeat is not proof that
work should be duplicated. Follow the [job recovery](../runtime/jobs.md) and
[cleanup](../runtime/pruning.md) diagnostics, preserving original job IDs
where the recovery operation requires them.

Record what was restored, what was rebuilt, validation failures, and the
remaining recovery prerequisites. Promote only after isolation checks pass,
and retain the pre-restore snapshot until rollback is no longer needed.

## What reset does not do

[Reset](reset.md) deletes selected state under a reviewed plan; it is not a
backup, consistency repair, migration strategy, or restore prerequisite.
A Qdrant snapshot alone cannot restore Axon's job/source authority, and a
configuration-only fresh start does not preserve prior source history.
