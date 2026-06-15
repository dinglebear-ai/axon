# Spider Adaptive Crawl and Remote Policy Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` if available; otherwise use `superpowers:executing-plans`. Implement task-by-task and update each checkbox as it is completed.

**Goal:** Wire Axon to Spider 2.52.0's opt-in adaptive crawl concurrency and Chrome remote-local-policy APIs without changing default crawl behavior.

**Reviewed scope:** This plan has been updated after Lavra engineering review. The first release intentionally avoids palette UI exposure, new CLI flags, arbitrary decrease-factor config, and unused sync-interval config. The accepted surface is TOML/config snapshot/runtime/docs/tests only.

**Architecture:** Keep `runtime.rs` focused on Spider `Website` construction. Add `src/crawl/engine/adaptive.rs` for Axon-owned adaptive control, attach it only when `[workers.adaptive-concurrency] enabled = true`, and pass it through `CollectorConfig`. Chrome remote-local-policy remains a separate opt-in boolean under `[chrome]`.

**Spider evidence:** `Website::with_adaptive_concurrency(handle.semaphore())` stores only the semaphore. Axon must keep its own `AdaptiveSemaphore` handle alive so the collector can call `AIMDController` and resize the same semaphore used by the `Website`. Spider 2.52.0 currently ignores arbitrary `decrease_factor` values and effectively halves on failure, so Axon must document a fixed `0.5` failure decrease instead of exposing a misleading knob.

---

## File Structure

- Modify: `src/core/config/types/config.rs` - add minimal `AdaptiveConcurrencyConfig` plus `chrome_remote_local_policy`.
- Modify: `src/core/config/types/config_impls.rs` - disabled defaults.
- Modify: `src/core/config/types/config_debug.rs` - debug fields.
- Modify: `src/core/config/parse/toml_config.rs` - parse `[workers.adaptive-concurrency]` and `chrome.remote-local-policy`.
- Modify: `src/core/config/parse/build_config/config_literal.rs` - copy parsed values.
- Modify: `src/core/config/parse/build_config/post_init.rs` - validate adaptive bounds and max cap.
- Modify: `src/core/config/parse/build_config/tests/priority_chain/workers_search.rs` - TOML/validation tests.
- Modify: `src/jobs/config_snapshot.rs` - snapshot/replay both new config values.
- Add: `src/crawl/engine/adaptive.rs` - adaptive controller, status classification, warnings, tests.
- Modify: `src/crawl/engine.rs` - attach adaptive controller to crawl `Website` and collector config.
- Modify: `src/crawl/engine/collector.rs` - record page status and broadcast lag feedback.
- Modify: `src/crawl/engine/collector/types.rs` or local collector config module - add `adaptive: Option<AdaptiveCrawlControl>`.
- Modify: `src/crawl/engine/runtime.rs` - set Chrome remote-local-policy when configured.
- Modify: `src/crawl/engine_tests.rs` - runtime/security/integration tests.
- Modify: `config.example.toml`, `docs/guides/configuration.md`, `docs/operations/performance.md`, `docs/reference/spider-feature-flags.md`, and `CLAUDE.md` - document opt-in behavior and limitations.

---

## Current State

Axon already resolves `spider 2.52.0` and compiles Spider's `basic` feature, so `adaptive_concurrency` and Chrome interception APIs are available. The active crawl builder still uses a fixed profile-derived limit:

```rust
if let Some(limit) = cfg.crawl_concurrency_limit {
    website.with_concurrency_limit(Some(limit.max(1)));
}
if cfg.delay_ms > 0 {
    website.with_delay(cfg.delay_ms);
}
```

Axon also enables Chrome request interception with:

```rust
website
    .with_chrome_intercept(RequestInterceptConfiguration::new(true))
    .with_stealth(true)
    .with_fingerprint(true);
```

This plan keeps those defaults. Adaptive mode replaces the fixed crawl semaphore only when explicitly enabled.

---

## Task 1: Minimal Configuration and Snapshot Replay

**Files:**
- `src/core/config/types/config.rs`
- `src/core/config/types/config_impls.rs`
- `src/core/config/types/config_debug.rs`
- `src/core/config/parse/toml_config.rs`
- `src/core/config/parse/build_config/config_literal.rs`
- `src/core/config/parse/build_config/post_init.rs`
- `src/core/config/parse/build_config/tests/priority_chain/workers_search.rs`
- `src/jobs/config_snapshot.rs`

- [x] **Step 1: Add failing TOML parsing and validation tests**

Add tests beside the existing workers config tests in `src/core/config/parse/build_config/tests/priority_chain/workers_search.rs`.

Test cases:
- `[workers.adaptive-concurrency] enabled = true`, `min = 2`, `max = 32` parses into `cfg.adaptive_concurrency`.
- `[chrome] remote-local-policy = true` parses into `cfg.chrome_remote_local_policy`.
- `min > max` fails with `workers.adaptive-concurrency.min must be <= max`.
- `max > min(crawl-broadcast-buffer-max, 1024)` fails with `workers.adaptive-concurrency.max must be <= min(crawl-broadcast-buffer-max, 1024)`.
- Unknown keys such as `decrease-factor`, `initial`, or `sync-interval-ms` fail because `deny_unknown_fields` must remain active. This prevents accepting knobs Spider/Axon do not honor.

Use the existing test helpers: `TempfileBuilder`, `ENV_LOCK`, `with_env_saved`, and `into_config_via_args(&["status"])`.

Run:

```bash
cargo test config::parse::build_config_tests::priority_chain::workers_search -- --nocapture
```

Expected before implementation: the new tests fail because the fields do not exist yet.

- [x] **Step 2: Add the typed config**

In `src/core/config/types/config.rs`, add:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdaptiveConcurrencyConfig {
    pub enabled: bool,
    pub min: usize,
    pub max: Option<usize>,
}
```

Add fields to `Config`:

```rust
pub adaptive_concurrency: AdaptiveConcurrencyConfig,
pub chrome_remote_local_policy: bool,
```

Default in `src/core/config/types/config_impls.rs`:

```rust
adaptive_concurrency: AdaptiveConcurrencyConfig {
    enabled: false,
    min: 1,
    max: None,
},
chrome_remote_local_policy: false,
```

Add both fields to `src/core/config/types/config_debug.rs`.

- [x] **Step 3: Parse TOML only**

In `src/core/config/parse/toml_config.rs`, add:

```rust
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(super) struct TomlAdaptiveConcurrencySection {
    pub enabled: Option<bool>,
    pub min: Option<usize>,
    pub max: Option<usize>,
}
```

Add to the TOML workers section:

```rust
#[serde(default)]
pub adaptive_concurrency: TomlAdaptiveConcurrencySection,
```

Add to the TOML Chrome section:

```rust
pub remote_local_policy: Option<bool>,
```

In `config_literal.rs`, copy parsed values into `Config`. Do not add global CLI flags in this slice.

- [x] **Step 4: Validate derived bounds**

In `post_init.rs`, validate only when `cfg.adaptive_concurrency.enabled` is true:

- Clamp `min` to at least `1` during literal construction.
- Resolve `max` to `cfg.crawl_concurrency_limit.unwrap_or(profile_resolved_limit)`.
- Reject `min > max`.
- Reject `max > cfg.crawl_broadcast_buffer_max.min(1024)`.

Keep fixed `crawl_concurrency_limit` behavior unchanged when adaptive is disabled.

- [x] **Step 5: Snapshot and replay config**

In `src/jobs/config_snapshot.rs`:

- Add serializable snapshot fields for `adaptive_concurrency` and `chrome_remote_local_policy`.
- Include them in `ConfigSnapshot::from_config`.
- Restore them in `ConfigSnapshot::apply_to`.
- Add a focused unit test or extend an existing snapshot round-trip test proving queued/recovered crawl jobs preserve these settings.

Run:

```bash
cargo test jobs::config_snapshot -- --nocapture
cargo test config::parse::build_config_tests::priority_chain::workers_search -- --nocapture
```

---

## Task 2: Chrome Remote-Local-Policy Runtime Wiring

**Files:**
- `src/crawl/engine/runtime.rs`
- `src/crawl/engine_tests.rs`

- [x] **Step 1: Write failing runtime tests**

Add tests in `src/crawl/engine_tests.rs` that construct Chrome render-path websites with `chrome_remote_local_policy = false` and `true`.

Assertions:
- Default config leaves remote-local-policy disabled.
- Enabled config calls Spider's `RequestInterceptConfiguration::set_remote_local_policy(true)`.
- Existing SSRF/local protections remain active with both values: local/private URL blacklist patterns are still applied, and local/private discovered links are still rejected by Axon's existing URL validation path.

If Spider does not expose a direct getter for the policy, test via a narrow helper in `runtime.rs` that builds the `RequestInterceptConfiguration` from `Config` and can be inspected in tests.

- [x] **Step 2: Wire runtime**

In `runtime.rs`, replace direct construction with a helper:

```rust
fn chrome_intercept_config(cfg: &Config) -> RequestInterceptConfiguration {
    let mut intercept = RequestInterceptConfiguration::new(true);
    if cfg.chrome_remote_local_policy {
        intercept.set_remote_local_policy(true);
    }
    intercept
}
```

Then pass `chrome_intercept_config(cfg)` to `website.with_chrome_intercept(...)`.

- [x] **Step 3: Document failure mode**

Docs must state:

- This flag is only for capable remote Chrome engines that support Spider/Chromey's policy push.
- Generic CDP proxies may reject the underlying command.
- The flag applies to crawl Chrome render paths only. It does not apply to `axon screenshot` in this release.

Run:

```bash
cargo test crawl::engine_tests -- --nocapture
```

---

## Task 3: Adaptive Crawl Control Module

**Files:**
- `src/crawl/engine/adaptive.rs`
- `src/crawl/engine.rs`
- `src/crawl/engine/collector.rs`
- `src/crawl/engine/collector/types.rs` or equivalent collector config file
- `src/crawl/engine_tests.rs`

- [x] **Step 1: Add failing unit tests for adaptive behavior**

Create tests in `src/crawl/engine/adaptive.rs` under `#[cfg(test)]`.

Required tests:
- Disabled config returns `None`.
- Enabled config attaches an adaptive semaphore with the resolved max permits.
- Ten `200` statuses increase the controller target by one using Spider's fixed success threshold.
- One `429` decreases the target.
- One `503` decreases the target.
- `record_broadcast_lag(10)` applies negative pressure and decreases the target.
- Shrink convergence: when the controller shrinks below current in-flight permits, new admission is limited only after permits are released. Document this in the test name and assertion.
- The controller resizes the same semaphore attached to the `Website`, not a detached test semaphore.

Implementation note: Spider 2.52.0's `AdaptiveSemaphore::set_target()` only forgets permits that are available at resize time. When all permits are in flight, release can temporarily return availability above the shrunken target; the test documents this observed behavior instead of asserting cancellation or retroactive permit forgetting.

- [x] **Step 2: Implement `AdaptiveCrawlControl`**

Add `src/crawl/engine/adaptive.rs`.

Shape:

```rust
use std::sync::Arc;

use spider::utils::adaptive_concurrency::{AIMDController, AdaptiveSemaphore};
use spider::website::Website;

use crate::core::config::Config;

const ADAPTIVE_INCREASE_THRESHOLD: usize = 10;
const ADAPTIVE_DECREASE_FACTOR: f64 = 0.5;

#[derive(Clone)]
pub(crate) struct AdaptiveCrawlControl {
    semaphore: AdaptiveSemaphore,
    controller: Arc<AIMDController>,
    // Include atomics for successes, failures, lag_events, syncs, and last_target.
}
```

Implementation rules:

- `from_config(cfg: &Config) -> Option<Self>` returns `None` unless enabled.
- Initial target should be `cfg.crawl_concurrency_limit.unwrap_or(resolved_max).clamp(min, max)`.
- `attach_to(&self, website: &mut Website)` calls `website.with_adaptive_concurrency(self.semaphore.clone())`.
- `record_status(status: u16)` treats `status == 429 || status >= 500` as failure; all other statuses are success.
- `record_broadcast_lag(dropped: u64)` records at least one failure and at most eight failures per lag event, then syncs.
- After each recorded outcome, compare previous target to current target and call `self.semaphore.sync_from(&self.controller)` only when the target changed. This avoids a configurable sync interval while keeping runtime behavior responsive.
- `snapshot()` returns raw counters and current target for logs/tests.

Do not put adaptive controller logic in `runtime.rs`.

- [x] **Step 3: Attach in the crawl lifecycle**

In `src/crawl/engine.rs`:

- Keep `runtime::configure_website()` and `runtime::configure_website_with_crawl_id()` returning bare `Website`.
- After constructing the crawl `Website`, call `let adaptive = AdaptiveCrawlControl::from_config(cfg);`.
- If present, call `adaptive.attach_to(&mut website)`.
- Pass `adaptive.clone()` through `CollectorConfig`.
- Emit startup warnings from `adaptive::warnings_for_config(cfg)` when adaptive is enabled and the crawl is unbounded or impolite:
  - `respect_robots == false`
  - `delay_ms == 0`
  - `max_pages == 0`
  - no path budgets and no URL whitelist

Warnings should not fail the crawl in this release.

- [x] **Step 4: Record collector feedback**

In `src/crawl/engine/collector.rs`:

- On every page with a status code, call `collector_config.adaptive.record_status(status)`.
- Keep the existing 429 operator-facing warning.
- In the `RecvError::Lagged(n)` branch, call `collector_config.adaptive.record_broadcast_lag(n)`.
- At crawl completion, include adaptive stats in diagnostics/log output. Avoid changing public JSON contracts unless an existing diagnostics field can carry this safely.

- [x] **Step 5: Add local-server integration coverage**

Add an integration-style test using an in-process `axum::Router` or existing local test-server helper.

Scenario:
- Seed page links to three pages.
- Pages return `200`, `429`, and `503`.
- Run a bounded HTTP crawl with adaptive enabled, `min = 1`, `max = 8`, and `embed = false`.
- Assert the crawl completes, the existing 429 warning path still runs, and adaptive stats show at least two failures with target below the starting target.

Implementation note: the first full local HTTP crawl proof using a loopback-resolving host produced zero Spider pages in this worktree, which made the test validate local routing rather than adaptive feedback. The final coverage feeds mocked HTTP pages through `process_received_page`, exercising the same collector status path that records the 429 warning and 5xx adaptive failures while avoiding DNS/SSRF local-network ambiguity.

Run:

```bash
cargo test crawl::engine_tests adaptive -- --nocapture
```

---

## Task 4: Documentation and Operator Contracts

**Files:**
- `config.example.toml`
- `docs/guides/configuration.md`
- `docs/operations/performance.md`
- `docs/reference/spider-feature-flags.md`
- `CLAUDE.md`

- [x] **Step 1: Update sample config**

Add to `config.example.toml`:

```toml
[workers.adaptive-concurrency]
# Off by default. When enabled, Axon replaces the fixed crawl semaphore with
# Spider's adaptive semaphore. Failure decrease is fixed at 0.5 in Spider 2.52.0.
enabled = false
min = 1
# max defaults to the resolved crawl concurrency limit. Explicit values are capped
# by min(crawl-broadcast-buffer-max, 1024).
# max = 64

[chrome]
# Push Spider/Chromey's local policy to capable remote Chrome engines.
# Generic CDP proxies may reject this command.
remote-local-policy = false
```

If `[chrome]` already exists, merge the setting into it.

- [x] **Step 2: Update docs**

Document:

- Adaptive concurrency is TOML-only in this release.
- Default behavior is unchanged.
- Adaptive mode applies to the main Spider crawl path. Post-crawl sitemap backfill, standalone screenshot, and non-Spider fetch helpers remain governed by their existing fixed limits unless separately wired later.
- 429 and 5xx responses reduce concurrency; successful statuses increase after Spider's fixed success threshold.
- Shrink affects future admission and may not cancel already in-flight fetches.
- Broadcast lag is treated as negative pressure.
- Operators should pair adaptive mode with polite crawl settings: robots, delay, max pages, path budgets, or whitelist.
- `decrease-factor`, `sync-interval-ms`, and palette editing are intentionally not supported in this release.
- Remote-local-policy is Chrome-render-crawl only and may fail on generic CDP proxies.

- [x] **Step 3: Update `CLAUDE.md`**

Add a short gotcha under crawl/performance notes:

- Adaptive crawl concurrency is opt-in via `[workers.adaptive-concurrency]`.
- Do not add arbitrary Spider adaptive knobs until Spider actually honors them.
- Keep adaptive controller logic in `src/crawl/engine/adaptive.rs`, not `runtime.rs`.

Ensure sibling `AGENTS.md` and `GEMINI.md` are symlinks to `CLAUDE.md` if they exist in this directory.

---

## Task 5: Verification

- [x] **Step 1: Focused tests**

Run:

```bash
cargo test config::parse::build_config_tests::priority_chain::workers_search -- --nocapture
cargo test jobs::config_snapshot -- --nocapture
cargo test crawl::engine_tests -- --nocapture
cargo test crawl::engine::adaptive -- --nocapture
```

- [x] **Step 2: Build**

Run:

```bash
cargo check --all-targets
```

- [x] **Step 3: Runtime smoke**

Run:

```bash
./scripts/axon crawl https://example.com --wait true --max-pages 1 --embed false
```

Then run the same smoke with a temporary config enabling adaptive mode. The smoke must finish and preserve default behavior when adaptive is disabled.

---

## Not In Scope

- CLI flags for these settings.
- Palette config editor exposure.
- Spider `auto_throttle`.
- Arbitrary `decrease-factor`.
- `sync-interval-ms` or background reconciliation.
- Standalone `axon screenshot` remote-local-policy support.
- Full web UI telemetry.

---

## Engineering Review Changes Applied

- Removed misleading adaptive knobs: `initial`, `decrease-factor`, `increase-threshold`, `sync-interval-ms`, and `failure-status-threshold`.
- Removed CLI and palette scope from the first release.
- Moved adaptive behavior into a dedicated `adaptive.rs` module.
- Required `429`, `5xx`, and broadcast lag to reduce concurrency.
- Required proof that the controller resizes the same semaphore attached to Spider's `Website`.
- Added max-cap validation against `min(crawl-broadcast-buffer-max, 1024)`.
- Added config snapshot/replay requirements.
- Added SSRF/private-network regression coverage for Chrome remote-local-policy.
- Added warnings for adaptive mode combined with unbounded or impolite crawl settings.
