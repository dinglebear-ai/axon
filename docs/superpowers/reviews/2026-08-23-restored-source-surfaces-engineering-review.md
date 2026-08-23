# Engineering Review: axon_rust-twfx7

Reviewed the restored `scrape`, `crawl`, `embed`, `ingest`, and `code-search`
plan as an epic-equivalent anchor. Four Lavra reviewers covered architecture,
simplicity, security, and performance. The plan and design were amended in the
same review cycle; this report records the findings and their disposition.

## Architecture

Strengths: the API/services/jobs/transports dependency direction is sound, the
restored names remain projections, and canonical scheduling remains intact.

Resolved concerns:

- Foreground mutations are now job-first and share atomic admission with
  detached calls, closing the idempotency race.
- Reused jobs correlate to multiple initiating batches through
  `projection_batch_items` rather than mutable single ownership.
- Prepared identities and execution-time fail-closed validation replace an
  unsafe preflight/reroute gap.
- REST handlers call the public services facade instead of handler-owned seams.

## Simplicity

Resolved over-engineering:

- `BatchOutcome<T>` makes queued/completed/failed/canceled states valid by type.
- The global jobs idempotency model is preserved; projection scope is encoded in
  an opaque derived key rather than rewriting every legacy caller.
- The fictional generic expanded-item allocator and per-request concurrency knob
  were removed in favor of owning stage limits and canonical scheduler fairness.
- The registry is metadata plus bijection enforcement, not an unplanned Rust
  code generator; fixtures compare semantic fields without a combinatorial
  cross-product.
- CLI idempotency uses self-contained items/request files instead of
  adjacency-sensitive flag pairing.

## Security

Resolved vulnerabilities and missing protections:

- Restored code search is committed-state read-only and cannot request refresh.
- Principal identity is an opaque issuer/subject digest; loopback is scoped by
  instance and OS uid; raw email/token/key data is not persisted.
- URL redirects/connections and local opens repeat SSRF/root/no-follow checks at
  use time, covering DNS rebinding and symlink replacement.
- Body, decoded input, key, fetched/decompressed, prepared, vector, query-window,
  and response limits are explicit; admission/rate saturation maps to `429`.
- CLI output is contained, no-follow, create-new, atomic, and no-clobber.
- Errors and telemetry are sanitized; batch/job/event reads are principal-bound.

## Performance

Resolved bottlenecks:

- SQLite preparation happens before a short writer transaction; key lookup and
  inserts are set-based/batched with canonical busy/snapshot retries.
- Foreground execution waits sequentially; detached jobs use the existing global
  scheduler rather than an unenforceable per-batch restart contract.
- Identical immutable code-search plans are coalesced within a batch.
- Input and result memory are byte-bounded; indexed result slots avoid a final
  result sort/copy.
- Limits are enforced where pages, manifest items, prepared bytes/documents,
  chunks/vectors, and query windows are actually created.

## Failure Modes

| Codepath | Failure mode | Rescued? | Test? | User sees | Logged? |
|---|---|---:|---:|---|---:|
| Projection parse | Empty/oversized/unknown input | Yes | Yes | Typed 4xx/413 | Safe counter |
| Principal resolution | Token rotation/issuer collision | Yes | Yes | Typed auth error | Opaque ID only |
| URL/path use | Redirect/DNS/symlink target changes | Yes | Yes | Typed auth/routing error | Opaque IDs |
| Admission | Collision, busy, cancel, commit ambiguity | Yes | Yes | 409/retryable 5xx | Batch/index |
| Foreground worker | Panic/timeout/cancel | Yes | Yes | Tagged failure | Batch/index |
| Detached response | Disconnect after commit | Yes | Yes | Retry reuses jobs | Post-commit event |
| Stage limits | Bytes/chunks/vectors exhausted | Yes | Yes | Structured limit | Safe metric |
| Code search | Duplicate plans/provider failure | Yes | Yes | Ordered result/failure | Safe counts |
| CLI output | Escape, race, ENOSPC, rename failure | Yes | Yes | Nonzero exit | Redacted path |
| MCP/REST | Oversize/auth/rate saturation | Yes | Yes | 413/4xx/429 | Safe counter |
| Telemetry | Sink failure or secret-bearing error | Yes | Yes | Work unaffected | Drop counter |

No row remains both unrescued and untested with silent user impact.

## NOT in Scope

- Grouped batch cancellation and a batch state machine: correlation and
  per-item canonical lifecycle are sufficient for this delivery.
- Restoring `code-search` indexing/watch/freshness: mutation stays on canonical
  source/index surfaces.
- Dynamic unused-capacity redistribution: owning persisted limits are stable
  across restart.
- A private batch scheduler or query-result cache: existing global scheduler
  and within-batch immutable-plan coalescing cover the required objective.

## Summary

- Critical issues: 4 found, 4 addressed.
- Important issues: 10 consolidated, 10 addressed.
- Minor improvements: 4 consolidated, 4 addressed.
- Deferrable proposals: 4 explicitly retained as non-goals; none are required
  for the restored focused surfaces.

## Completion Summary

```text
Architecture issues: 6  |  Simplicity: 8  |  Security: 9  |  Performance: 8
Critical gaps: 0 remaining  |  Recommendations applied: 18
```
