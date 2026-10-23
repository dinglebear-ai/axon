---
title: "Pipeline performance and resource boundaries"
created: 2026-09-08
updated: 2026-10-16
---

# Pipeline performance and resource boundaries

This guide describes the backend safeguards introduced by the September 2026
performance remediation. These are implementation boundaries, not measured
end-to-end crawl speedups. The generated configuration registries remain the
authority for supported tuning keys.

## Acquisition and preparation

HTTP acquisition reuses a connection pool while maintaining request-local
redirect history and validating every redirect destination. Credentialed
requests retain their origin restrictions. Reuse does not bypass SSRF checks.

Web results can arrive in completion order rather than discovery order. Item
ordinals still identify their original discovery positions; finality means all
items have completed, not that the numerically last ordinal arrived. Adapters
that support acquisition prefetch may have two acquisition waves admitted;
other adapters retain one. Provider reservations and bounded channels still
limit active work. Preparation can consume available results before a slow
earlier fetch finishes.

Markdown source positions use a document-wide index instead of repeatedly
scanning prefixes. Splitting an oversized section copies its selected window,
not the entire section. These changes preserve content and source ranges.

## Embedding and publication

TEI request and weighted-input admission are shared by normalized endpoint.
Different tuning profiles cannot create independent global budgets for the
same endpoint. The first live owner establishes that endpoint's limits; later
profiles may be stricter but cannot raise its global limits. New limits take
effect after all owners of the old endpoint admission state have gone away.
Restart or drain long-running consumers when applying such changes.

A TEI `Retry-After` beyond the in-call wait budget is returned as provider
cooling rather than keeping request permits occupied during a long sleep.
Representable retry dates are preserved for scheduling. Invalid, overflowing
retry dates fail explicitly. Short retries release admission before backoff.

Successful TEI `/embed` bodies are capped at **16 MiB** while streaming, even
without a Content-Length header. Oversized multi-input responses trigger smaller
batches; a single oversized vector fails with `embedding.tei.response_too_large`
rather than retrying indefinitely. `/info` success bodies are capped at **1 MiB**.
These byte limits do not impose a new configured batch-size or dimension limit.

Qdrant carry-forward publication uses both point-count and encoded-request-byte
limits, including the 16 MiB request cap. An individual point that cannot fit
is rejected explicitly before that oversized request is sent. Requests are
encoded incrementally rather than collecting every encoded batch in memory.

## Generation side effects

The generation accumulator enforces a **256 MiB lifetime side-effect charge**,
independently of rolling prepared-work admission. Each accepted batch is charged
using the larger of its estimated resident size and serialized size. The charge
applies whether the batch is on disk or in the in-memory fallback.

The private spool stores records and unique keys in SQLite rather than growing
an in-memory key set. Each record remains capped at 64 MiB. The database has a
256 MiB page limit, a 4 MiB cache target, cache spilling enabled, and memory
mapping disabled. The cache target is not an exact process RSS ceiling; record
encoding, SQLite value buffers, and deserialized values have separate transient
costs. Rollback journals can temporarily add disk usage beyond the main database.

Appends commit atomically with full SQLite synchronization. A private owner lock
fences concurrent access, and stored identity includes source, job, generation,
and attempt. Final replay seals a checksum receipt and rejects changed or
corrupted records. Reopening a private store can recover committed records and
roll back an interrupted transaction. Production work directories are registered
with durable owner descriptors and discovered at startup. Recovery preserves
unknown or active work and reaps only a proven terminal or published generation
under the publication settlement fence. It never republishes private work. Four
local work permits bound concurrent registered generations. Registered append
failures fail closed; the bounded in-memory fallback is restricted to legacy
unregistered callers. Corruption or a seal mismatch also fails closed.

Finalization receives spool records through a one-record Tokio channel. The
blocking producer owns the cursor and sealed store; a closed receiver releases
producer backpressure. With the production outbox, candidates are staged in
pages of at most 16 per record, without rebuilding a generation-wide candidate
vector. Refreshed manifest items replace the existing inventory directly using
a key-to-position index. The sealed owner survives normal publication and
candidate settlement. Persistent recovery also covers canceled callers and
process exits. Delivery pages use the existing outbox format and startup drain;
publication errors retain intent for ledger-gated recovery. Outbox scans admit
at most 1,024 paths, 64 MiB serialized bytes, and a separate 64 MiB conservative
structural charge per pass. A rotating cursor prevents later pages starving.
A cross-process admission lock enforces a 1 GiB disk quota including quarantined
files. Historical bounded publication receipts fence delivery and stale cleanup.

This is not an exact RSS ceiling. Web WARC archives stream one acquired item
at a time into a private registered file with a 256 MiB archive limit. The file
artifact provider verifies its size and digest while copying through a 64 KiB
buffer, and publishes its manifest after synchronization. Cleanup intent is
recorded before the write; provider locks prevent deletion overtaking a detached
writer. A generation exceeding its side-effect budget still fails explicitly.

Spool creation, serialization, append, fallback replay, and final replay run on
blocking workers rather than occupying a Tokio async worker. Each accumulator
has one blocking operation outstanding. A canceled operation retains ownership
of its private spool until the blocking operation finishes. Ambiguous append
failure does not replay the same side-effect record twice.

## Private graph journal

The combined graph journal and generation side-effect budget remains **256 MiB**.
Graph journal records now use lossless compressed JSON in the existing text
column; legacy JSON records remain readable. Accounting charges the stored
encoded record size. Decoding is capped at 8 MiB per record, encoded records at
12 MiB, and raw and encoded batches independently at 64 MiB. Private canonical
rows reject growth beyond an 8 MiB stored window. The private graph database has
a 1 GiB main-file page limit and a 64 MiB cache target; rollback journals can
transiently increase physical usage. These are admission bounds, not total RSS
ceilings. No graph facts or code indexing are disabled.

Code evidence validation uses sparse source line offsets rather than rescanning
the entire source for each range. Single-line quote searches avoid constructing
a complete function-body string; CRLF and Unicode semantics remain equivalent.

## SQLite scheduling and lifecycle

Observing a fresh queued reservation does not acquire the SQLite writer.
Liveness renewal is conditional and coalesced to a 30-second interval; the
elected recovery path continues to protect long-lived queued work. Domain
notifications still cause reads, but not a write transaction per fresh waiter.
Cancellation and queue timeout remove the reservation and grant eligible
successors in the same writer transaction, then notify waiters. Freed capacity
does not wait for the five-second recovery poll.

Terminal-warning projection reads events in 128-row keyset pages outside the
writer transaction and deduplicates with a hash set. The write is fenced by
attempt, event sequence, and prior warning state. A concurrent event forces a
fresh projection rather than silently losing a late warning. Warnings across
attempts remain visible.

Definitive source-lease loss cancels uncommittable work through a child
cancellation token and settles generation cleanup. A transient heartbeat error
is not treated as proof that ownership was lost. Failure-summary errors no
longer bypass lease release.

## Graph retirement and physical cleanup

Source jobs complete after publication and graph writes. Physical cleanup is
owned by the existing durable debt worker and does not run in the completion
path. A failed deletion remains pending across restart; vector collection
identity is persisted before handoff, and each sweep uses the current committed
generation as its deletion fence.

Graph item retirement groups at most 64 debts per provider operation. One source
lease protects the group's fresh manifest/status checks. SQLite deletes only
evidence belonging to eligible items and preserves shared edges and currently
supported nodes. A stable-key-leading index supports node retirement across node
kinds; each bounded SQL transaction releases the writer gate before the next.

Graph extraction already overlaps embedding. Durable graph writes cannot move
before publication without hidden generation staging and query visibility gates:
the current graph uses shared identities and exposes each committed write batch.

## LLM admission and subprocess lifetime

The completion deadline includes reservation and execution-admission waiting,
not only backend execution. Execution admission honors interactive/background
priority, with a bounded interactive burst so background work is not starved.
Cancellation removes queued or granted admission and releases pool accounting.

OpenAI-compatible completion concurrency defaults to 16 when unset. Explicit
values, including 4, remain explicit; they are not replaced by a heuristic.
Other backend defaults remain backend-specific.

OpenAI-compatible successful bodies are limited to 16 MiB. Streaming frames
are limited to 1 MiB and cumulative stream input to 16 MiB. The definitive
`[DONE]` marker ends reading without waiting for HTTP EOF; ordinary finish
reasons do not discard later usage information.

Codex protocol lines are limited to 1 MiB, with 16 MiB aggregate budgets for
handshake/turn input. Cancellation during initialization or an active turn
cleans up the subprocess group and stderr reader. Idle pools reclaim children
without requiring another checkout; retired pools do not keep reaper tasks
alive indefinitely.

## Graph evidence

Evidence reads preflight record count and charged stored bytes in a consistent
read snapshot before materializing payloads. Limits are **1,000 records**,
**1 MiB charged stored bytes**, and **2 MiB serialized evidence bytes** per
bounded evidence set. Stored-byte charging includes per-record overhead.

If evidence exceeds a limit, graph traversal returns topology with an explicit
warning and omits all evidence from that response. Direct evidence-bearing
detail reads return `graph.evidence_limit_exceeded` instead of silently returning
partial evidence. Normal bounded results preserve order and attribution.

This is a summary-mode fallback, not evidence pagination. Existing responses
may represent bounded evidence in two places; the per-copy serialized bound
does not mean the entire response envelope is limited to 2 MiB.

## Verification and rollout

Use `RUST_MIN_STACK=8388608` for the composed Rust test suite, matching CI's
stack setting for deeply nested async tests. Protocol-success subprocess tests
use generous startup budgets; separate deadline tests enforce timeout behavior.

Unit and mock-provider evidence is not proof that a running daemon contains the
new implementation. Confirm the deployed binary before a live benchmark. A CLI
enqueue may be consumed by an already-running worker sharing its database. Use
an explicit binary and an isolated data directory, as the benchmark harness
does, when testing a local build. There is no generic CLI forwarding mode. Do not compare a
frozen-corpus TEI replay directly with a live acquisition-to-publication crawl.

### Inline artifact reads and outbox quota reservations

Inline artifact resolution admits at most the existing 96 MiB upload limit;
artifact manifests admit 1 MiB. Larger bodies fail before allocation with an
actionable streaming-download alternative. CLI byte ranges seek before reading
and admit only the selected window; they do not load the entire artifact.

Outbox pages reserve bytes durably under the cross-process admission fence.
Opening an outbox reconciles legacy files, quarantine and interrupted writes once;
subsequent page reservations use an indexed SQLite counter instead of rescanning
the directory. Removal is synced before releasing its reservation. Interrupted
writes may conservatively overcharge until reopen/reconciliation. The existing
1 GiB total admission includes 96 MiB reserved overhead: 16 MiB quota database,
its rollback journal, and one 64 MiB temporary delivery. Corrupt quota state fails
closed and preserves files for inspection; it never grants extra disk admission.
