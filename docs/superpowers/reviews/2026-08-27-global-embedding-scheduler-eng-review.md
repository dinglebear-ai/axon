# Engineering Review: Global Embedding Scheduler Epic

Date: 2026-08-27
Review mode: Lavra engineering review, full/default
Reviewers: architecture, simplicity, security, performance

## Architecture

Strengths: evidence-first rollout, one Metal owner, bounded producer/consumer
shape, explicit quality invariants, disabled-by-default cutover, and retention
of generation publication/cleanup ownership.

Critical concerns found: naïve `join!` cancellation, delayed artifact tracking,
generation-wide result retention, partial durable statuses, regressing progress
writes, and a `created_generation`/`vectorize` dependency cycle.

Resolution: the revised design uses neutral `generation_work`, generation-layer
orchestration, immediate artifact registration, incremental absorption,
cumulative statuses, serialized progress, and quiescent provider cleanup.

## Simplicity

Over-engineering found: `ScheduledBatchResult` attribution, duplicate telemetry
transports, independent queue/pool/flush knobs, document-median sorting, and a
permanent compatibility accumulator API.

Resolution: remove scheduled results, use aggregate epoch metrics, derive a
two-message queue from one experimental pool knob, preserve arrival order, and
delete compatibility APIs during integration.

## Security

Critical risks found: artifacts produced before bulk registration could leak;
late remote upserts could complete after failed-generation deletion.

Other risks: unauthenticated non-loopback MLX exposure, unbounded requests,
shell/secret leakage, untrusted telemetry, excessive memory, timing side
channels, and unsanitized joined errors.

Resolution: immediate tracking, provider quiescence/final delete, loopback by
default with authenticated non-loopback refusal, request limits, hardened shell
rules, strict metric validation, byte/RSS gates, and established redaction.

## Performance

Critical concerns found: early permit release, redundant document sorting,
invalid overlapping-duration math, loss of acquisition prefetch, result-vector
memory growth, flush-deadline starvation, SQLite transaction amplification,
unequal cold states, and the existing `truncate: true` wire contract.

Resolution: permits live through durable completion, exact synchronized Metal
intervals, retained prefetch, absolute deadlines, 100-status buffering, pinned
replay plus separate cold-service tests, peak RSS/thermal monitoring, and an
explicit lossless TEI contract.

## Failure Modes After Plan Revision

| Codepath | Failure mode | Rescued? | Test planned? | User sees | Logged? |
|---|---|---:|---:|---|---:|
| MLX request | oversized/deep JSON exhausts memory | Yes, 4xx | Yes | Clear error | Aggregate only |
| MLX bind | unauthenticated remote peer drives Metal | Yes, startup refusal/auth | Yes | Clear startup/401 | Sanitized |
| MLX metrics | async Metal timing creates false gate | Yes, synchronized intervals | Yes | Benchmark fails closed | Reason code |
| Metric delta | restart or unrelated request contaminates run | Yes, epoch/request isolation | Yes | Benchmark rejects | Reason code |
| Benchmark | secret-bearing subprocess output escapes | Yes, sanitization/private temp | Yes | Redacted failure | Sanitized |
| Producer artifacts | later preparation step fails | Yes, immediate tracking | Yes, every boundary | Job failure | Sanitized |
| Queue permit | consumer fails while producer waits | Yes, cancellation select | Yes | Job failure | Aggregate |
| Queue/accumulator memory | retained state or long inputs exceed bound | Yes, byte permits + durable spool | Yes, sustained/oversized | Clear limit/stability rejection | Metrics |
| Scheduler timer | trickle arrivals reset deadline | Yes, absolute deadline | Yes | Bounded latency | Metrics |
| Split document | later pool overwrites total status | Yes, cumulative map | Yes, SQLite-backed | Correct cumulative state | Status |
| Progress | old producer write lands late | Yes, epoch/write mutex | Yes, reversed completion | Monotonic | Progress |
| SQLite | status/progress writers starve | Yes, fair gate/buffering | Yes | Bounded or explicit error | Wait metrics |
| Producer failure | prior durable results are discarded | Yes, incremental absorb | Yes | Accurate failed job | Sanitized |
| Consumer failure | non-cooperative producer hangs join | Yes, bounded select/drop | Yes | Prompt failure | Sanitized |
| Upsert cancellation | remote mutation finishes after delete | Yes, quiescence/final delete | Yes | Failed job, zero vectors | Cleanup debt |
| Prefetch | new producer serializes acquisition | Yes, three-event overlap contract | Yes | No regression | Timing |
| TEI wire | provider honors `truncate:true` | Yes, explicit no-truncation | Yes | Clear incompatibility | Sanitized |
| Cutover | thermal/network noise chooses loser | Yes, paired pinned replay/gates | Yes | Scheduler stays off | Benchmark |

No row remains `RESCUED=N`, `TEST=N`, and silent. Critical gaps after revision: 0.

## Not in Initial Scope

- Rust response-header telemetry — aggregate epoch deltas are sufficient.
- Public queue/flush configuration — prevents unsupported combinations.
- Document-median/adaptive scheduling — redundant until evidence says otherwise.
- Prometheus history/request audit/mTLS/signing — defer for loopback-only use.
- Default-on cutover — separate acceptance decision after all gates.

## Recommendation Audit

Applied:

- [x] 1. Stream accumulation; remove generation-wide scheduled results.
- [x] 2. Preserve immediate artifact registration.
- [x] 3. Fence provider work before failed-generation deletion.
- [x] 4. Replace naïve join semantics with bounded cancellation orchestration.
- [x] 5. Make permit and send waits cancellation-aware.
- [x] 6. Move neutral work contract out of sibling modules.
- [x] 7. Persist cumulative split-document statuses.
- [x] 8. Serialize progress and make late producer updates count-only.
- [x] 9. Inventory, batch, instrument, and contention-test SQLite writes.
- [x] 10. Hold permits through completion; add byte and peak-RSS bounds.
- [x] 11. Remove redundant document-median ordering.
- [x] 12. Anchor the absolute oldest-item flush deadline.
- [x] 13. Preserve and test acquisition prefetch.
- [x] 14. Correct Metal telemetry, occupancy, epoch, and validation semantics.
- [x] 15. Secure MLX binding and limit total requests.
- [x] 16. Harden and separate benchmark states; verify corpus/vector equality.
- [x] 17. Enforce and test no truncation on the wire.
- [x] 18. Defer duplicate telemetry and premature public knobs explicitly.
- [x] 19. Durably spool retained accumulator state and quantify the complete
  scheduler-owned memory bound.

Skipped: none.

Total: 19 applied, 0 skipped.

## Completion Summary

```text
Architecture issues: 12 | Simplicity: 13 | Security: 9 | Performance: 18
Critical gaps before revision: 7 consolidated | Critical gaps after revision: 0
Recommendations applied: 19 | Skipped: 0 | Deferred decisions documented: 5
```
