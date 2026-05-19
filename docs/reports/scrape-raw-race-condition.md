# `scrape_raw()` Biased-Select Race Condition — Root Cause Analysis

**Reported by:** axon_rust audit
**Spider versions affected:** `v2.44.4` through `v2.45.24+` (current HEAD)
**axon_rust locked version:** `v2.45.20` — **affected**
**Symptom:** `get_pages()` returns empty on fast single-URL fetches
**Status:** Confirmed bug — NOT reported upstream — worked around in `axon`

---

## Bug History / Version Timeline

This bug is the second in a chain of two mutually exclusive bugs in the `scrape*` API:

### Bug 1 — Infinite hang (`≤ v2.27.62`)

**Issue:** [#268](https://github.com/spider-rs/spider/issues/268), [#269](https://github.com/spider-rs/spider/issues/269)
**Fixed in:** `v2.27.63` (commit `5e890028`, Feb 14 2025)

The original `v2.27.63` implementation used `spawn_task`:
```rust
pub async fn scrape_raw(&mut self) {
    let mut w = self.clone();
    let mut rx2 = w.subscribe(0).expect("receiver enabled");

    spawn_task("crawl_raw", async move {
        w.crawl_raw().await;
        // ← BUG: w.unsubscribe() NOT called — sender never dropped
    });

    while let Ok(page) = rx2.recv().await { ... }
    //  ↑ hangs forever — channel never closes
}
```
`rx2.recv()` blocks indefinitely because `w` is moved into the spawned closure but `w.unsubscribe()` was never called, keeping the broadcast sender alive.

The reporter (#268) noted: `"...the receiving end does not seem to be closed anymore, leading to website.scrape().await never finishing."`

**Fix applied in v2.27.63:** Added `w.unsubscribe()` to the `spawn_task` closures — but only for `crawl`/`crawl_smart`/`crawl_sitemap`, NOT for `scrape_raw`. For `scrape_raw`, no fix was applied in that commit.

---

### Bug 2 — Empty pages on fast single-page fetches (`v2.44.4` – present)

**No upstream issue filed** — this bug was introduced as a side-effect of fixing Bug 1.
**Introduced in:** `v2.44.4` (commit `441c3712`, Feb 3 2026)
**Still present in:** `v2.45.20` (axon_rust's locked version), `v2.45.24` (current HEAD)

The "fix" in commit `441c3712` ("chore(website): fix scrape subscription hang") rewrote all
three `scrape()` / `scrape_raw()` / `scrape_smart()` from the hang-prone `while let Ok()`
pattern to `tokio::join!` + a `biased;` select driven by a `oneshot` done channel:

**BEFORE `v2.44.4`** (hang bug present):
```rust
let crawl = async move { w.crawl().await; w.unsubscribe(); };
let sub = async move { while let Ok(page) = rx2.recv().await { ... } };
tokio::join!(sub, crawl);
```

**AFTER `v2.44.4`** (race condition present):
```rust
let (done_tx, mut done_rx) = tokio::sync::oneshot::channel::<()>();
let crawl = async move {
    w.crawl_raw().await;
    w.unsubscribe();
    let _ = done_tx.send(());  // ← fires after all pages are broadcast
};
let sub = async {
    loop {
        tokio::select! {
            biased;
            _ = &mut done_rx => break,     // ← ALWAYS checked first
            result = rx2.recv() => { ... } // ← starved on fast fetches
        }
    }
};
tokio::join!(sub, crawl);
```

The hang was fixed (the `done_rx` arm terminates the loop), but a new race was introduced.

---

## Root Cause of Bug 2

### Three-part failure

**Part 1 — `tokio::join!` is cooperative, not concurrent**

Both `sub` and `crawl` run on the **same Tokio task**. They share CPU through `.await` yield
points. For fast single-page fetches, the entire `crawl` future can complete (including
`done_tx.send(())`), before `sub` ever calls `rx2.recv()`.

**Part 2 — `biased;` guarantees `done_rx` always wins**

When both `done_rx` and `rx2` have data simultaneously, `biased;` means `done_rx` arm wins
100% of the time — no random chance, no fairness. The loop immediately breaks.

**Part 3 — Race window is maximized for `with_limit(1)` on fast servers**

```
join! polls sub  → done_rx pending, rx2.recv() pending → yields
join! polls crawl → crawl_raw() runs → channel_send_page() → unsubscribe() → done_tx.send(()) → completes
join! polls sub  → BOTH done_rx and rx2 are ready simultaneously
                   biased; → done_rx always wins → break
                   page in rx2 buffer → NEVER drained
```

The page IS in `rx2`'s broadcast buffer, but `sub` never reads it.

### When it occurs vs. when it doesn't

| Scenario | Result |
|----------|--------|
| `with_limit(1)` on fast server | **Drops page** — entire crawl fits in one cooperative poll cycle |
| Small crawls (2-5 pages) on fast server | **Drops pages** — probabilistic based on server speed |
| Large multi-page crawls | **Works** — many yield points in `crawl_concurrent_raw()` give `sub` turns to drain |
| Tests with `current_thread` Tokio runtime | **Drops** — single thread, no preemption |
| Tests with multi-threaded Tokio runtime | **Flaky** — depends on scheduling |

---

## Known Related Issues (Upstream GitHub)

### Directly relevant

| Issue | Title | Version | Status | Relation |
|-------|-------|---------|--------|----------|
| [#268](https://github.com/spider-rs/spider/issues/268) | Subscribing to scraping does not seem to work anymore | Fixed v2.27.63 | Closed | Original hang — triggered the biased fix that caused Bug 2 |
| [#269](https://github.com/spider-rs/spider/issues/269) | Scrape with subscribe still does not close | Fixed v2.27.63 | Closed | Same hang, different reporter |
| [#201](https://github.com/spider-rs/spider/issues/201) | with_limit(1) does not work when "chrome" feature is enabled | Fixed v2.0.0 | Closed | Different bug (Chrome code path), same symptom: empty `get_pages()` with limit(1) |
| [#210](https://github.com/spider-rs/spider/issues/210) | Broadcast never end when scraping with limit | v2.x | Closed | User mixing external `subscribe()` + `scrape()` — told to pick one |

### Indirect / related symptoms

| Issue | Relation |
|-------|----------|
| [#256](https://github.com/spider-rs/spider/issues/256) | "no page" + queue race — unrelated root cause but same symptom |
| [#293](https://github.com/spider-rs/spider/issues/293) | Chrome renders before JS — limit(1) context |

**The current Bug 2 (biased-select empty pages, introduced in v2.44.4) has no upstream issue.**

---

## All Known Fixes / Approaches

### Fix A — Drain on `done_rx` (minimal upstream patch, correct)

The cleanest upstream fix. After `done_rx` fires, drain any remaining buffered messages
before breaking:

```rust
tokio::select! {
    biased;
    _ = &mut done_rx => {
        // Drain pages that arrived before done_rx fired.
        while let Ok(page) = rx2.try_recv() {
            if let Some(sid) = page.signature {
                self.insert_signature(sid).await;
            }
            self.insert_link(page.get_url().into()).await;
            if let Some(p) = self.pages.as_mut() {
                p.push(page);
            }
        }
        break;
    }
    result = rx2.recv() => { /* existing code */ }
}
```

**Verdict:** Correct. Guarantees all buffered pages are consumed before terminating.
One-liner reasoning: the race window closes because we always drain after done fires.

---

### Fix B — Remove `biased;` (partial fix, still probabilistic)

```rust
tokio::select! {  // fair scheduling
    _ = &mut done_rx => break,
    result = rx2.recv() => { /* existing code */ }
}
```

**Verdict:** Incomplete. Without `biased;`, Tokio randomly picks between the two arms when
both are ready. For a single-page fast fetch where both arms are simultaneously ready on the
first poll, there's a 50% chance the page is dropped. Better than 100% drop rate, but still
wrong.

---

### Fix C — Revert to `while let Ok()` + fix the original hang (correct, simpler)

The original hang (#268) was caused by missing `w.unsubscribe()`. The minimal fix would have
been:

```rust
let crawl = async move {
    w.crawl_raw().await;
    w.unsubscribe();  // ← was missing; when this drops the sender, rx2.recv() returns Err
};
let sub = async move {
    while let Ok(page) = rx2.recv().await { ... }  // naturally terminates on Err::Closed
};
tokio::join!(sub, crawl);
```

**Verdict:** Correct and simpler. The original code was fine except for the missing
`unsubscribe()`. No `done_tx`/`done_rx` or `biased;` needed at all. However, the `tokio::join!`
cooperative scheduling still means there's a narrow window where `crawl` completes and drops
the sender before `sub` polls — but since `rx2.recv()` returns `Err(RecvError::Closed)` (not
`Ok(page)`), the page would still be buffered and available. On the next `rx2.recv()` call,
the buffered page IS returned before the `Err` — `tokio::broadcast::Receiver::recv()` returns
buffered messages before returning `Closed`. So Fix C would work correctly.

---

### Fix D — `tokio::spawn` for the collector (our workaround, correct)

This is what axon_rust currently uses:

```rust
let mut rx = website.subscribe(16).ok_or("...")?;
let collect: tokio::task::JoinHandle<Option<Page>> =
    tokio::spawn(async move { rx.recv().await.ok() });

match cfg.render_mode {
    RenderMode::Http | RenderMode::AutoSwitch => website.crawl_raw().await,
    RenderMode::Chrome => website.crawl().await,
}
website.unsubscribe();

let page = collect.await.map_err(...)?.ok_or("...")?;
```

**Verdict:** Correct. `tokio::spawn` creates an independent task with its own scheduling.
No done channel, no biased select, no cooperative scheduling race. The 16-slot broadcast
buffer holds the page until the spawned task drains it. Works regardless of crawl speed.

This is equivalent to what Spider intends `scrape_raw()` to do, minus the race.

---

### Fix E — Two-phase: wait for page, then unsubscribe (alternative external pattern)

```rust
let mut rx = website.subscribe(16).ok_or("...")?;
// spawn crawl, await page, then cleanup
tokio::spawn(async move { website.crawl_raw().await; website.unsubscribe(); });
let page = rx.recv().await.ok();
```

**Verdict:** Correct for single-page use but breaks for multi-page. Our Fix D is better
because it's a `JoinHandle` that can be awaited.

---

## Version Compatibility Matrix

| Spider version | `scrape_raw()` behavior | Bug present? |
|----------------|-------------------------|--------------|
| `≤ v2.44.3` | `while let Ok() = rx2.recv().await` | **Hangs** (Bug 1: missing unsubscribe) |
| `v2.44.4 – v2.45.24+` | `tokio::join!` + `biased; done_rx` | **Drops pages** on fast single-page fetches (Bug 2) |
| **axon_rust locked: `v2.45.20`** | Bug 2 present | **Yes — workaround applied** |

---

## What We Use in `axon` and Why

`axon`'s `scrape.rs` uses Fix D — `subscribe(16) + tokio::spawn + crawl_raw()`. This:

1. Avoids `scrape_raw()` entirely (the buggy API)
2. Uses Spider's own `subscribe()` mechanism (documented, public API)
3. Is immune to the biased-select race (independent task, broadcast buffer)
4. Works on `v2.44.4+` without requiring upstream changes
5. The 16-slot capacity handles bursts if we ever change to multi-page scraping

Our workaround does NOT diverge from Spider's intended design — it uses the same subscription
mechanism that `scrape_raw()` uses internally, just without the broken cooperative scheduling.

---

## Recommendation: File Upstream Issue

The biased-select bug (Bug 2) was **introduced by a fix** in `v2.44.4` and has **no upstream
issue tracking it**. It's a regression that:

- Silently drops pages on fast servers (no error, no warning)
- Is reproducible 100% of the time with `with_limit(1)` on a fast server
- Affects all three `scrape*` variants simultaneously
- Is present in the current `v2.45.24` HEAD

A minimal issue report would include:
- Reproduction: `with_limit(1).scrape_raw()` on any fast public URL → `get_pages()` is empty
- Introduced in: `v2.44.4` (commit `441c3712`)
- Root cause: `biased;` select + `tokio::join!` cooperative scheduling drops page when `done_rx` fires before `sub` polls `rx2`
- Proposed fix: Fix A (drain on done) or Fix C (revert + add unsubscribe)
