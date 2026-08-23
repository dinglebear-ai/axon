# Adversarial Pipeline Review — 2026-08-23

## Scope

Adversarial review of the source-pipeline code that landed with the
pipeline-unification and performance work through
`ad842bd396423e632d0f6281e836dea0d5bcd280` (PR #576), on `main` at
`35b743caf`. Four areas were reviewed independently and the significant
claims were then re-verified directly against the code:

1. `axon-jobs` provider scheduler + SQLite embedding cache store
   (writer-admission gate, reservations, trigger-maintained cardinality).
2. `axon-embedding` cache decorator and identity, plus its composition in
   `axon-services`.
3. `axon-document` structural Markdown chunking (`markdown.rs`,
   `markdown/semantics.rs`, `markdown/windowing.rs`, `chunk_router.rs`).
4. `axon-services` executor overlap (acquisition prefetch in
   `created_generation/batches.rs`, embedding/upsert overlap in
   `vectorize/pipeline.rs`).

The review hunted for real defects only (data loss, capacity wedges,
unbounded output, wrong vectors); style was ignored. The Markdown area was
additionally fuzzed empirically (≈3,000 adversarial documents) in a scratch
harness outside the repo. Findings are ordered by severity; every finding
below was verified against the code, not just reported.

## Critical

### C1. Cancelled or renew-failed `call_reserved` permanently leaks granted capacity

`crates/axon-jobs/src/scheduler.rs:228-271`,
`crates/axon-jobs/src/scheduler/reconcile.rs:40-53`,
`crates/axon-jobs/src/scheduler/grant.rs:69-76`.

`ActiveReservationLease` has no `Drop` guard (the only `Drop` impl in the
scheduler is `WaitingReservationGuard`, which covers the queued phase). Two
paths leave a reservation `status='active'` with nonzero `granted_units`
and no owner:

- The `call_reserved` future is dropped after `activate()` succeeds (e.g. a
  caller-side `tokio::time::timeout`/`select!`).
- `lease.renew().await?` at `scheduler.rs:265` propagates a transient
  `SqlxError` (a single `SQLITE_BUSY` from a cross-process CLI writer is
  enough); the pinned operation is dropped mid-flight and no
  `fail()`/`cancel()` is attempted.

Recovery never releases the units: `reconcile()` only sets
`quarantined = 1` and keeps `status='active'`, no other code touches
`quarantined`, and the capacity sum in `grant.rs` counts
`status IN ('granted','active')` with no quarantine or expiry filter. The
job-level cleanup in `unified/control_helpers.rs` terminalizes reservations
only on job cancel/fail/requeue — a job that continues and *succeeds* never
does. With `authority_id` stable per DB path, the leak survives process
restart. With the vector lane's small capacity, two leaked reservations
wedge the domain permanently until manual DB surgery.

Suggested direction: give the active lease a drop/deadline-based release —
either a `Drop` guard that spawns a best-effort `fail()`, or make the
capacity sum exclude quarantined/expired-active rows (and let reconcile
terminalize quarantined rows after a grace period). Also: on renew failure,
attempt `fail()` before returning, and don't convert a provider failure
into a scheduler error when `fail()` itself errors (`scheduler.rs:259`
currently discards the provider root cause via `?`).

## High

### H1. Removal-only recrawl now fails the generation instead of publishing removals

`crates/axon-services/src/source/executor/created_generation/batches.rs:91-93`,
`crates/axon-services/src/source/executor/helpers.rs:116-140,173-178`,
`crates/axon-services/src/source/executor.rs:320`.

`manifest_has_changes` returns true for a diff with only `removed` (or only
`failed`) entries, so the executor proceeds into
`run_created_generation`. But `batch_changed_diff` yields batches from
`added` + `modified` only, so a removal-only diff produces zero batches and
hits the new `anyhow::bail!("created generation has no changed acquisition
batches")`. The generation errors and `fail_generation` runs, so page
deletions are never published and the stale previous generation stays
live — repeatedly, on every recrawl of a shrunk site. The pre-refactor loop
(before the prefetch rewrite, `ec8ef7fa4`) simply fell through to
finalize/publish with no batches. No test covers the removal-only path.

Suggested fix: replace the bail with a fall-through to finalization when
`batch_changed_diff` is empty (and add removal-only and failed-only
regression tests).

### H2. A single stray fence line disables all Markdown size limits

`crates/axon-document/src/markdown/windowing.rs:46-70,278-283`,
`crates/axon-document/src/markdown.rs:210-239`.

In `split_oversized_sections`, a `Fence` span is always emitted as one
chunk with no size check, and `fenced_spans` treats an unterminated fence
as running to end of content; `fence_aware_headings` likewise never closes
an open fence at EOF, so headings after the stray fence are suppressed. A
document containing one ` ``` ` that is never closed (including a literal
` ``` ` inside a 4-space-indented code block, which `opens_fence` misreads
after `trim_start`) turns everything from that line to EOF into a single
arbitrarily large "code" chunk — a 10 MB page becomes one 10 MB chunk,
`max_chars` is a no-op, and downstream embedding rejects the payload so the
content is never indexed. `code.rs` has `split_if_huge` for exactly this
reason; the Markdown fence path has no equivalent.

Suggested fix: apply a hard size backstop (e.g. `split_if_huge`-style
windowing) to fence chunks exceeding `max_chars`, at least for
unterminated fences.

## Medium

### M1. Embedding cache is enabled on an unverified/stale identity → mixed-model vectors

`crates/axon-services/src/context/target_runtime.rs:327-331,380-405`,
`crates/axon-embedding/src/cache.rs:129-140`.

`build_embedding_composition` wires `CachedEmbeddingProvider` purely on
`cfg.embed_cache_enabled`, never checking `identity.verified`. When the
TEI `/info` probe fails, the identity falls back to
`Qwen3-Embedding-0.6B`/1024 with `verified: false`; the identity is also
cached for up to 30 minutes. The cache key's "resolved model" is therefore
whatever was last resolved (or the fallback), and the decorator's per-hit
identity re-validation compares stored rows against that same stale
`self.model`, so it passes. If the model served at `tei_url` changes to
another 1024-dim model within that window, warm hits return up-to-7-day-old
vectors from the old model while misses embed with the new one — mixed
models in one collection, silently. Two different real models can also
share the identical key tuple under the fallback name across restarts.

Suggested fix: only decorate with the cache when `identity.verified` is
true (fail open to the raw provider otherwise).

### M2. Provider heartbeats leak the speculative Embedding phase mid-overlap

`crates/axon-services/src/source/executor/vectorize/pipeline.rs:140-147,253-274`,
`crates/axon-services/src/source/executor/reserved_call.rs` (+
`reserved_call/support.rs`).

The overlap code publishes Embedding to the `ProgressCoordinator` only
after the current Upserting write is accounted, but `call_embedding` builds
a `ProviderCallContext` with `PipelinePhase::Embedding` whose heartbeat is
recorded to the job store the moment the reserved call starts — while the
previous batch's upsert is still in flight. Externally visible job phase
flaps Upserting→Embedding→Upserting on every overlapped step; only the
coordinator snapshots are ordered, which is why existing tests don't catch
it.

### M3. Cancellation mid-run skips failed-generation cleanup

`crates/axon-services/src/runtime/job_runners/source_runner.rs:136-137`,
`crates/axon-services/src/source/executor.rs:358-386`.

Job cancel drops `run_fut` via `tokio::select!`; the vector cleanup and
`fail_generation` marking run only on an `Err` result, never on cancel.
Vectors already upserted for the uncommitted generation stay in Qdrant
(invisible but occupying storage) and the generation row is never marked
failed, with no cleanup debt recorded. `ArtifactCleanupGuard` covers
tracked artifacts on drop, but vectors and the generation row have no
equivalent. Pre-existing, but the overlap work widens the window by keeping
more provider work in flight per await point.

### M4. `html_article` duplicates text on a trailing unclosed tag

`crates/axon-document/src/markdown.rs:128-131,165-167`.

The pre-`<` text is pushed before the `find('>')` check; on `break` the
cursor was never advanced, so the tail push re-emits it. Verified
empirically: `"hello world <truncated-at-end"` renders as
`"hello world hello world <truncated-at-end"`. Truncated scrapes — exactly
what this path ingests — index duplicated content.

### M5. False frontmatter swallows an arbitrarily large document head

`crates/axon-document/src/markdown.rs:192-206`,
`crates/axon-document/src/markdown/windowing.rs:17-25`.

`extract_frontmatter` accepts any document starting `---\n` with a later
`\n---`; a leading thematic break plus another `---` 500 KB later labels
everything between as one `frontmatter` chunk, which
`split_oversized_sections` exempts from all size limits and whose headings
are never sectioned. Related laxness: `find("\n---")` accepts `----` or
`--- junk` as closers, and CRLF documents never match `strip_prefix("---\n")`
so frontmatter extraction silently doesn't run on them.

### M6. Router substring match sends real Markdown to the schema parser

`crates/axon-document/src/chunk_router.rs:417-430`,
`crates/axon-document/src/preparer/chunk_build.rs:98-129`.

`is_api_schema` runs before the `content_kind` match and does
`path.contains("openapi")`/`contains("swagger")` on the full path, so
`docs/openapi-guide.md` routes to `ApiSchema`; the structured parse fails
on prose and the fallback emits the entire document as one chunk, bypassing
Markdown limits.

### M7. Live queued waiters can be expired as "abandoned"; priority aging is unreachable

`crates/axon-jobs/src/scheduler/grant.rs:116-132,174-191`,
`crates/axon-jobs/src/scheduler.rs:21`.

A queued row's `updated_at` is written once at insert and never refreshed
while the waiter polls, and `expire_abandoned_queued_locked` (run inside
every `reserve()`) expires rows older than 30 s — exactly `WAIT_TIMEOUT`,
but measured from before the waiter's own clock starts. Under contention a
still-polling waiter is expired by an unrelated `reserve()` first and sees
`StaleFence` instead of `WaitTimeout`. With `AGING_QUANTUM_SECS` equal to
the abandonment threshold, a queued background request can climb at most
one priority level before expiry, so the four-level aging ladder is dead in
practice.

## Low

- **Writer gate is process-local while the DB is multi-process**
  (`crates/axon-jobs/src/scheduler.rs:98-104`; cache schema comment in
  migration 0007 notes short-lived CLI processes share the DB). A CLI
  writer takes SQLite's write lock directly; the daemon's gate holder then
  parks in the busy handler while holding the gate, stalling all in-process
  writers — including lease renewals, which feeds C1.
- **Successful provider work discarded on `complete()` fence loss**
  (`scheduler.rs:269`): a concurrent job cancel terminalizes the
  reservation, and the completed, paid-for provider result is returned as
  an error.
- **`prune` hard-fails if the cache state singleton row is missing**
  (`crates/axon-jobs/src/embedding_cache_store.rs:284-291` uses
  `fetch_one`): any drift permanently disables cache writes with no
  self-heal.
- **`max_entries` is soft under bursty writes**: a single `put_many` more
  than 512 rows over capacity prunes only 512 (delete budget clamp); the
  excess drains via maintenance.
- **Decorator bypasses `validate_batch` on empty/fully-cached batches**
  (`crates/axon-embedding/src/cache.rs:214-215,378-400`): an empty batch
  returns `Ok(vec![])` instead of `embedding.batch_empty`, and a full hit
  skips blank-text/duplicate-id validation — the provider contract varies
  with cache warmth. Mitigated because production batches come from
  `EmbeddingBatchBuilder::build()`.
- **Miss vectors zipped by position, `chunk_id` ignored**
  (`cache.rs:220-231,404-415`): safe with the current TEI provider (which
  enforces count and order), but any future provider that reorders vectors
  would persistently cache wrong text→vector pairs for 7 days. Cheap
  hardening: match by `chunk_id`.
- **Overlap failure latency** (`batches.rs:26`, `pipeline.rs:23`):
  `tokio::join!` never aborts the sibling, so an early normalize failure
  waits for a full speculative acquisition batch (e.g. a 64-page crawl)
  before surfacing.
- **Fetching progress freezes at the first batch for prefetch adapters**
  (`batches.rs:116`, `progress.rs:304-306`): suppressing the phase
  regression also suppresses the count update, so a 1000-page crawl's
  persisted Fetching snapshot stays at 64/1000.
- **In-batch artifacts registered with the cleanup guard only after batch
  success** (`created_generation.rs:154`, `generation_state.rs:57-59`):
  clean-output/refetch artifacts stored mid-batch are orphaned if the batch
  later fails — the same class the prefetch fix addressed on the
  speculative-acquisition path.
- **Overlapped upsert accounting dropped when the paired embedding fails**
  (`pipeline.rs:48-59`): the checkpointed write's counts never reach the
  failure summary, understating completed work (durability unaffected).
- **Markdown fallback path ignores injected limits**
  (`chunk_build.rs:45-47` dispatches to `plain_text_windows` with
  hardcoded byte/char caps, dropping `markdown_limits`).
- **`first_fence_language` metadata quirks** (`markdown.rs:251-265`):
  ` ````rust ` yields language `` `rust ``, and a ` ```rust ` line inside a
  `~~~` block is reported as the section's fence language.
- **Full-hit cache results report `usage.input_tokens: Some(0)`**
  (`cache.rs:448`) where TEI reports `None`, skewing any aggregation that
  treats `Some` as metered.
- **Cache "fail-open" admits up to ~750 ms of bounded inline stall per
  `embed` call** when SQLite is saturated (250 ms read + two 250 ms
  detachment waits) — outcome-open, not latency-open.

## Verified solid

- **Cache key construction** (`axon-embedding/src/cache.rs:475-509`): every
  field length-prefixed before hashing — no concatenation-ambiguity
  collisions; instruction folding exactly mirrors TEI request behavior;
  keys computed once per call, so store/lookup normalization cannot
  diverge.
- **Cache store SQL**: all bind budgets under both the 900 self-budget and
  SQLite's 999 floor; trigger-maintained `entry_count` is correct for
  upserts and transactional under ROLLBACK; the `(cache_key, created_at)`
  retire tuple plus the `MAX(..., created_at + 1)` upsert bump prevents
  stale retirements from deleting fresh rows; poison defenses are layered
  on both read and write.
- **Lock ordering**: uniformly gate→pool everywhere; no path holds a pool
  connection while acquiring the gate; the gate is held through COMMIT in
  every mutation path. No deadlock surface found.
- **Overlap durability ordering**: no path persists a later batch's
  checkpoint before an earlier batch's vectors are written; generation
  visibility flips atomically with rollback on failure; the prefetch
  result is never silently dropped or double-processed, and on dual
  failure the primary error is preserved with the secondary attached.
- **Markdown windowing core**: `bounded_content_windows` + semantic
  boundaries fuzz-clean over ≈3,000 adversarial documents (fences, tables,
  lists, CRLF, emoji, combining marks, degenerate limits) — linear,
  terminating, gap-free tiling, strictly monotonic starts, all offsets on
  char boundaries; all in-scope byte slicing is UTF-8-boundary-safe by
  construction; degenerate limits (0/1, overlap ≥ max) clamp safely.

## Remediation status (updated 2026-08-23, same branch)

Fixes are landing on this PR's branch alongside the report:

- **C1, M7 + scheduler lows** — fixed in `fix(jobs): release leaked
  reservations and fix queued-waiter liveness`: an `ActiveReservationGuard`
  releases dropped active leases; renew failure attempts `fail()` first;
  `reconcile()` terminalizes quarantined-active rows with stale renewals
  (renewals clear quarantine, preserving live-lease fencing); queued-waiter
  liveness moved to poll heartbeats on `renewed_at` with a 90 s threshold
  decoupled from `WAIT_TIMEOUT`, making priority aging reachable. Provider
  root cause is preserved when the failure release errors; `complete()`
  fence loss after successful work now returns the value with a warning;
  the cache state singleton self-heals in `prune()`. Accepted with
  documenting comments: process-local writer gate, soft `max_entries`.
- **H2, M4, M5, M6 + document lows** — fixed in `fix(document): bound
  fence/frontmatter chunks and harden markdown parsing`: fence and
  frontmatter spans are hard-windowed at `max_chars` (unterminated fences
  included); `html_article` cursor fixed; frontmatter delimiters strict
  with CRLF support; the openapi/swagger path heuristic restricted to
  json/yaml/yml; injected limits flow through the size-fallback path; wide
  fence language parsing fixed. Accepted: heading suppression after a
  stray fence remains (treating an unclosed fence as closed at EOF is
  unsafe); output size is now bounded regardless.
- **M1 + embedding-cache lows** — fixed in `fix(embedding): gate vector
  cache on verified identity and harden decorator`: cache decoration
  requires `identity.verified`; batch validation runs regardless of cache
  warmth; miss results are verified against `chunk_id` order and fail
  closed on misalignment; full-hit usage reports unmetered. Accepted with
  a documenting comment: the bounded ~750 ms worst-case inline stall.
- **H1, M2, M3 + executor lows** — remediation in progress on this branch.

## Suggested priority

1. C1 — capacity leak wedges provider domains across restarts.
2. H1 — removal-only recrawls are broken today for any shrinking source.
3. H2 + M4/M5/M6 — adversarial or merely truncated real-world pages defeat
   chunk limits or duplicate content.
4. M1 — gate cache decoration on `identity.verified`.
5. M2/M3/M7 and the low items as scheduled hardening.
