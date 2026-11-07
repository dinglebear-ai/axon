---
title: "Pipeline performance and resource boundaries"
created: 2026-09-08
updated: 2026-09-08
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

This is not an exact RSS ceiling. Final archive construction still requires a
bounded complete generation in memory. A generation exceeding the supported
side-effect budget fails explicitly; spilling is not permission to accept an
unlimited generation. This implementation does not add a streaming archive API.

Spool creation, serialization, append, fallback replay, and final replay run on
blocking workers rather than occupying a Tokio async worker. Each accumulator
has one blocking operation outstanding. A canceled operation retains ownership
of its private spool until the blocking operation finishes. Ambiguous append
failure does not replay the same side-effect record twice.

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
