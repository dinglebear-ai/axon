---
title: "Re-indexing and Vector Payload Cutover"
created: 2026-05-21
updated: 2026-08-02
---

# Re-indexing and Vector Payload Cutover

Axon's live vector payload contract is identified by the required string field:

```text
payload_contract_version = "2026-07-01"
```

The authoritative generated shape is
[`docs/reference/sources/vector-payload.md`](../reference/sources/vector-payload.md)
and its adjacent JSON Schema. The hand-written operational contract is
[`docs/reference/qdrant-payload-schema.md`](../reference/qdrant-payload-schema.md).

This is a clean-break contract, not an incremental numeric schema series.
Current points must carry the dated version plus the required source,
generation, document, chunk, visibility, redaction, embedding, and locator
lineage. Retired numeric schema markers are not compatibility signals for the
current runtime.

## When Re-indexing Is Required

Re-index or reset when a collection contains any point that:

- lacks `payload_contract_version`;
- carries a value other than `2026-07-01`;
- lacks current required lineage such as `source_id`, `source_item_key`,
  `source_generation`, `document_id`, `chunk_id`, `chunk_locator`, or
  `source_range`;
- was written into an unnamed/dense-only collection that cannot satisfy the
  current named `dense` plus sparse `bm42` retrieval shape;
- uses retired payload keys that normal query, retrieve, source, delete, and
  prune paths no longer read.

Do not assume older points remain semantically searchable. Current retrieval
fails closed on missing citation/redaction lineage, and hybrid search requires
the sparse namespace declared by the collection.

## Inventory Before Destruction

Record the canonical source inputs you need to replay before resetting. A
collection cannot reconstruct every original `SourceRequest` from vector URLs
alone. Keep the explicit web roots, repository URLs, local paths, session
selectors, registry targets, and watch definitions that produced the corpus.

Useful read-only inventory commands include:

```bash
axon sources --json
axon watch list --json
axon jobs list --json
axon stats --json
```

`axon sources` is a current vector URL/chunk inventory, not a replay manifest.
Use it to cross-check coverage, not as the sole backup of source intent.

### Inspect the vector contract with reset planning

`reset` is a dry-run by default. Planning a vector reset scans the configured
collection and reports every distinct `payload_contract_version` in the vector
store plan row:

```bash
axon reset --stores vectors --json
```

Review:

- the configured Qdrant URL and collection in `plan[].location`;
- the exact point count;
- the `payload contracts:` values in the vectors row;
- blockers, warnings, plan expiry, and receipt target.

Do not execute this vectors-only plan as a general re-index recipe. Retaining a
ledger that says unchanged items are already committed can prevent a pure
vector reset from replaying all unchanged material. The supported clean-break
replay is an all-store reset followed by re-sourcing.

## Supported Clean-Break Procedure

### 1. Stop active workers and servers

Destructive reset acquires the worker drain lock and refuses to replace SQLite
while an Axon worker or server is active. Stop those processes before
execution. Qdrant, the embedding provider, and any acquisition dependencies
must remain reachable.

### 2. Create and review an all-store plan

With no `--stores` list, reset selects all logical stores: unified SQLite
state, vectors, and artifacts.

```bash
axon reset --json
```

This command is a dry-run. It inventories the live targets, writes a saved plan,
and prints a `plan_id`. Review every path, collection, count, blocker, warning,
and estimate. The plan is bound to its store scope, configuration snapshot,
inventory checksum, and expiry.

### 3. Execute that exact reviewed plan

```bash
axon reset --yes --plan-id reset_plan_REVIEWED_ID --json
```

Execution is destructive. It requires local/admin trust, the saved plan id, an
unchanged plan scope/configuration, and an available worker drain lock. The
all-store reset:

- wipes and re-migrates the unified SQLite database;
- drops and recreates the configured Qdrant collection with named `dense` and
  sparse `bm42` vectors;
- removes selected artifacts;
- writes a durable reset receipt.

Keep the receipt. It is the evidence for what was deleted and recreated.

### 4. Re-source canonical inputs

Replay each source through the unified source pipeline. Use `--wait true` when
you want the command to host workers and return the final result immediately.

```bash
# Documentation site
axon source https://docs.example.com --scope site --wait true --json

# One page
axon source https://example.com/guide --scope page --wait true --json

# Git repository
axon source https://github.com/owner/repo --scope repo --wait true --json

# Local checkout
axon source /home/user/project --wait true --json

# Session exports
axon sessions --wait true --json

# Other classified inputs use the same entry point
axon source r/rust --scope subreddit --wait true --json
axon source https://www.youtube.com/watch?v=VIDEO_ID --scope video --wait true --json
axon source pkg:crates/serde --wait true --json
```

For detached replay, omit `--wait true`, run a worker in a long-lived process,
and monitor the unified job surface:

```bash
axon source https://docs.example.com --scope site
axon jobs list --active --json
axon jobs get JOB_ID --json
```

Do not treat enqueue success as indexing success. The final source result or
job record must reach a successful terminal state.

### 5. Verify the rebuilt corpus

```bash
axon doctor
axon stats --json
axon sources --json
axon query "known corpus phrase" --json
axon ask "question with a known indexed answer" --json
```

Then run another vector reset **plan only** to perform a collection-wide
contract scan without deleting the rebuilt store:

```bash
axon reset --stores vectors --json
```

The vectors plan row should list only `2026-07-01`. Verify that representative
query and ask results carry canonical source/document/chunk citation lineage,
not merely that the point count is non-zero.

## Targeted Source Refresh

A normal refresh does not require a collection reset when the collection is
already entirely on the current contract. Re-run the same canonical source:

```bash
axon source https://github.com/owner/repo --scope repo --wait true --json
```

The unified pipeline creates a new ledger generation, publishes its vector
points, advances the committed generation, and records cleanup debt for stale
state. A source job keeps one job id through acquisition, preparation,
embedding, publish, graph work, and cleanup.

Capture `source_id` and `committed_generation` from successful source results
when you need to audit or manually clean an older generation.

## Explicit Cleanup: Plan, Then Execute

`prune` is the only current scoped cleanup command. It is plan-first and uses a
saved plan id for execution.

### Remove an old generation

```bash
axon prune plan SOURCE_ID --generation OLD_GENERATION_ID --json
axon prune exec PRUNE_PLAN_ID --confirm --json
```

Read `plan.job_id` from the plan response and pass that value to `prune exec`.
Do not pass the source id to `exec`. Execution requires `--confirm` and admin
trust. The generation fence refuses to delete the currently committed
generation.

### Remove a source's vector points

```bash
axon prune plan SOURCE_ID --json
axon prune exec PRUNE_PLAN_ID --confirm --json
```

This removes the source's vector points. The current committed ledger identity
is retained so generation fencing and a later refresh still have an ownership
record. Use this only for an explicit vector cleanup, not as the normal refresh
sequence.

### Remove all points in a collection

```bash
axon prune plan collection:axon --json
axon prune exec PRUNE_PLAN_ID --confirm --json
```

Collection prune scrolls and deletes points in bounded batches. It does not
drop or recreate the collection schema, so it is not a substitute for the
clean-break reset procedure.

Standalone purge and dedupe command families are not part of the current CLI.
All supported operator cleanup uses a reviewed `prune plan` followed by
saved-plan `prune exec`.

See [Pruning](../reference/runtime/pruning.md) for selector, safety, execution
order, and receipt details.

## What `axon migrate` Does Not Do

`axon migrate --from OLD --to NEW` converts an unnamed-vector collection to a
named dense+sparse collection and computes BM42 vectors from stored chunk text.
It does not reacquire sources, rerun document preparation, recreate missing
lineage, or rewrite retired payloads into the `2026-07-01` contract.

Use migration for vector-mode conversion only when the existing payloads
already satisfy the current contract. Use reset plus re-source for a payload
contract cutover.

## Operational Checklist

```text
[ ] Preserve the canonical source inputs and watch definitions to replay
[ ] Confirm QDRANT_URL, TEI_URL, collection, SQLite path, and artifact root
[ ] Stop active Axon workers/servers
[ ] Run `axon reset --json` and review every target/count/blocker
[ ] Execute only the returned plan id with `--yes --plan-id`
[ ] Keep the reset receipt
[ ] Re-source every intended input and verify successful terminal results
[ ] Run doctor, stats, sources, representative query, and representative ask
[ ] Run a vector reset plan and confirm only contract 2026-07-01 is present
[ ] Use prune plan + saved-plan exec only for explicit stale-state cleanup
```
