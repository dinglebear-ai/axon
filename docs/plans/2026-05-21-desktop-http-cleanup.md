# Desktop HTTP Cleanup — Health Probe, Server-Down UX, Dead Code Removal

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete the desktop palette HTTP-API migration by switching the health probe to `/healthz`, surfacing a clear server-unreachable notice instead of a cryptic error, and removing dead subprocess scaffolding.

**Architecture:** The palette already dispatches every user-invoked command through `rest_client.rs` → typed REST endpoints. Three gaps remain: (1) the background health-dot probe hits the auth-guarded `/v1/doctor` instead of the unguarded `/healthz`; (2) when the server is unreachable the user sees a misnamed `spawn_error` card rather than a helpful "Start `axon serve`" notice; (3) `output.rs` still carries `#[cfg(test)]`-gated subprocess scaffolding (`from_process`, `BoundedProcessOutput`, `BoundedByteBuffer`) that is dead path relative to the current HTTP-only runtime.

**Tech Stack:** Rust 2024 edition, GPUI, reqwest (blocking), serde\_json. Files are all under `apps/desktop/src/`. Build: `cd apps/desktop && cargo build`. Tests: `cd apps/desktop && cargo test`.

---

## File Map

| File | Change type | What changes |
|---|---|---|
| `apps/desktop/src/rest_client.rs` | Modify | Add `health_probe_request()` helper returning `GET /healthz` without going through `build_rest_request` |
| `apps/desktop/src/ui_commands.rs` | Modify | (1) Switch `spawn_health_check` to use `health_probe_request`; (2) add pre-submit server-down guard; (3) map connection errors in `finalize_result` to a server-unreachable notice |
| `apps/desktop/src/output.rs` | Modify | Remove dead subprocess code: `from_process`, `BoundedProcessOutput`, `BoundedByteBuffer`, `valid_utf8_boundary`, `success_status`. Rename `spawn_error` → `request_error`. Remove unused `#[cfg(test)]` imports of subprocess formatting helpers |
| `apps/desktop/src/output_tests.rs` | Modify | Remove tests that relied on `from_process`/`BoundedProcessOutput` (two tests: `ingest_summary_suggests_status_for_async_job`, `successful_process_output_drops_progress_stderr`). Keep all other tests. |
| `apps/desktop/src/rest_client_tests.rs` | Modify | Update the doctor health-check test (if it asserts `/v1/doctor`). Add a test for `health_probe_request` asserting `GET /healthz` |

---

## Task 1: Add `health_probe_request()` to `rest_client.rs`

**Files:**
- Modify: `apps/desktop/src/rest_client.rs`
- Test: `apps/desktop/src/rest_client_tests.rs`

The current `spawn_health_check` constructs a synthetic `Doctor` action and calls `build_rest_request`. This is wrong for two reasons: (a) `/v1/doctor` is auth-guarded on the server — a user without `AXON_MCP_HTTP_TOKEN` gets a 401 and the dot stays red even against a healthy server; (b) the health probe is conceptually separate from the user-invoked `doctor` command. The `/healthz` endpoint (`web/health.rs`) returns `200 ok` with no auth.

- [ ] **Step 1.1: Write the failing test**

  Add to the bottom of `apps/desktop/src/rest_client_tests.rs`:

  ```rust
  #[test]
  fn health_probe_is_get_healthz_with_no_body() {
      let request = health_probe_request();
      assert_eq!(request.method, "GET");
      assert_eq!(request.path, "/healthz");
      assert!(request.body.is_none());
  }
  ```

  Note: `health_probe_request` is not yet defined, so you must also add it to the `use super::*;` scope by ensuring `rest_client_tests.rs` uses `super::*`. The existing tests already do this (line 1: `use super::*;`). The new function just needs to be `pub(crate)` in `rest_client.rs`.

- [ ] **Step 1.2: Run test to confirm it fails**

  ```bash
  cd /home/jmagar/workspace/axon_rust/apps/desktop && cargo test health_probe_is_get_healthz 2>&1 | tail -20
  ```

  Expected: compile error — `health_probe_request` not found.

- [ ] **Step 1.3: Add `health_probe_request()` to `rest_client.rs`**

  Add after the `get()` helper (around line 281):

  ```rust
  /// Returns a lightweight `GET /healthz` request for the background health-dot probe.
  ///
  /// `/healthz` requires no auth and returns `200 ok` from a running `axon serve`.
  /// This is deliberately separate from the user-invoked `doctor` action, which hits
  /// the auth-guarded `/v1/doctor` endpoint and returns richer diagnostic JSON.
  pub(crate) fn health_probe_request() -> RestRequest {
      get("/healthz", "GET /healthz")
  }
  ```

- [ ] **Step 1.4: Run test to confirm it passes**

  ```bash
  cd /home/jmagar/workspace/axon_rust/apps/desktop && cargo test health_probe_is_get_healthz 2>&1 | tail -10
  ```

  Expected: `test health_probe_is_get_healthz_with_no_body ... ok`

- [ ] **Step 1.5: Commit**

  ```bash
  cd /home/jmagar/workspace/axon_rust && rtk git add apps/desktop/src/rest_client.rs apps/desktop/src/rest_client_tests.rs && rtk git commit -m "feat(desktop): add health_probe_request() for GET /healthz"
  ```

---

## Task 2: Switch health-dot probe to `/healthz`

**Files:**
- Modify: `apps/desktop/src/ui_commands.rs`
- Test: `apps/desktop/src/rest_client_tests.rs` (already done in Task 1)

The `spawn_health_check` method (lines 23–65 of `ui_commands.rs`) currently builds a synthetic `CommandAction { subcommand: "doctor", ... }` and calls `build_rest_request`. Replace the whole probe body with `health_probe_request()`.

- [ ] **Step 2.1: Replace the probe logic in `ui_commands.rs`**

  Find this block inside `spawn_health_check` (lines 29–46):

  ```rust
  let task = cx.background_spawn(async move {
      let ok = RestClient::from_env()
          .and_then(|client| {
              let request = build_rest_request(
                  crate::actions::CommandAction {
                      label: "Doctor",
                      subcommand: "doctor",
                      arg_mode: ArgMode::None,
                      aliases: &[],
                      description: "",
                      example: "",
                  },
                  "",
              )?;
              client.execute(&request)
          })
          .map(|output| output.ok)
          .unwrap_or(false);
      HealthResult { ok }
  });
  ```

  Replace it with:

  ```rust
  let task = cx.background_spawn(async move {
      let ok = RestClient::from_env()
          .and_then(|client| {
              let request = crate::rest_client::health_probe_request();
              client.execute(&request)
          })
          .map(|output| output.ok)
          .unwrap_or(false);
      HealthResult { ok }
  });
  ```

  Also remove the now-unused import of `ArgMode` at the top of `ui_commands.rs` **if** it is only used for this synthetic action. Check — `ArgMode` is still used in `submit()` (line 105: `matches!(action.arg_mode, ArgMode::Single | ArgMode::Split)`). Keep the import.

- [ ] **Step 2.2: Verify compilation**

  ```bash
  cd /home/jmagar/workspace/axon_rust/apps/desktop && cargo check 2>&1 | tail -20
  ```

  Expected: no errors.

- [ ] **Step 2.3: Run all tests**

  ```bash
  cd /home/jmagar/workspace/axon_rust/apps/desktop && rtk cargo test 2>&1 | tail -20
  ```

  Expected: all pass.

- [ ] **Step 2.4: Commit**

  ```bash
  cd /home/jmagar/workspace/axon_rust && rtk git add apps/desktop/src/ui_commands.rs && rtk git commit -m "fix(desktop): health dot uses GET /healthz instead of auth-guarded /v1/doctor"
  ```

---

## Task 3: Add pre-submit server-down guard and connection-error UX

**Files:**
- Modify: `apps/desktop/src/ui_commands.rs`

Currently `submit()` fires the REST request regardless of `self.connection`. If the server is unreachable, the error propagates to `finalize_result` which calls `CommandOutput::spawn_error` (misnamed — it's now a network error, not a spawn failure). The user sees a cryptic "Could not start axon" card.

Two changes:
1. **Pre-submit guard:** if `connection == Disconnected`, show a clear notice and skip the request.
2. **Post-request connection-error mapping:** if `RestClient::execute` returns an `Err(...)` that indicates the server is unreachable (reqwest connect error — the string contains "connect"), show the server-down notice and flip `self.connection = Disconnected` to keep the dot red.

- [ ] **Step 3.1: Write the failing test**

  Add to `apps/desktop/src/ui_tests.rs` (or create if needed — `ui_tests.rs` already exists per the file listing). Alternatively, this logic is simpler to cover with an integration-style check in `ui_commands.rs` itself. Since `submit` and `finalize_result` are tightly coupled to GPUI context, write a pure unit test for the new helper function `is_server_unreachable_error` that will be extracted:

  Add to `apps/desktop/src/ui_tests.rs`:

  ```rust
  use crate::ui_commands::is_server_unreachable_error;

  #[test]
  fn connect_error_is_classified_as_unreachable() {
      assert!(is_server_unreachable_error("connect to http://127.0.0.1:8001: connection refused"));
  }

  #[test]
  fn auth_error_is_not_classified_as_unreachable() {
      assert!(!is_server_unreachable_error("HTTP 401 Unauthorized"));
  }
  ```

  Note: `ui_tests.rs` is currently small. Add this import block at the top if missing:

  ```rust
  use super::*;
  ```

- [ ] **Step 3.2: Run test to confirm it fails**

  ```bash
  cd /home/jmagar/workspace/axon_rust/apps/desktop && cargo test is_server_unreachable 2>&1 | tail -20
  ```

  Expected: compile error — `is_server_unreachable_error` not found, or `ui_commands` module not accessible.

- [ ] **Step 3.3: Add `is_server_unreachable_error` to `ui_commands.rs`**

  Add near the top of `ui_commands.rs` (before `impl Palette`), after the `use` block:

  ```rust
  /// Returns `true` when a REST error string indicates the server is not running
  /// (TCP connection refused / DNS failure / network unreachable). Used to show
  /// a helpful "Start `axon serve`" notice rather than a generic request-error card.
  pub(crate) fn is_server_unreachable_error(error: &str) -> bool {
      let lower = error.to_ascii_lowercase();
      lower.contains("connection refused")
          || lower.contains("connect error")
          || lower.contains("failed to connect")
          || lower.contains("no route to host")
          || lower.contains("network unreachable")
          || lower.contains("os error 111") // Linux ECONNREFUSED
          || lower.contains("os error 10061") // Windows WSAECONNREFUSED
  }
  ```

- [ ] **Step 3.4: Run test to confirm it passes**

  ```bash
  cd /home/jmagar/workspace/axon_rust/apps/desktop && cargo test is_server_unreachable 2>&1 | tail -10
  ```

  Expected: both new tests pass.

- [ ] **Step 3.5: Add pre-submit guard in `submit()`**

  In `submit()`, find the block that checks for `self.running.is_some()` (around line 115 in `ui_commands.rs`). **Before** that check, insert:

  ```rust
  // Guard: if the health dot is red, skip the request and show a clear notice
  // rather than a cryptic network-error card.
  if self.connection == ConnectionState::Disconnected {
      self.command_output = Some(CommandOutput::notice(
          OutputKind::Warning,
          "Server unreachable",
          "Start `axon serve` to enable commands. Click the dot to retry.",
      ));
      cx.notify();
      return;
  }
  ```

  Place this immediately after the `arg_mode` empty-arg guard and before the `self.running.is_some()` check. The full ordering in `submit()` should be:
  1. `ask-reset` sentinel
  2. `settings` sentinel
  3. empty-arg guard
  4. **server-down guard** (new)
  5. already-running guard
  6. `build_rest_request` + dispatch

- [ ] **Step 3.6: Update `finalize_result` to detect connection errors**

  Find `finalize_result` (around line 220 in `ui_commands.rs`):

  ```rust
  fn finalize_result(&mut self, result: CommandResult) -> CommandOutput {
      match result.result {
          Ok(output) => CommandOutput::from_rest(&result.command_line, result.subcommand, output),
          Err(error) => CommandOutput::spawn_error(&result.command_line, error),
      }
  }
  ```

  Replace with:

  ```rust
  fn finalize_result(&mut self, result: CommandResult) -> CommandOutput {
      match result.result {
          Ok(output) => CommandOutput::from_rest(&result.command_line, result.subcommand, output),
          Err(error) => {
              if is_server_unreachable_error(&error) {
                  // Mark the dot red so the next pre-submit guard fires immediately.
                  self.connection = ConnectionState::Disconnected;
                  CommandOutput::notice(
                      OutputKind::Warning,
                      "Server unreachable",
                      "Start `axon serve` to enable commands. Click the dot to retry.",
                  )
              } else {
                  CommandOutput::request_error(&result.command_line, error)
              }
          }
      }
  }
  ```

  Note: this references `request_error`, which will be added in Task 4 (rename of `spawn_error`). For now keep `spawn_error` in the branch — you'll update this in Task 4 once the rename lands.

- [ ] **Step 3.7: Verify compilation**

  ```bash
  cd /home/jmagar/workspace/axon_rust/apps/desktop && cargo check 2>&1 | tail -20
  ```

  Expected: no errors.

- [ ] **Step 3.8: Run all tests**

  ```bash
  cd /home/jmagar/workspace/axon_rust/apps/desktop && rtk cargo test 2>&1 | tail -20
  ```

  Expected: all pass.

- [ ] **Step 3.9: Commit**

  ```bash
  cd /home/jmagar/workspace/axon_rust && rtk git add apps/desktop/src/ui_commands.rs apps/desktop/src/ui_tests.rs && rtk git commit -m "feat(desktop): server-unreachable notice instead of cryptic error; pre-submit connection guard"
  ```

---

## Task 4: Remove dead subprocess scaffolding and rename `spawn_error`

**Files:**
- Modify: `apps/desktop/src/output.rs`
- Modify: `apps/desktop/src/output_tests.rs`
- Modify: `apps/desktop/src/ui_commands.rs` (final reference update)

The following items in `output.rs` are dead code left over from the subprocess-spawn era:

- `#[cfg(test)] pub(crate) struct BoundedProcessOutput` — subprocess stdout/stderr capture
- `#[cfg(test)] impl CommandOutput { fn from_process(...) }` — builds a `CommandOutput` from a process exit
- `#[cfg(test)] struct BoundedByteBuffer` — bounded byte capture buffer (no longer needed)
- `#[cfg(test)] fn valid_utf8_boundary(...)` — helper for the byte buffer
- `#[cfg(test)] fn success_status()` — cross-platform success `ExitStatus` factory
- `#[cfg(test)]` imports of `format_exit_status`, `actionable_error_text`, `ask_answer`, `crawl_summary`, `drop_cli_scaffolding`, `palette_output_text`, `scrape_body`, `strip_ansi` (lines 13–16) — these are used only by `from_process` and the now-deleted tests

`spawn_error` is still live (called from `ui_commands.rs`), but its name is now wrong. Rename it to `request_error`.

The following `output_tests.rs` tests depend on `from_process` / `BoundedProcessOutput` and must be removed:
- `ingest_summary_suggests_status_for_async_job` (line 211–227)
- `successful_process_output_drops_progress_stderr` (line 229–247)

All other tests in `output_tests.rs` use `strip_ansi`, `map_url_listing`, `rest_output_text`, `OutputSection`, etc. and must be kept.

- [ ] **Step 4.1: Identify the exact lines to remove in `output.rs`**

  The items to delete are:

  a. Lines 2 (`use std::process::ExitStatus;`) — this import is only used by `from_process` and `success_status`.
  b. Lines 13–16 (the `#[cfg(test)]` use block with subprocess formatting helpers: `actionable_error_text`, `ask_answer`, `crawl_summary`, `drop_cli_scaffolding`, `format_exit_status`, `palette_output_text`, `scrape_body`, `strip_ansi`).
  c. The `pub(crate) fn spawn_error(...)` at line 98 → rename to `request_error` and update the title string.
  d. The `#[cfg(test)] pub(crate) fn from_process(...)` block (lines 110–155).
  e. The `#[cfg(test)] struct BoundedByteBuffer` block (lines 299–337).
  f. The `#[cfg(test)] fn valid_utf8_boundary(...)` helper (lines 339–344).
  g. The `#[cfg(test)] fn success_status()` helper (lines 367–382).

  Items b are re-exported from `formatting.rs` into `output.rs` under `#[cfg(test)]`. After deleting them from `output.rs`, confirm they are still accessible to `output_tests.rs` via `use super::*;` — they will be, because `output_tests.rs` uses `super::*` and the formatting module exports them to the parent module scope. Wait — actually look at this carefully:

  In `output.rs` lines 13–16:
  ```rust
  #[cfg(test)]
  use formatting::{
      actionable_error_text, ask_answer, crawl_summary, drop_cli_scaffolding, format_exit_status,
      palette_output_text, scrape_body, strip_ansi,
  };
  ```

  These bring `formatting::*` helpers into the `output` module scope under `#[cfg(test)]`. The `output_tests.rs` tests call `strip_ansi(...)`, `map_url_listing(...)`, `rest_output_text(...)`, etc. directly through `use super::*;`.

  After removing this import block, the tests that call `strip_ansi`, `crawl_summary`, etc. will fail to compile. To preserve the surviving tests: move the needed items to a narrower `#[cfg(test)]` import that excludes the subprocess-only items:

  Replace lines 13–16 with:

  ```rust
  #[cfg(test)]
  use formatting::{
      actionable_error_text, ask_answer, crawl_summary, drop_cli_scaffolding,
      palette_output_text, scrape_body, strip_ansi,
  };
  ```

  (Remove only `format_exit_status` from the list, since that's only used in the deleted `from_process` method and the deleted test `successful_process_output_drops_progress_stderr`.)

  Verify `format_exit_status` is not referenced anywhere else in `output.rs`:

  ```bash
  grep -n "format_exit_status" /home/jmagar/workspace/axon_rust/apps/desktop/src/output.rs
  ```

- [ ] **Step 4.2: Rename `spawn_error` to `request_error` in `output.rs`**

  Find (around line 98):

  ```rust
  pub(crate) fn spawn_error(command_line: &str, error: String) -> Self {
      Self {
          kind: OutputKind::Error,
          title: "Could not start axon".to_string(),
          subtitle: command_line.to_string(),
          stdout: None,
          stderr: Some(OutputSection::new("spawn error", error)),
          use_markdown: false,
          compact_stdout: false,
      }
  }
  ```

  Replace with:

  ```rust
  pub(crate) fn request_error(command_line: &str, error: String) -> Self {
      Self {
          kind: OutputKind::Error,
          title: "Request failed".to_string(),
          subtitle: command_line.to_string(),
          stdout: None,
          stderr: Some(OutputSection::new("error", error)),
          use_markdown: false,
          compact_stdout: false,
      }
  }
  ```

- [ ] **Step 4.3: Remove dead `#[cfg(test)]` items from `output.rs`**

  Remove all of the following blocks from `output.rs`:
  - `use std::process::ExitStatus;` (line 2)
  - `pub(crate) fn from_process(...)` and its entire body (the `#[cfg(test)]` impl block at lines ~110–155)
  - `struct BoundedByteBuffer` and its `impl BoundedByteBuffer` (the `#[cfg(test)]` items)
  - `fn valid_utf8_boundary(...)` (`#[cfg(test)]`)
  - `fn success_status()` (`#[cfg(test)]`)

  After removing `from_process`, the `#[cfg(test)] impl CommandOutput` block will be empty. Remove the block wrapper entirely.

  The remaining `#[cfg(test)]` live items that must stay:
  - `pub(crate) struct BoundedProcessOutput` → **delete** (only used by deleted `from_process`)
  - The `use formatting::{...}` import block → **keep**, updated to remove `format_exit_status`

  Wait: re-examine `BoundedProcessOutput`. It is used in the two deleted `output_tests.rs` tests (`ingest_summary_suggests_status_for_async_job` and `successful_process_output_drops_progress_stderr`). After those tests are deleted, `BoundedProcessOutput` is unused. Delete it too.

- [ ] **Step 4.4: Remove the two dead tests from `output_tests.rs`**

  Delete these two test functions from `apps/desktop/src/output_tests.rs`:

  - `fn ingest_summary_suggests_status_for_async_job()` (lines 211–227)
  - `fn successful_process_output_drops_progress_stderr()` (lines 229–247)

- [ ] **Step 4.5: Update the call site in `ui_commands.rs`**

  In `finalize_result` (from Task 3), update the fallback branch to use `request_error`:

  Change:
  ```rust
  CommandOutput::request_error(&result.command_line, error)
  ```

  It's already `request_error` from Step 3.6 — confirm it compiles now that the method exists.

- [ ] **Step 4.6: Verify compilation and tests**

  ```bash
  cd /home/jmagar/workspace/axon_rust/apps/desktop && cargo check 2>&1 | tail -20
  ```

  Expected: no errors.

  ```bash
  cd /home/jmagar/workspace/axon_rust/apps/desktop && rtk cargo test 2>&1 | tail -30
  ```

  Expected: all remaining tests pass; deleted tests are gone.

- [ ] **Step 4.7: Commit**

  ```bash
  cd /home/jmagar/workspace/axon_rust && rtk git add apps/desktop/src/output.rs apps/desktop/src/output_tests.rs apps/desktop/src/ui_commands.rs && rtk git commit -m "refactor(desktop): remove dead subprocess scaffolding; rename spawn_error → request_error"
  ```

---

## Task 5: File follow-up beads and bump version

**Files:**
- Modify: `apps/desktop/Cargo.toml` (version bump)

This task completes the bead and creates follow-up tracking for the deferred work items.

- [ ] **Step 5.1: Close bead `axon_rust-j19t`**

  ```bash
  bd close axon_rust-j19t
  ```

- [ ] **Step 5.2: Create follow-up bead for SSE streaming on `/v1/ask`**

  ```bash
  bd create --title "feat(desktop): SSE streaming for /v1/ask in the palette" \
    --description "The /v1/ask server endpoint is non-streaming (returns full JSON). To support incremental display in the palette, the server needs a text/event-stream path, and rest_client.rs needs an async SSE consumer. Blocks on server-side implementation. See axon_rust-j19t for context."
  ```

- [ ] **Step 5.3: Create follow-up bead for cancel endpoint**

  ```bash
  bd create --title "feat(desktop): cancel button calls /v1/cancel or job-specific cancel endpoint" \
    --description "The palette has no cancel button wired yet. Needs a /v1/cancel endpoint or per-job cancel path on the server, then a Cancel action in the palette that fires during the running state. See axon_rust-j19t acceptance criteria."
  ```

- [ ] **Step 5.4: Bump desktop version (patch)**

  Edit `apps/desktop/Cargo.toml` — change `version = "0.3.1"` to `version = "0.3.2"`.

- [ ] **Step 5.5: Final verify and push**

  ```bash
  cd /home/jmagar/workspace/axon_rust/apps/desktop && rtk cargo test 2>&1 | tail -20
  ```

  Expected: all pass.

  ```bash
  cd /home/jmagar/workspace/axon_rust && rtk git add apps/desktop/Cargo.toml && rtk git commit -m "chore(desktop): bump version to 0.3.2 after HTTP cleanup"
  ```

  ```bash
  cd /home/jmagar/workspace/axon_rust && rtk git pull --rebase && rtk git push
  ```

---

## Acceptance Criteria Checklist

- [ ] `spawn_health_check` uses `GET /healthz` (no auth required) — a palette with no token configured shows Connected against a healthy server
- [ ] `build_rest_request("doctor", ...)` still routes to `/v1/doctor` (user-invoked doctor command unchanged)
- [ ] When `ConnectionState::Disconnected`, submitting any command shows "Server unreachable — Start `axon serve` to enable commands" instead of firing the request
- [ ] When `RestClient::execute` returns a connect error, the dot flips to red and the same notice is shown
- [ ] `CommandOutput::spawn_error` is gone; `CommandOutput::request_error` is its replacement with an accurate title
- [ ] `from_process`, `BoundedProcessOutput`, `BoundedByteBuffer`, `valid_utf8_boundary`, `success_status` are removed from `output.rs`
- [ ] `ingest_summary_suggests_status_for_async_job` and `successful_process_output_drops_progress_stderr` are removed from `output_tests.rs`
- [ ] All remaining tests pass: `cd apps/desktop && cargo test` exits 0
- [ ] Desktop version bumped to `0.3.2`

---

## Test Strategy

Tests are all in `apps/desktop` (separate workspace). Run them with:

```bash
cd /home/jmagar/workspace/axon_rust/apps/desktop && cargo test
```

No network or Docker is required — all tests are pure unit tests over data transformations and request builders. The health probe and server-down guard path cannot be tested with live reqwest calls in unit tests; the acceptance test is manual: run `axon-palette` without `axon serve` running and confirm the status dot shows red and submit shows the notice card.

## Self-Review

**Spec coverage:**

| Bead AC | Task |
|---|---|
| `spawn_health_check` → `GET /healthz` | Tasks 1 + 2 |
| Submit uses typed REST endpoints (not `/v1/actions`) | Already done — no change needed |
| Read `AXON_SERVER_URL` and `AXON_MCP_HTTP_TOKEN` from env at startup | Already done — no change needed |
| Server-unreachable notice instead of silent fallthrough | Task 3 |
| Remove `axon_command()` Windows-stopgap | Already done — not present |
| Ask SSE streaming | Deferred to follow-up bead (Task 5) |
| Cancel button → endpoint | Deferred to follow-up bead (Task 5) |

**Gaps:** None — all in-scope AC items are covered. SSE and cancel are explicitly deferred per bead instructions ("file separately if not yet implemented").

**Placeholder scan:** No TBD, TODO, or "similar to Task N" references. All code blocks are complete.

**Type consistency:** `request_error` is introduced in Task 4 and referenced in Task 3 Step 3.6. The ordering in the plan (Task 3 first) means `ui_commands.rs` briefly references `request_error` before it exists. The step notes say "keep `spawn_error` in the branch — you'll update this in Task 4 once the rename lands." Implementers should follow that guidance: in Task 3.6, write `spawn_error` (since it still exists), then change to `request_error` in Task 4.5. Both methods cannot coexist during the intermediate commits, so the correct sequence is: Task 3 uses `spawn_error`, Task 4 renames it and updates the call site atomically in a single commit.
