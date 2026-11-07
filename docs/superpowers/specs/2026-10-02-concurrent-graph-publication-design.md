# Concurrent graph construction and source publication

## Objective

Move expensive graph construction into the prepare/embed interval. Keep staged
graph output invisible until the source generation commits. Preserve shared
identities, authority resolution, provenance, cancellation, and durable recovery.

Owner: bead `axon_rust-0t0a2.6`. This is a design, not an implementation claim.

## Observed baseline

Main `215a40468248cafcac6eb21970b8b470fa45fdea` completed a forced Labby refresh
in 404.986 seconds: approximately 259.810 seconds before publication, 35.984
seconds publishing, and 108.817 seconds writing the graph. It published 2,243
documents and 35,046 points. Physical cleanup now runs independently with durable
retries. Those provider timings are observations, not future speed guarantees.

Graph extraction already overlaps embedding. `source.rs::finalize_source_index`
currently writes graph candidates after dispatch publishes the generation.
`axon-graph` merges shared node and edge identities; modifying an existing row
before publication would change visible properties even if new evidence were
filtered out. A visibility flag on new rows alone therefore is insufficient.

## Alternatives

1. **Isolated construction with optimistic bulk activation (selected).** Run the
   existing graph merge implementation against a private durable SQLite staging
   store. Seed only affected existing identities and retain their original row
   images. At publication verify those images under the live writer transaction,
   then bulk apply the staged changes. Reconcile changed identities through the
   existing merge rules before activation. Read paths remain on the existing
   graph tables.
2. **Versioned graph claims and visibility-aware queries.** Persist every claim
   with its generation and activate a pointer. This makes activation cheap but
   changes every graph read and requires read-time reconciliation of shared
   identities. It is a larger retrieval redesign with query-latency risk.
3. **Raw candidate outbox with post-publication replay.** Simple and recoverable,
   but leaves the expensive graph merge on the completion path. It does not meet
   this request.

## Ownership and storage

`axon-graph` owns stage creation, row-image capture, candidate replay, activation,
and stage disposal. `axon-services` owns bounded concurrent scheduling and the
cross-domain publication transaction. `axon-ledger` owns generation validation,
compare-and-swap, statuses, and cleanup debt. Transports remain thin shims.

Use an opaque stage ID tied to source ID, generation, job ID, and attempt.
Store the private database beneath the configured data root with restrictive
permissions; never accept a caller-supplied database path. Register ownership
and disposal durably before any staging write. Do not use the Unraid share path
for production SQLite files: deployment uses the direct cache mount.

The private writer is serial and uses one SQLite connection with a bounded
64 MiB page cache. DELETE journaling and FULL synchronization remain enabled.

The private store uses the canonical graph schema and existing merge code. A
bounded candidate journal preserves the exact ordered claims needed to rebuild
when a concurrently published source changes a shared identity. Candidate bytes
remain charged against the existing generation side-effect budget; disk staging
does not authorize an unlimited journal. Seeded rows and their baseline images
also have an explicit bounded byte charge. Exceeding it returns an actionable
error and leaves the prior generation visible.

The combined journal and side-effect payload is capped at 256 MiB. Physical
SQLite storage has a separate 1 GiB aggregate cap across the database, WAL, SHM,
and rollback journal, allowing for canonical rows, indexes, and transaction
overhead. These physical bytes must not consume the logical payload allowance.

Seed only identities referenced by incoming candidates, including endpoint nodes,
existing edges, evidence, aliases, and conflicts needed by the merge. Capture
baseline images from one consistent read snapshot per seed group. Record absent
rows explicitly. Never reseed an identity already modified in this stage.
Retain seeded-only rows separately from the stage write set so activation cannot
reinsert unrelated rows or overwrite untouched evidence.

## Concurrent scheduling

After preparation, submit graph candidates to a bounded generation-owned writer
while the embedding scheduler consumes prepared chunks. Both paths remain independently polled while ordered buffers consume earlier
results, so a suspended speculative completion cannot retain the writer needed
by an earlier checkpoint. Both paths use existing
provider reservations, heartbeats, cancellation, and byte admission. Stage writes
use their private writer lane; copying baseline rows reads the live store without
holding its writer lane during embedding network calls.

Do not clone the entire generation graph into another unbounded queue. Backpressure
must bound resident candidates as well as serialized bytes. The generation owns
and joins the writer; dropping or cancelling ingestion aborts it and retains its
durable disposal record. No detached writer may continue mutating an abandoned
stage. All producer and writer failures settle before publication.

Build the baseline container/document skeleton from the final validated manifest
and document dispositions. Skipped items never acquire graph output. Retained
unchanged items preserve existing graph output. Manifest rebuilding and budget
truncation must settle before the stage is marked ready.

## Activation and source commit

A ready stage is immutable and records its graph summary plus a digest of its
journal and write set. Publication holds the existing source finalizer lease.
Qdrant visibility preparation retains its existing ordering and rollback rules.

Expose caller-owned transaction entry points in the ledger and graph crates.
Services begins one `ImmediateTx` using the shared live SQLite writer gate,
validates the generation baseline, and checks every affected live row against
its captured baseline image. Node, edge, evidence, alias, and relevant conflict
changes all participate; timestamps alone are not a sufficient change detector.

When unchanged, insert/update the stage write set using bounded bulk statements
inside that transaction, preserving existing timestamps and merge output. Skip
conflict updates when every mutable column is identical using null-safe
comparisons, avoiding index and revision-trigger writes for unchanged seed rows. When
changed, rebuild affected candidate components against the current rows through
the existing authority and conflict rules. Candidate connectivity defines the
rebuild closure, so an edge cannot be activated with a stale endpoint or alias.
A bounded fallback may rebuild the entire stage when that closure is large;
record fallback counts and elapsed time. Do not silently overwrite other sources.

Ledger commit and graph activation share the same transaction. A reader observes
either the preceding committed graph/source state or the new state. There is no
separate graph-visible commit followed by a ledger commit. Any transaction error
rolls both back and invokes existing vector rollback/debt handling. Keep SQL in
the owning domain crate; do not teach the ledger to write graph tables.

Persist activation receipt and stage-disposal debt in the same transaction.
An ambiguous commit is recovered by reading that receipt and committed generation,
not by applying the stage again blindly. The final graph summary comes from the
activated stage; finalization does not replay graph candidates a second time.

## Failure, cancellation, and recovery

- Failure before source commit: prior graph and source remain visible; staged
  files and journal remain privately owned and eligible for durable disposal.
- Cancellation: settle/abort graph work, prevent publication, and retain cleanup
  ownership even if immediate disposal fails.
- Crash during activation: SQLite transaction atomicity preserves both domains;
  restart checks the activation receipt before resuming or disposing the stage.
- Crash after commit: the receipt prevents duplicate activation; the existing
  cleanup worker resumes physical stage disposal.
- Newer source generation: never activate an older ready stage over it. Validate
  the expected baseline and current source lease inside the commit transaction.
- Graph staging failure: fail before publication with a typed diagnostic rather
  than publishing an incomplete hidden graph or claiming a clean result.

Diagnostics identify stage, source, generation, job/attempt, stage ID, retryability,
known commit state, and recovery action. Paths and candidate content are not
transport-visible diagnostics.

## Verification and delivery

Meaningful tests must prove invisible new and modified shared rows during staging;
atomic activation with ledger commit; idempotent recovery; shared-source authority,
metadata, evidence, and alias preservation; concurrent cleanup and other-source
publication; retained/removed/skipped items; failure and cancellation; durable
stage disposal; and upgrade/reopen from the currently deployed migration prefix.

A scheduler test blocks embedding and observes actual durable staging progress,
then blocks staging and verifies that publication waits. Test queue/byte bounds
and cancellation with active work, rather than only helper return values.

Run affected graph, ledger, jobs, services, differential/security/observability,
and generated-schema gates. Lavra reviews every touched file and all findings
are addressed before publication. Run exact-head CI, merge main, deploy only the
Axon container, verify image revision and readiness, then force the real Labby
refresh and a real web-page ingestion. Report preparation/embedding, stage write,
activation, publication, and total elapsed timings separately, including fallback
and background cleanup timings. Preserve content/count differences when comparing
runs; do not promise a numeric speedup before measuring.
