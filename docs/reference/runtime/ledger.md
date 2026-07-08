# Ledger Runtime Reference

`axon-ledger` is the SQLite-backed system of record for source accounting: it
answers "what sources exist, what is in each generation, and what is safe to
search." It is a live, tested crate (`crates/axon-ledger/`), not a design
document — this page describes the actual `SqliteLedgerStore` implementation.

See also: crate guide `crates/axon-ledger/src/CLAUDE.md`, behavior contract
`docs/pipeline-unification/runtime/ledger-contract.md`, crate contract
`docs/pipeline-unification/crates/axon-ledger/README.md`.

## What it owns

`LedgerStore` (`crates/axon-ledger/src/store.rs`) is the trait every caller
depends on; `SqliteLedgerStore` (`crates/axon-ledger/src/sqlite.rs`) is the
only concrete implementation. It owns five kinds of durable state:

- **Sources** — one row per indexable source (`sources` table), keyed by
  `source_id`, with a `committed_generation` pointer and a JSON `SourceSummary`
  payload. `canonical_uri` is unique (enforced via a `json_extract` index), so
  the same origin can't register twice under different IDs.
- **Generations** — `source_generations`, one row per attempt to re-index a
  source. Generations are sequential per source (`sequence` column, unique per
  `source_id`) and carry `status` (`LifecycleStatus`) and `publish_state`
  (`PublishState`).
- **Manifests and items** — `source_manifests` (one JSON manifest per
  generation) plus `source_items` (one row per manifest item, keyed by
  `(source_id, generation, source_item_key)`), used both to serve
  `get_manifest` and as the input to manifest diffing.
- **Document status** — `document_status`, one row per document
  (`document_id`), tracking `DocumentLifecycleStatus` (prepared / embedded /
  published / cleaned, defined in `axon-api`), optional `SourceError`, and an
  optional `cleanup_status`.
- **Cleanup debt and leases** — `cleanup_debt` (durable, idempotent record of
  work `axon-prune` must still do) and `leases` (short-lived ownership locks
  for refresh/watch coordination).

Everything the store persists is a JSON-serialized `axon-api` DTO
(`SourceSummary`, `SourceGeneration`, `SourceManifest`, `DocumentStatus`,
`CleanupDebt`, `LeaseGuard`, …) stored alongside a handful of indexed scalar
columns used for lookups and constraints. `axon-ledger` does not redefine
these wire shapes — it stores and returns them.

## Deployment model

`SqliteLedgerStore` is normally constructed via `from_pool`, binding to the
**same SQLite pool** that backs the job runtime (`axon-jobs`/`JobStore`). Per
the storage contract, the runtime intentionally uses one database so that
`jobs.source_id` can foreign-key to `sources(source_id)`; the composed
cross-crate migration runner in `axon-jobs` applies `axon-ledger`'s migration
set (`crates/axon-ledger/src/migrations/0001_ledger_lifecycle.sql`) against
that shared pool. `SqliteLedgerStore::connect`/`::in_memory` are standalone
constructors for tests and tooling only — they run migrations themselves and
should not be used against the shared runtime database.

## Source generation lifecycle

A generation is the unit of "one attempt to re-index a source." The lifecycle
is:

1. **`create_generation(source_id)`** — allocates the next `source_generations`
   row for a source. Requires the source to already exist. The new
   generation's `sequence` is `MAX(sequence)+1` for that source (enforced
   unique), `status = Running`, `publish_state = Writing`, and
   `previous_generation` is captured from whatever generation is currently
   committed (`sources.committed_generation`) at creation time — this becomes
   the generation's declared baseline.
2. **`put_manifest(manifest)`** — writes the `SourceManifest` and its
   `SourceItem` rows for that generation. This must happen before the
   generation can be completed or published (both check for manifest
   existence and fail with `source.ledger.manifest_missing_error` otherwise).
3. **`diff_manifest(manifest)`** — reads the **currently committed**
   generation's manifest for the same source (via `committed_generation` +
   `get_manifest`) and classifies every item in the new manifest as
   added/changed/removed/unchanged relative to it. This is how the ledger
   answers "what changed since last time" without re-scanning the source.
4. **`complete_generation(generation)`** — transitions a generation once
   acquisition/prepare/embed work is done. Validates the generation is still
   writable and that its stored `previous_generation` still matches what was
   passed in (`source.ledger.generation_baseline_changed` otherwise — this
   guards against two concurrent generations racing on the same baseline).
5. **`publish_generation(request)`** — the commit point. Re-validates the
   manifest exists and that `request.expected_previous_generation` still
   matches both the generation's own baseline and the source's actual
   committed generation (same race guard as above, checked again at the
   narrower publish boundary). On success it stamps `published_at`, flips
   `publish_state` toward `Committed`, and (via
   `stale_cleanup::stale_item_cleanup_debt_in_tx`) diffs the newly published
   manifest against the previous one to enqueue cleanup debt for anything that
   disappeared.
   On success, `publish_state` becomes `Committed` if no stale-item cleanup
   debt was generated, or `CleanupPending` if it was — either way the
   generation is now the source's `committed_generation` and visible to
   readers.
6. **`fail_generation(generation)`** — marks a generation failed without ever
   making it visible as committed.

**Invariant:** generation commit/publish is transactional (each step runs
inside one SQLite transaction), and a partially-written or failed generation
is never exposed as the source's `committed_generation`. Readers only ever see
a generation after `publish_generation` succeeds.

## Manifest diff

`diff_manifest` compares the incoming manifest's items against the previously
committed generation's items by `source_item_key`, using `content_hash`
(falling back to `version`/`mtime` where a source type has no content hash) to
decide changed vs. unchanged. The result is a `SourceManifestDiff` with
added/changed/removed/unchanged buckets — this is the artifact the
family-bridge acquisition path uses to decide which items actually need
prepare/embed work, and it's also the mechanism that drives publish-time stale
cleanup debt.

## Document status

`update_document_status(status)` upserts one row per `document_id` recording
where a specific document is in its own lifecycle
(`DocumentLifecycleStatus`: prepared → embedded → published → cleaned, plus
error/cleanup-status fields). This is a finer-grained parallel track to the
generation-level `document_counts` aggregate stored on `SourceGeneration` —
generations answer "how much of this run succeeded," document status answers
"where is this one document right now."

## Cleanup debt

`CleanupDebt` (`cleanup_debt` table) is how the ledger tells `axon-prune` what
still needs to be deleted after a generation supersedes another one — it is
**recorded** here and **executed** elsewhere; `axon-ledger` never touches
Qdrant, the filesystem, or any other store directly (enforced by
`cargo xtask check-layering`: forbidden dependencies include Qdrant/TEI/LLM/
provider clients).

- `kind` is one of `VectorDelete`, `ArtifactDelete`, `LedgerPrune`,
  `GraphPrune`, `MemoryPrune`, `JobRetention`, `CachePrune` — each names a
  different downstream system that owns execution.
- `selector` scopes the debt to a `Source`, a `Generation`, or a specific
  `SourceItem`, and must be internally consistent with the debt's own
  `source_id`/`generation` (`validate_cleanup_debt` rejects mismatches before
  insert).
- Debt rows are deduplicated by `(source_id, generation_key, kind,
  selector_hash)`. `record_cleanup_debt` (public API) upserts and refreshes an
  existing pending row if the incoming debt is newer; the internal
  `insert_cleanup_debt_once_in_tx` helper (used by publish-time stale cleanup)
  instead does nothing on conflict, so a generation's own auto-derived cleanup
  debt is never clobbered by a subsequent identical insert.
- **Idempotent by design.** `resolve_cleanup_debt(debt_id)` is a no-op on an
  unknown or already-resolved (`completed_at` set) debt id — re-running a
  cleanup pass is always safe. `list_pending_cleanup_debt(source_id)` returns
  only unresolved rows (`completed_at IS NULL`), oldest first, which is what
  drivers like the publish-time cleanup drain in
  `axon-services::source::index_source_with_auth` poll after a generation
  publishes.

## Leases

`leases` back short-lived ownership coordination for refresh/watch work —
`acquire_lease`/`heartbeat_lease`/`release_lease`, keyed by an arbitrary
caller-chosen `lease_key`. `acquire_lease` is safe under contention: if an
unexpired lease already exists for the key and is owned by someone else, the
call returns `Ok(None)` (not an error) rather than blocking; if the existing
lease has expired, it is deleted and replaced. Leases carry an `expires_at`
computed from the caller's `ttl_seconds`, so a crashed or stalled owner's
lease is automatically reclaimable once it lapses — there are no permanent
locks.

## Error shape

Ledger-specific failures use `axon-api`'s `ApiError` with a stable `code`
namespace, e.g. `source.ledger.source_missing`,
`source.ledger.manifest_missing` (generation completion/publish without a
manifest), `source.ledger.generation_baseline_changed` (concurrent generation
race — raised both when completing a generation against a stale baseline and
when the committed generation moved during publish), `source.ledger.
committed_manifest_missing` (diff against a committed generation whose
manifest disappeared — an internal-consistency failure),
`source.ledger.manifest_item_source_mismatch` /
`source.ledger.manifest_duplicate_item` (manifest validation),
`source.ledger.generation_missing` (cleanup debt referencing a nonexistent
generation), `source.ledger.generation_not_publishable` /
`source.ledger.generation_already_published` (generation state guards),
and `source.ledger.lease_missing` / `source.ledger.lease_owner_mismatch`
(lease heartbeat/release against the wrong owner or a gone lease).

## Testing

`crates/axon-ledger/src/testing.rs` exposes `FakeLedgerStore` (an in-memory
implementation of `LedgerStore`) plus SQLite temp-db fixtures, so callers in
`axon-services`/`axon-jobs` can exercise ledger-dependent logic without a real
database. `crates/axon-ledger/src/sqlite_tests.rs` and
`crates/axon-ledger/src/store_tests.rs` cover the SQLite implementation and
the store-contract behavior (generation races, cleanup-debt idempotency, lease
expiry) directly. Run with:

```bash
cargo test -p axon-ledger
```
