# Session: AMQP Nack Storm Fix — Ingest Worker Lanes

**Date:** 2026-03-24
**Branch:** `feat/warm-session-pool`

---

## Session Overview

Fixed a nack storm that caused all 6 ingest AMQP worker lanes to spam "pre-ack buffer full, nacking delivery for requeue" hundreds of times per second with zero forward progress. Root cause: the saturation select block always polled `consumer.next()` regardless of buffer state — when full, every delivery was nacked back to RabbitMQ, which immediately re-delivered to the next saturated lane, triggering another nack in a tight feedback loop.

---

## Timeline

1. **Identified root cause** from log output: all 6 lanes simultaneously at saturation, preacked buffer full (cap=12), nacking every delivery with `requeue: true` → tight loop across all lanes.
2. **Fixed `amqp.rs`**: Split saturation select into two paths — buffer full → skip `consumer.next()` entirely; buffer has capacity → poll and pre-ack as before.
3. **Fixed `delivery.rs`**: Removed nack from late-saturation race path (`try_acquire_owned` failure); always ack and push to buffer (overflow at most 1).
4. **Removed `preack_cap` from `claim_delivery` signature** (parameter no longer needed after nack removal).
5. **Removed unused `BasicNackOptions` import** from `amqp.rs`.
6. **Confirmed `ws_runner.rs` clean**: Pre-existing `url::form_urlencoded` compile error was resolved by a linter applying inline `percent_encode` helper before this session's edit retry.
7. **Verified compile**: `cargo check --bin axon` passes clean.

---

## Key Findings

- **Nack storm mechanism** (`amqp.rs:278-309`): With `basic_qos(1)`, every `ack` caused RabbitMQ to push the next queued message. When the pre-ack buffer was full (12 items × 6 lanes = 72 jobs in RAM), each newly pushed delivery was nacked with `requeue: true` → instantly re-delivered → nacked again. All 6 lanes participated simultaneously, creating hundreds of warn/second with 0 forward progress.
- **Fix insight**: When the buffer is full, *not polling* `consumer.next()` leaves the 1 unacked delivery sitting in lapin's internal buffer. This counts against `basic_qos(1)` — RabbitMQ will not push another message. The delivery waits (seconds, well within 30-min `consumer_timeout`) until a job completes and frees a buffer slot.
- **Late-saturation race** (`delivery.rs:104-129`): The `try_acquire_owned` failure path also nacked with `requeue: true`. This fires when a delivery arrives just as the last semaphore permit is taken (between the saturation check and `claim_delivery`). Since it's a single race event (not a sustained loop), always acking and accepting a 1-item buffer overflow is correct and safe.
- **`ws_runner.rs`**: The `percent_encode` helper was added inline by the linter, eliminating the undeclared `url` crate dependency. The file is clean.

---

## Technical Decisions

### Split saturation select on buffer state
**Decision:** Two `tokio::select!` blocks instead of one with conditional nack.

**Why:** The original design tried to handle both cases (buffer available / buffer full) in one select by nacking on overflow. The nack itself was the bug — it triggered immediate redelivery to another saturated lane. Splitting makes the "do not accept deliveries" intent explicit rather than trying to accept-then-discard.

**Rejected alternative:** `nack` with `requeue: true` when buffer full, relying on RabbitMQ's nack-delay behavior. RabbitMQ does not provide a configurable nack delay on standard queues — requeue is effectively immediate.

### Remove `preack_cap` from `claim_delivery` signature
**Why:** The only use of `preack_cap` in `claim_delivery` was to decide whether to nack. After removing the nack path, the cap check is gone and the parameter became dead weight.

### Always ack in late-saturation race path
**Why:** The race fires at most once per delivery cycle (not in a loop). The 1-item buffer overflow is bounded and harmless. Nacking would push the delivery back to another lane that is equally saturated — same storm, smaller scale.

---

## Files Modified

| File | Change |
|------|--------|
| `crates/jobs/worker_lane/amqp.rs` | Split saturation select on buffer fullness; removed unused `BasicNackOptions` import |
| `crates/jobs/worker_lane/delivery.rs` | Always ack in late-saturation race; removed `preack_cap` parameter from `claim_delivery` |

---

## Commands Executed

```bash
cargo check --bin axon
# → Finished `dev` profile [unoptimized + debuginfo] target(s) in 31.87s
```

---

## Behavior Changes (Before/After)

| Scenario | Before | After |
|----------|--------|-------|
| All 6 ingest lanes saturated + pre-ack buffer full | Nack storm: hundreds of WARN/second, zero forward progress | Lanes block on `inflight.next()` — no deliveries accepted until a job completes, no nacks |
| Delivery arrives just as last semaphore permit taken | Nacked with `requeue: true` | Acked and buffered (1-item overflow); not re-delivered |
| Buffer has capacity during saturation | Pre-acked and buffered | Unchanged |
| Consumer timeout risk | Present (nack storm consumed CPU, no forward progress) | Eliminated — unacked delivery stays in lapin buffer, well within 30-min timeout |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check --bin axon` | 0 errors, 0 warnings | Finished clean | ✓ PASS |
| `grep 'BasicNackOptions' crates/jobs/worker_lane/amqp.rs` | No matches | No matches | ✓ PASS |
| `grep 'preack_cap' crates/jobs/worker_lane/delivery.rs` | No matches | No matches | ✓ PASS |
| `grep 'url::' crates/services/acp_llm/ws_runner.rs` | No matches | No matches | ✓ PASS |

---

## Risks and Rollback

**Risk:** Leaving a delivery unacked in lapin's buffer during buffer-full saturation means that one RabbitMQ consumer slot is "occupied" for the duration. This is the intended behavior — it backpressures the broker — but if the lane restarts before a job completes, the delivery is nacked by lapin on channel close and returns to the queue.

**Mitigation:** Existing channel-close path in `close_amqp_lane()` and RabbitMQ's nack-on-disconnect semantics handle this correctly. Re-enqueue logic for pre-acked jobs also fires on lane exit.

**Rollback:** Revert `amqp.rs` and `delivery.rs` to pre-fix state. The previous behavior was broken (nack storm) so rollback is not recommended unless a different bug is introduced.

---

## Decisions Not Taken

- **Deadletter queue for overflow:** Considered routing excess deliveries to a DLX instead of nacking. Rejected — adds infrastructure complexity; the real fix is not nacking at all.
- **Increase `preack_cap`:** Could raise the cap to delay the storm. Rejected — does not fix the root cause; just delays the storm until the (larger) cap is hit.
- **`consumer.next()` with nack + delay via `tokio::time::sleep`:** Rejected — `tokio::sleep` in a select arm with `consumer.next()` would still create a redelivery loop, just slower.

---

## Open Questions

- Under sustained saturation (all 6 lanes + large queue), do the lanes drain the pre-ack buffer efficiently once permits free? Monitor `pre-acked job(s) on exit` log lines in production to confirm the buffer doesn't grow unbounded.

---

## Next Steps

- Deploy and monitor ingest worker logs to confirm nack storm is eliminated under real load.
- Consider adding a metric/counter for pre-ack buffer watermark to detect future saturation regressions.
