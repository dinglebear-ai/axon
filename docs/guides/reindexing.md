---
title: "Refreshing and Reindexing Sources"
created: 2026-05-21
updated: 2026-09-30
---

# Refreshing and Reindexing Sources

Last reviewed: 2026-09-29

Reindexing is a source lifecycle operation, not a blind replacement of
Qdrant points by URL and chunk number. The ledger owns stable source/item
identity, manifests, committed generations, and cleanup debt. Queries must
respect that authority after refresh and removal.

## Choose the required operation

| Change | Required consideration |
|---|---|
| Source content changed | Reacquire through the existing source identity; stable hashes can skip unchanged items |
| Parser/chunker/preparation changed | Verify its version/fingerprint invalidates stale prepared output; repeating an unchanged request may legitimately skip work |
| Metadata contract changed | Determine which owner emits the new field and whether acquisition/preparation must run again |
| Embedding model/dimensions changed | Plan a compatible collection and model configuration; a renamed collection is not a migration |
| Source removed | Use complete snapshots, explicit tombstones, or reviewed prune plans; do not infer deletion from partial discovery |

The current [vector payload](../reference/sources/vector-payload.md),
[metadata contract](../reference/sources/metadata-payload.md), and
[database schema](../reference/runtime/database-schema.md) own field definitions.
Historical payload version tables describe old producers, not guaranteed
fields for every current adapter. Do not use old GitHub stars/topics fields
as a universal test for whether a modern checkout was indexed correctly.

## Inspect before writing

Verify the runtime version, effective data paths, source identity, selected
collection, model/dimensions, and authorization. Inspect prior source/job
results and the actual reason content is stale. A missing provider, wrong
filter, uncommitted generation, and obsolete chunk profile require different
recovery actions.

Back up a consistent recovery point before collection/layout or durable-state
changes. Use [backup/restore](../reference/operations/backup-restore.md).
Keep reindex tests isolated from production and preserve unrelated settings.

## Refresh through the unified pipeline

Examples below perform source writes on the executing runtime:

```bash
axon https://example.com/docs --scope docs --wait true
axon https://github.com/owner/repository --wait true
axon r/rust --wait true
axon sessions --codex --wait true
```

Use real targets and adapter-supported scopes. The local CLI does not
automatically forward to a remote server; remote callers use the matching
REST/MCP source operations. A command selecting several roots can return
several source jobs.

Each source job resolves/acquires, diffs the ledger, prepares documents,
embeds, publishes, and updates graph/cleanup. Unchanged items can skip
re-embedding. A successful repeated request does not prove a forced rebuild
of every old vector. Check the actual invalidation reason, changed counts,
and generation result rather than inventing an unsupported force flag.

Publication makes the new generation visible before derived graph cleanup.
Failures and cancellation must retain completed-stage/commit evidence and
cleanup debt. Never delete the previous corpus first merely to make a
subsequent insertion appear fresh.

## Source-specific limits

[GitHub checkout ingestion](ingest/github.md) processes repository files.
Issue/PR/release URLs use targeted API acquisition; a repository refresh
does not automatically refresh every issue, PR, release, and wiki page.
Other Git providers share acquisition contracts, not necessarily all
GitHub-specific vertical scopes.

[Reddit](ingest/reddit.md) uses a bounded partial API snapshot. A post absent
from the next listing is not deleted. Sessions and capped local/Git/web
inventories also require correct completeness handling. Interrupted pages
or truncated output must not silently erase unseen items.

## Verify completion and visibility

```bash
axon jobs get <job_id> --json
axon jobs events <job_id> --json
```

Inspect terminal status, document counts, committed generation, provenance,
and representative queries. Include an added item, a changed item, a removed
item where completeness permits removal, and unchanged content. Verify
source/path/content filters and that superseded vectors are not retrieved.

A job ID, successful provider request, or increased vector count is not
complete evidence. Wait timeout and cancellation are distinct. Unknown
commit status requires inspection before repeating a write.

## Collection and schema migrations

Vector-layout migration and source re-acquisition are different workflows.
A migration operating on existing points cannot invent semantic text or
metadata that only a newer parser/acquisition source can supply. Read the
actual `axon migrate --help` and release migration contract before choosing
it instead of source refresh.

Use a planned compatible collection/model boundary for dimension or layout
changes. Preserve source/ledger identity and generation semantics, verify
retrieval and rollback, and only then approve cleanup of superseded state.
Do not directly overwrite generation metadata or assume first upsert repairs
all payload indexes, source records, and dimensions.

## Cleanup and recovery

Use [prune plans](../reference/runtime/pruning.md) for approved source/derived
cleanup. [Reset](../reference/operations/reset.md) is a separate destructive
clean-slate action, not ordinary reindex preparation. Do not remove database
files, WAL sidecars, or live collections to troubleshoot a failed refresh.

Retain job/source IDs, plan/receipt identifiers, diagnostics, and known
side effects. Fix the named provider/configuration/permission constraint or
resume the eligible stage using the supported job recovery operation.
A historical URL-scoped delete recipe is not a substitute for current
ledger visibility and cleanup-debt ownership.
