# Tasks: ACP Full Capabilities
**Spec**: acp-full-capabilities
**Granularity**: fine
**Total**: 78 tasks across 5 phases
**POC Milestone**: Task 1.28 — Terminal API functional (create/output/wait/kill/release), Diff rendering in mapping, SDK bumped, bridge split complete

---

## Phase 1 — Make It Work (POC)

### Group A: SDK Bump + Bridge Split (Prerequisites)

## Task 1.1 — Bump agent-client-protocol to 0.10.2 <!-- DONE -->

**Do**: In `Cargo.toml`, change `agent-client-protocol` from `"0.10.0"` to `"0.10.2"` (both workspace and direct dep lines). Run `cargo check` to confirm no breaking API changes.
**Files**: `Cargo.toml`
**Done when**: `cargo check` exits 0 with the new SDK version
**Verify**: `cargo check`
**Commit**: `feat(acp): bump agent-client-protocol to 0.10.2`
_Requirements: FR-030_ / _Design: Section 2_

## Task 1.2 — Create bridge module directory and move bridge.rs <!-- DONE -->

**Do**: Convert `crates/services/acp/bridge.rs` (527 lines) into a module: copy `bridge.rs` to `bridge/mod_tmp.rs`, create `crates/services/acp/bridge/` directory. Actually — per project rules (NO mod.rs), keep `bridge.rs` as the module root and create `crates/services/acp/bridge/` for submodules. Extract the `AcpRuntimeState` struct, `stop_reason_to_str`, and `finalize_successful_turn` into `crates/services/acp/bridge/state.rs`. Keep `AcpBridgeClient`, `validate_fs_path`, and `impl Client for AcpBridgeClient` in `bridge.rs`. Add `mod state;` and `pub(super) use state::*;` in `bridge.rs`.
**Files**: `crates/services/acp/bridge.rs` (modify), `crates/services/acp/bridge/state.rs` (create)
**Done when**: `cargo check` passes, `bridge.rs` is under 400 lines, `state.rs` exists with the extracted items
**Verify**: `cargo check`
**Commit**: `refactor(acp): split bridge.rs into bridge/ module with state.rs`
_Requirements: FR-007_ / _Design: Section 3_

## Task 1.3 — Create bridge/terminal.rs skeleton <!-- DONE -->

**Do**: Create `crates/services/acp/bridge/terminal.rs` with the skeleton structures: `TerminalId` (newtype around `String`), `TerminalState` (fields: `child: Option<tokio::process::Child>`, `output_buf: std::collections::VecDeque<u8>`, `exit_status: Option<std::process::ExitStatus>`), `TerminalManager` (field: `terminals: Rc<RefCell<HashMap<TerminalId, TerminalState>>>`). Add `mod terminal;` to `bridge.rs`. Impl `TerminalManager::new()`.
**Files**: `crates/services/acp/bridge/terminal.rs` (create), `crates/services/acp/bridge.rs` (add mod declaration)
**Done when**: `cargo check` passes, terminal types exist
**Verify**: `cargo check`
**Commit**: `feat(acp): add terminal module skeleton with TerminalId, TerminalState, TerminalManager`
_Requirements: FR-007_ / _Design: Section 3_

---

- [x] V1 [VERIFY] Quality checkpoint: `cargo fmt --check && cargo clippy && cargo check`
  - **Do**: Run quality commands and verify all pass
  - **Verify**: All commands exit 0
  - **Done when**: No lint errors, no type errors
  - **Commit**: `chore(acp): pass quality checkpoint` (if fixes needed)

---

### Group B: Terminal API Implementation (FR-001 through FR-006)

## Task 1.3b — [RED] Write failing test: create_terminal lifecycle (NFR-007 test 1) <!-- DONE -->

**Do**: In `crates/services/acp/bridge/terminal.rs` tests module, write a failing test `test_create_terminal_output_wait` that:
  - Calls `TerminalManager::create("echo", &["hello"], &cwd, DEFAULT_OUTPUT_BYTE_LIMIT)`
  - Calls `TerminalManager::output(&id)` after a short sleep
  - Calls `TerminalManager::wait_for_exit(&id)`
  - Asserts output contains "hello" and exit code is 0
This test MUST FAIL to compile or panic before Task 1.4's implementation exists.
**Files**: `crates/services/acp/bridge/terminal.rs`
**Done when**: Test exists and fails (compile error or runtime panic) — RED phase
**Verify**: `cargo test test_create_terminal_output_wait 2>&1 | grep -E 'FAILED|error'`
**Commit**: `test(acp): RED - add failing create_terminal lifecycle test`
_Requirements: FR-001, FR-002, FR-003_ / _Design: Section 4.1_

## Task 1.4 — Implement TerminalManager::create (FR-001) <!-- DONE -->

**Do**: Implement `TerminalManager::create(cmd: &str, args: &[String], cwd: &Path, byte_limit: usize) -> Result<TerminalId, String>`. Spawn `tokio::process::Command` with stdout/stderr piped, stdin null. Validate CWD is within session CWD boundary (reuse `validate_fs_path` logic). Generate a UUID `TerminalId`. Store `TerminalState` in the map. Spawn a `tokio::task::spawn_local` output reader task that reads stdout+stderr into the ring buffer (`VecDeque<u8>`, capped at `byte_limit` — default 256 KiB, drain front when over limit).
**Files**: `crates/services/acp/bridge/terminal.rs`
**Done when**: `create` method compiles and spawns a child process with ring buffer reader
**Verify**: `cargo check`
**Commit**: `feat(acp): implement TerminalManager::create with subprocess spawn and ring buffer`
_Requirements: FR-001_ / _Design: Section 4.1_

## Task 1.4.1 [FIX 1.4] Fix: Remove thread_local global, use instance Rc<RefCell> in TerminalManager <!-- DONE -->

**Do**: The current `terminal.rs` uses a `thread_local! { static TERMINALS: RefCell<HashMap<TerminalId, TerminalState>> }` global instead of an instance field. This violates session isolation (AC-1.7). Fix:
1. Remove the `thread_local! { static TERMINALS: ... }` block from `terminal.rs`
2. Ensure `TerminalManager` struct has `pub(crate) terminals: Rc<RefCell<HashMap<TerminalId, TerminalState>>>` field
3. Update `TerminalManager::new()` to initialize the instance field (not the thread-local)
4. Update `TerminalManager::create()` to use `self.terminals.borrow_mut()` instead of `TERMINALS.with(...)`
5. Ensure the `spawn_local` buffer task still works with the instance's Rc (clone the Rc for the task)
6. Run `cargo check` to verify
**Files**: `crates/services/acp/bridge/terminal.rs`
**Done when**: No thread_local in terminal.rs, `cargo check` passes, `TerminalManager` uses instance field
**Verify**: `cargo check && ! grep -q 'thread_local' crates/services/acp/bridge/terminal.rs`
**Commit**: `fix(acp): remove thread_local global, use instance Rc<RefCell> in TerminalManager`

## Task 1.5 — Implement TerminalManager::output (FR-002) <!-- DONE -->

**Do**: Implement `TerminalManager::output(id: &TerminalId) -> Result<(String, bool, Option<i32>), String>`. Drain the ring buffer into a UTF-8 string (lossy), return `(output_text, truncated_flag, exit_code_if_exited)`. The `truncated` flag is true if the buffer hit the byte limit and older data was dropped.
**Files**: `crates/services/acp/bridge/terminal.rs`
**Done when**: `output` method compiles and drains the buffer
**Verify**: `cargo check`
**Commit**: `feat(acp): implement TerminalManager::output with buffer drain`
_Requirements: FR-002_ / _Design: Section 4.1_

## Task 1.6 — Implement TerminalManager::wait_for_exit (FR-003) <!-- DONE -->

**Do**: Implement `TerminalManager::wait_for_exit(id: &TerminalId) -> Result<i32, String>`. If already exited, return cached exit code. Otherwise, take the `Child` handle, await `child.wait()`, store the `ExitStatus`, return the code. Use `Rc<RefCell>` carefully — take child out of the state, await, then write back the exit status.
**Files**: `crates/services/acp/bridge/terminal.rs`
**Done when**: `wait_for_exit` compiles, handles already-exited case
**Verify**: `cargo check`
**Commit**: `feat(acp): implement TerminalManager::wait_for_exit`
_Requirements: FR-003_ / _Design: Section 4.1_

## Task 1.6b — [GREEN] Verify create_terminal lifecycle test passes <!-- DONE -->

**Do**: Run `cargo test test_create_terminal_output_wait` and confirm it passes. If it fails, fix the implementation. This is the GREEN phase for the test written in Task 1.3b.
**Files**: (none if test passes; fix implementation files if failing)
**Done when**: `cargo test test_create_terminal_output_wait` exits 0
**Verify**: `cargo test test_create_terminal_output_wait`
**Commit**: `test(acp): GREEN - create_terminal lifecycle test passing`
_Requirements: FR-001, FR-002, FR-003_ / _Design: Section 4.1_

---

- [x] V2 [VERIFY] Quality checkpoint: `cargo fmt --check && cargo clippy && cargo check`
  - **Do**: Run quality commands and verify all pass
  - **Verify**: All commands exit 0
  - **Done when**: No lint errors, no type errors
  - **Commit**: `chore(acp): pass quality checkpoint` (if fixes needed)

---

## Task 1.6c — [RED] Write failing test: kill + release terminal (NFR-007 test 2) <!-- DONE -->

**Do**: Write `test_create_kill_release` in terminal.rs tests: create terminal running `sleep 60`, call kill, call release, assert terminal no longer exists in map.
**Files**: `crates/services/acp/bridge/terminal.rs`
**Done when**: Test exists and fails — RED phase
**Verify**: `cargo test test_create_kill_release 2>&1 | grep -E 'FAILED|error'`
**Commit**: `test(acp): RED - add failing kill+release terminal test`
_Requirements: FR-004, FR-005_ / _Design: Section 4.1_

## Task 1.6d — [RED] Write failing test: double-release is no-op (NFR-007 test 3) <!-- DONE -->

**Do**: Write `test_double_release_noop` in terminal.rs tests: create terminal, release twice, assert both calls return Ok.
**Files**: `crates/services/acp/bridge/terminal.rs`
**Done when**: Test exists and fails — RED phase
**Verify**: `cargo test test_double_release_noop 2>&1 | grep -E 'FAILED|error'`
**Commit**: `test(acp): RED - add failing double-release no-op test`
_Requirements: FR-005_ / _Design: Section 4.1_

## Task 1.6e [FIX 1.6c] Fix: Strengthen RED test assertions for kill+release <!-- DONE -->

**Do**: Improve the RED tests added in 1.6c/1.6d:
1. In `test_double_release_noop`: add `mgr.kill(&id).await.expect("kill")` before the first `release()` call
2. In `test_create_kill_release`: after `kill()`, add assertion that calls `mgr.wait_for_exit(&id).await` and asserts it returns Ok
3. In `test_double_release_noop`: add assertion that `mgr.terminals.borrow().contains_key(&id)` is false after first `release()`
**Files**: `crates/services/acp/bridge/terminal.rs`
**Done when**: Tests have improved assertions (still RED — kill/release don't exist yet)
**Verify**: `cargo test test_double_release_noop 2>&1 | grep -E 'FAILED|error'`
**Commit**: `test(acp): strengthen RED kill+release test assertions`

## Task 1.7 — Implement TerminalManager::kill (FR-004) <!-- DONE -->

**Do**: Implement `TerminalManager::kill(id: &TerminalId) -> Result<(), String>`. If process is running: send SIGTERM via `child.start_kill()`, then spawn a `spawn_local` task that waits 5 seconds and sends SIGKILL if still alive (`child.kill()`). If already exited, return Ok. On Unix, use `nix::sys::signal::kill(Pid, Signal::SIGTERM)` or `child.start_kill()` (tokio uses SIGKILL on Unix by default — need to send SIGTERM first via `unsafe { libc::kill(pid, libc::SIGTERM) }`).
**Files**: `crates/services/acp/bridge/terminal.rs`
**Done when**: `kill` method compiles with SIGTERM→SIGKILL escalation
**Verify**: `cargo check`
**Commit**: `feat(acp): implement TerminalManager::kill with SIGTERM→SIGKILL escalation`
_Requirements: FR-004_ / _Design: Section 4.1_

## Task 1.8 — Implement TerminalManager::release (FR-005) <!-- DONE -->

**Do**: Implement `TerminalManager::release(id: &TerminalId) -> Result<(), String>`. Call `kill(id)` if still running, then remove the entry from the HashMap. If the terminal doesn't exist (already released or never created), return Ok (idempotent — double-release is a no-op per NFR-007).
**Files**: `crates/services/acp/bridge/terminal.rs`
**Done when**: `release` method compiles, double-release returns Ok
**Verify**: `cargo check`
**Commit**: `feat(acp): implement TerminalManager::release (idempotent)`
_Requirements: FR-005_ / _Design: Section 4.1_

## Task 1.8b — [GREEN] Verify kill+release and double-release tests pass <!-- DONE -->

**Do**: Run `cargo test test_create_kill_release test_double_release_noop`. Fix any implementation issues.
**Files**: (fix terminal.rs if tests fail)
**Done when**: Both tests pass
**Verify**: `cargo test test_create_kill_release test_double_release_noop`
**Commit**: `test(acp): GREEN - kill+release and double-release tests passing`
_Requirements: FR-004, FR-005_ / _Design: Section 4.1_

---

- [ ] V3 [VERIFY] Quality checkpoint: `cargo fmt --check && cargo clippy && cargo check`
  - **Do**: Run quality commands and verify all pass
  - **Verify**: All commands exit 0
  - **Done when**: No lint errors, no type errors
  - **Commit**: `chore(acp): pass quality checkpoint` (if fixes needed)

---

### Group C: Wire Terminal into Bridge Client (FR-006, FR-007)

## Task 1.9 — Add TerminalManager to AcpBridgeClient (FR-006)

**Do**: Add `terminal_manager: Rc<RefCell<TerminalManager>>` field to `AcpBridgeClient` (in `bridge.rs` or `bridge/state.rs` depending on where the struct lives after split). Use `Rc<RefCell<...>>` per NFR-004. Initialize in all construction sites: `session.rs:initialize_connection` and the test helper.
**Files**: `crates/services/acp/bridge.rs`, `crates/services/acp/session.rs`
**Done when**: `cargo check` passes, `AcpBridgeClient` has `terminal_manager` field
**Verify**: `cargo check`
**Commit**: `feat(acp): add TerminalManager field to AcpBridgeClient`
_Requirements: FR-006_ / _Design: Section 4.1_

## Task 1.10 — Implement Client::create_terminal

**Do**: In the `impl Client for AcpBridgeClient` block, implement `create_terminal`. Extract `command`, `args`, and `working_directory` from the `CreateTerminalRequest`. Validate working_directory within session CWD. Delegate to `terminal_manager.borrow_mut().create(...)`. Return `CreateTerminalResponse` with the `TerminalId`.
**Files**: `crates/services/acp/bridge.rs`
**Done when**: `create_terminal` method compiles and delegates to TerminalManager
**Verify**: `cargo check`
**Commit**: `feat(acp): implement Client::create_terminal on AcpBridgeClient`
_Requirements: FR-001_ / _Design: Section 4.1_

## Task 1.11 — Implement Client::terminal_output

**Do**: Implement `terminal_output` in the Client impl. Extract `terminal_id` from request, delegate to `terminal_manager.borrow().output(...)`. Map result to `TerminalOutputResponse`.
**Files**: `crates/services/acp/bridge.rs`
**Done when**: `terminal_output` compiles
**Verify**: `cargo check`
**Commit**: `feat(acp): implement Client::terminal_output`
_Requirements: FR-002_ / _Design: Section 4.1_

## Task 1.12 — Implement Client::wait_for_terminal_exit

**Do**: Implement `wait_for_terminal_exit` in the Client impl. Delegate to `terminal_manager.borrow_mut().wait_for_exit(...)`. Return `WaitForTerminalExitResponse`.
**Files**: `crates/services/acp/bridge.rs`
**Done when**: `wait_for_terminal_exit` compiles
**Verify**: `cargo check`
**Commit**: `feat(acp): implement Client::wait_for_terminal_exit`
_Requirements: FR-003_ / _Design: Section 4.1_

---

- [ ] V4 [VERIFY] Quality checkpoint: `cargo fmt --check && cargo clippy && cargo check`
  - **Do**: Run quality commands and verify all pass
  - **Verify**: All commands exit 0
  - **Done when**: No lint errors, no type errors
  - **Commit**: `chore(acp): pass quality checkpoint` (if fixes needed)

---

## Task 1.13 — Implement Client::kill_terminal

**Do**: Implement `kill_terminal` in the Client impl. Delegate to `terminal_manager.borrow_mut().kill(...)`. Return `KillTerminalResponse`.
**Files**: `crates/services/acp/bridge.rs`
**Done when**: `kill_terminal` compiles
**Verify**: `cargo check`
**Commit**: `feat(acp): implement Client::kill_terminal`
_Requirements: FR-004_ / _Design: Section 4.1_

## Task 1.14 — Implement Client::release_terminal

**Do**: Implement `release_terminal` in the Client impl. Delegate to `terminal_manager.borrow_mut().release(...)`. Return `ReleaseTerminalResponse`.
**Files**: `crates/services/acp/bridge.rs`
**Done when**: `release_terminal` compiles
**Verify**: `cargo check`
**Commit**: `feat(acp): implement Client::release_terminal`
_Requirements: FR-005_ / _Design: Section 4.1_

---

- [ ] V5 [VERIFY] Quality checkpoint: `cargo fmt --check && cargo clippy && cargo check`
  - **Do**: Run quality commands and verify all pass
  - **Verify**: All commands exit 0
  - **Done when**: No lint errors, no type errors
  - **Commit**: `chore(acp): pass quality checkpoint` (if fixes needed)

---

### Group D: Diff Rendering (FR-008)

## Task 1.14b — <!-- DONE --> [x] [RED] Write failing test: Diff content extraction (AC-2.4)

**Do**: In `mapping.rs` tests, write `test_diff_content_extraction`: create a `ToolCallContent::Diff { old_text: "foo".into(), new_text: "bar".into() }`, call `extract_content_text`, assert result contains both old and new text. Test fails until Task 1.15 adds the Diff arm.
**Files**: `crates/services/acp/mapping.rs`
**Done when**: Test exists and fails
**Verify**: `cargo test test_diff_content_extraction 2>&1 | grep -E 'FAILED|error'`
**Commit**: `test(acp): RED - add failing Diff content extraction test`
_Requirements: FR-008_ / _Design: Section 4.3_

## Task 1.15 — <!-- DONE --> [x] Add Diff arm to extract_content_text (FR-008)

**Do**: In `crates/services/acp/mapping.rs`, update `extract_content_text` to handle `ToolCallContent::Diff` variant. Extract the diff content as a string (concatenate old_text/new_text or use the SDK's diff representation). Also update `extract_tool_content` which calls into `extract_content_text` via the `ToolCallContent` match arms.
**Files**: `crates/services/acp/mapping.rs`
**Done when**: `cargo check` passes, `ToolCallContent::Diff` no longer falls through to `None`
**Verify**: `cargo check`
**Commit**: `feat(acp): handle ToolCallContent::Diff in extract_content_text`
_Requirements: FR-008_ / _Design: Section 4.3_

## Task 1.15b — <!-- DONE --> [x] [GREEN] Verify Diff content extraction test passes

**Do**: Run `cargo test test_diff_content_extraction`. Fix if needed.
**Files**: (fix mapping.rs if test fails)
**Done when**: Test passes
**Verify**: `cargo test test_diff_content_extraction`
**Commit**: `test(acp): GREEN - Diff content extraction test passing`
_Requirements: FR-008_ / _Design: Section 4.3_

### Group E: SessionInfoUpdate Fields (FR-009)

## Task 1.16 — <!-- DONE --> Add title and updated_at to SessionInfoUpdate (FR-009) <!-- DONE -->

**Do**: In `crates/services/types/acp.rs`, change `AcpBridgeEvent::SessionInfoUpdate` from `{ session_id: String }` to `{ session_id: String, title: Option<String>, updated_at: Option<String> }`. Update `serialize_session_info_update` to include the new fields. In `mapping.rs`, update `map_session_notification_event`'s `SessionUpdate::SessionInfoUpdate` arm to extract `title` and `updated_at` from the SDK's `SessionInfoUpdate` struct.
**Files**: `crates/services/types/acp.rs`, `crates/services/acp/mapping.rs`
**Done when**: `cargo check` passes, SessionInfoUpdate carries title and updated_at
**Verify**: `cargo check`
**Commit**: `feat(acp): surface title and updated_at in SessionInfoUpdate`
_Requirements: FR-009_ / _Design: Section 4.3_

### Group F: ToolKind Forwarding (FR-010)

## Task 1.17 — <!-- DONE --> Add kind field to AcpSessionUpdateEvent (FR-010) <!-- DONE -->

**Do**: Add `pub kind_detail: Option<String>` to `AcpSessionUpdateEvent` in `types/acp.rs` (use `kind_detail` to avoid collision with existing `kind: AcpSessionUpdateKind`). In `mapping.rs`, extract `tool_call.kind` (if the SDK exposes it on ToolCall/ToolCallUpdate) and populate the field. Update `serialize_session_update` to include `"kind"` in the wire output when present.
**Files**: `crates/services/types/acp.rs`, `crates/services/acp/mapping.rs`
**Done when**: `cargo check` passes, tool kind forwarded on wire
**Verify**: `cargo check`
**Commit**: `feat(acp): forward ToolKind as kind_detail in session update events`
_Requirements: FR-010_ / _Design: Section 4.3_

---

- [x] V6 [VERIFY] Quality checkpoint: `cargo fmt --check && cargo clippy && cargo check`
  - **Do**: Run quality commands and verify all pass
  - **Verify**: All commands exit 0
  - **Done when**: No lint errors, no type errors
  - **Commit**: `chore(acp): pass quality checkpoint` (if fixes needed)

---

### Group G: Boolean Config (FR-012)

## Task 1.17b — <!-- DONE --> <!-- DONE --> [RED] Write failing test: Boolean config option mapping (AC-6.4)

**Do**: In `crates/services/acp/mapping.rs` tests, write `test_boolean_config_option_mapping`: create a `ConfigOptionUpdate` with `SessionConfigKind::Boolean(...)`, call the config mapping function, assert result has `kind = "boolean"`. Test must fail until Task 1.18 adds the Boolean arm.
**Files**: `crates/services/acp/mapping.rs`
**Done when**: Test exists and fails (unhandled variant or panic) — RED phase
**Verify**: `cargo test test_boolean_config_option_mapping 2>&1 | grep -E 'FAILED|error'`
**Commit**: `test(acp): RED - add failing Boolean config option mapping test`
_Requirements: FR-012_ / _Design: Section 4.3_

## Task 1.18 — <!-- DONE --> <!-- DONE --> Handle SessionConfigKind::Boolean in map_config_options (FR-012)

**Do**: In `mapping.rs`, the `map_config_options` function currently returns `None` for non-`Select` config kinds. Add a branch for `SessionConfigKind::Boolean(bool_config)` that creates an `AcpConfigOption` with two synthetic options: `{ value: "true", name: "Enabled" }` and `{ value: "false", name: "Disabled" }`, with `current_value` set to the boolean's current value as a string.
**Files**: `crates/services/acp/mapping.rs`
**Done when**: `cargo check` passes, Boolean config options are no longer silently dropped
**Verify**: `cargo check`
**Commit**: `feat(acp): handle SessionConfigKind::Boolean in config option mapping`
_Requirements: FR-012_ / _Design: Section 4.3_

## Task 1.18b — <!-- DONE --> <!-- DONE --> [GREEN] Verify Boolean config mapping test passes

**Do**: Run `cargo test test_boolean_config_option_mapping`. Fix mapping.rs if test fails.
**Files**: `crates/services/acp/mapping.rs` (fix if needed)
**Done when**: `cargo test test_boolean_config_option_mapping` exits 0
**Verify**: `cargo test test_boolean_config_option_mapping`
**Commit**: `test(acp): GREEN - Boolean config option mapping test passing`
_Requirements: FR-012_ / _Design: Section 4.3_

### Group H: Modes/Models at Session Start (FR-011)

## Task 1.18c — <!-- DONE --> [RED] Write failing test: modes and models extracted at session start (AC-5.5) <!-- DONE -->

**Do**: In `crates/services/acp/session.rs` tests, write `test_modes_models_at_session_start`: create a mock `NewSessionResponse` with populated `modes` and `models`, call the extraction logic, assert `available_modes` and `available_models` fields are non-empty in the emitted event. Test fails until Task 1.19 extracts these fields.
**Files**: `crates/services/acp/session.rs`
**Done when**: Test exists and fails — RED phase
**Verify**: `cargo test test_modes_models_at_session_start 2>&1 | grep -E 'FAILED|error'`
**Commit**: `test(acp): RED - add failing modes/models session start test`
_Requirements: FR-011_ / _Design: Section 4.4_

## Task 1.19 — <!-- DONE --> Extract modes and models from NewSessionResponse (FR-011) <!-- DONE -->

**Do**: In `session.rs:setup_session`, after `new_session` or `load_session` returns, check if the response includes `modes` and `models` (or equivalent fields in the SDK's `NewSessionResponse`/`LoadSessionResponse`). If present, emit them as a new `ServiceEvent::AcpBridge` event so the frontend has them at session start. Add a new `AcpBridgeEvent` variant if needed (e.g., `SessionStartInfo { session_id, modes, models }`), or emit them via existing `ConfigOptionsUpdate` / `ModeUpdate`.
**Files**: `crates/services/acp/session.rs`, `crates/services/types/acp.rs` (if new variant needed)
**Done when**: `cargo check` passes, modes/models emitted at session start
**Verify**: `cargo check`
**Commit**: `feat(acp): emit modes and models from session setup response`
_Requirements: FR-011_ / _Design: Section 4.4_

## Task 1.19b — <!-- DONE --> [GREEN] Verify modes/models session start test passes <!-- DONE -->

**Do**: Run `cargo test test_modes_models_at_session_start`. Fix session.rs if needed.
**Files**: `crates/services/acp/session.rs` (fix if needed)
**Done when**: `cargo test test_modes_models_at_session_start` exits 0
**Verify**: `cargo test test_modes_models_at_session_start`
**Commit**: `test(acp): GREEN - modes/models session start test passing`
_Requirements: FR-011_ / _Design: Section 4.4_

---

- [x] V7 [VERIFY] Quality checkpoint: `cargo fmt --check && cargo clippy && cargo check`
  - **Do**: Run quality commands and verify all pass
  - **Verify**: All commands exit 0
  - **Done when**: No lint errors, no type errors
  - **Commit**: `chore(acp): pass quality checkpoint` (if fixes needed)

---

### Group I: Authentication (FR-013)

## Task 1.20 — <!-- DONE --> Implement authenticate flow after initialize (FR-013) <!-- DONE -->

**Do**: In `session.rs:initialize_connection`, after `conn.initialize(initialize)` returns, check if the `InitializeResponse` indicates authentication is required (check `resp.auth_required` or equivalent SDK field). If so, read `AXON_ACP_AUTH_TOKEN` env var and call `conn.authenticate(token)`. On auth failure, return `Err("ACP authentication failed: ...")`. Add `AcpError::AuthFailed` variant if a custom error type exists, or use the existing `String` error path.
**Files**: `crates/services/acp/session.rs`
**Done when**: `cargo check` passes, authenticate is called when auth_required is true
**Verify**: `cargo check`
**Commit**: `feat(acp): call authenticate after initialize when auth required`
_Requirements: FR-013_ / _Design: Section 4.4_

### Group J: Capabilities Introspection (FR-014, FR-015, FR-016)

## Task 1.21 — <!-- DONE --> Store load_session_supported and prompt_capabilities (FR-014) <!-- DONE -->

**Do**: Add `load_session_supported: std::cell::Cell<bool>` and `prompt_capabilities: std::cell::RefCell<Option<String>>` (or a more structured type if the SDK provides one) to `AcpRuntimeState`. In `session.rs:initialize_connection`, after the `InitializeResponse`, store these from `resp.agent_capabilities`.
**Files**: `crates/services/acp/bridge/state.rs` (or `bridge.rs`), `crates/services/acp/session.rs`
**Done when**: `cargo check` passes, capabilities stored in runtime state
**Verify**: `cargo check`
**Commit**: `feat(acp): store load_session_supported and prompt_capabilities from InitializeResponse`
_Requirements: FR-014, FR-015_ / _Design: Section 4.4_

## Task 1.22 — <!-- DONE --> Guard load_session with capability flag (FR-015) <!-- DONE -->

**Do**: In `session.rs:setup_session`, before calling `conn.load_session(...)`, check `runtime_state.load_session_supported.get()`. If false, skip the load attempt and fall through to `new_session` directly, logging a warning. This requires passing `runtime_state` (or just the flag) into `setup_session`.
**Files**: `crates/services/acp/session.rs`
**Done when**: `cargo check` passes, load_session guarded by capability
**Verify**: `cargo check`
**Commit**: `feat(acp): guard load_session call with load_session_supported capability flag`
_Requirements: FR-014, FR-015_ / _Design: Section 4.4_

## Task 1.23 — <!-- DONE --> Expose prompt_capabilities via service layer (FR-016) <!-- DONE -->

**Do**: Add a function to retrieve the stored prompt_capabilities from `AcpRuntimeState`. This could be exposed via a service function or through the existing event system. At minimum, store it and make it accessible for callers that need to inspect what the adapter supports.
**Files**: `crates/services/acp/bridge/state.rs` (or `bridge.rs`)
**Done when**: `cargo check` passes, prompt_capabilities is accessible
**Verify**: `cargo check`
**Commit**: `feat(acp): expose prompt_capabilities from AcpRuntimeState`
_Requirements: FR-016_ / _Design: Section 4.4_

---

- [x] V8 [VERIFY] Quality checkpoint: `cargo fmt --check && cargo clippy && cargo check`
  - **Do**: Run quality commands and verify all pass
  - **Verify**: All commands exit 0
  - **Done when**: No lint errors, no type errors
  - **Commit**: `chore(acp): pass quality checkpoint` (if fixes needed)

---

### Group K: close_session (FR-017, FR-018)

## Task 1.24 — Implement close_session on teardown (FR-017) <!-- DONE -->

**Do**: In the persistent connection teardown path (`persistent_conn/` — look for where the adapter process is cleaned up on WS disconnect), call `conn.close_session(session_id)` before killing the adapter process. Log warning on failure but do not block teardown.
**Files**: `crates/services/acp/persistent_conn.rs` (or the relevant teardown file)
**Done when**: `cargo check` passes, close_session called on clean teardown
**Verify**: `cargo check`
**Commit**: `feat(acp): call close_session on persistent connection teardown`
_Requirements: FR-017, FR-018_ / _Design: Section 4.4_

## Task 1.25 — Gate close_session on adapter capability (FR-018) <!-- DONE -->

**Do**: Check if the adapter advertises close_session support (from `InitializeResponse.agent_capabilities` or similar). Store a `close_session_supported: Cell<bool>` on `AcpRuntimeState`. Only call `close_session` when the flag is true.
**Files**: `crates/services/acp/bridge/state.rs`, `crates/services/acp/persistent_conn.rs`
**Done when**: `cargo check` passes, close_session gated by capability
**Verify**: `cargo check`
**Commit**: `feat(acp): gate close_session call on adapter capability flag`
_Requirements: FR-017, FR-018_ / _Design: Section 4.4_

### Group L: message_id Forwarding (FR-019)

## Task 1.25b — [RED] Write failing test: message_id forwarded in event (AC-10.1) <!-- DONE -->

**Do**: In `crates/services/acp/mapping.rs` tests, write `test_message_id_forwarded`: create a `ContentChunk` with a non-empty `message_id`, map it through the event conversion, assert the resulting `AcpSessionUpdateEvent` has `message_id = Some("...")`. Test fails until Task 1.26 adds the field.
**Files**: `crates/services/acp/mapping.rs`
**Done when**: Test exists and fails — RED phase
**Verify**: `cargo test test_message_id_forwarded 2>&1 | grep -E 'FAILED|error'`
**Commit**: `test(acp): RED - add failing message_id forwarding test`
_Requirements: FR-019_ / _Design: Section 4.3_

## Task 1.26 — Forward ContentChunk.message_id (FR-019) <!-- DONE -->

**Do**: Add `pub message_id: Option<String>` to `AcpSessionUpdateEvent` in `types/acp.rs`. In `mapping.rs`, extract `message_id` from `ContentChunk` (the `AgentMessageChunk`, `UserMessageChunk`, `AgentThoughtChunk` variants). Update `serialize_session_update` to include `message_id` when present.
**Files**: `crates/services/types/acp.rs`, `crates/services/acp/mapping.rs`
**Done when**: `cargo check` passes, message_id forwarded on wire
**Verify**: `cargo check`
**Commit**: `feat(acp): forward ContentChunk.message_id in session update events`
_Requirements: FR-019_ / _Design: Section 4.3_

## Task 1.26b — [GREEN] Verify message_id forwarding test passes <!-- DONE -->

**Do**: Run `cargo test test_message_id_forwarded`. Fix mapping.rs if needed.
**Files**: `crates/services/acp/mapping.rs` (fix if needed)
**Done when**: `cargo test test_message_id_forwarded` exits 0
**Verify**: `cargo test test_message_id_forwarded`
**Commit**: `test(acp): GREEN - message_id forwarding test passing`
_Requirements: FR-019_ / _Design: Section 4.3_

---

- [x] V9 [VERIFY] Quality checkpoint: `cargo fmt --check && cargo clippy && cargo check`
  - **Do**: Run quality commands and verify all pass
  - **Verify**: All commands exit 0
  - **Done when**: No lint errors, no type errors
  - **Commit**: `chore(acp): pass quality checkpoint` (if fixes needed)

---

### Group M: MCP Handler Stubs for New Subactions

<!-- DONE --> Task 1.27 — Create handlers_acp.rs with list_sessions stub (FR-020)

**Do**: Create `crates/mcp/server/handlers_acp.rs` with the routing structure for ACP subactions. Start with `handle_acp_list_sessions` that calls `conn.list_sessions()` and returns the result as JSON. Register this handler in the MCP server's action/subaction router. If the main router is in `crates/mcp/server.rs` or similar, add the `acp` action with `list_sessions` subaction.
**Files**: `crates/mcp/server/handlers_acp.rs` (create), `crates/mcp/server.rs` (modify router)
**Done when**: `cargo check` passes, `acp/list_sessions` subaction is routed
**Verify**: `cargo check`
**Commit**: `feat(acp): add handlers_acp.rs with list_sessions subaction stub`
_Requirements: FR-020_ / _Design: Section 4.5_

<!-- DONE --> Task 1.28 — POC Checkpoint

**Do**: Verify the POC is functional: terminal module compiles, bridge client has all 5 terminal trait methods implemented, Diff arm handles content, SessionInfoUpdate carries title/updated_at, Boolean config mapped, auth flow wired, capabilities stored, close_session on teardown, message_id forwarded, MCP handler file exists. Run full `just verify`.
**Files**: (none — verification only)
**Done when**: `just verify` passes (fmt-check + clippy + check + test)
**Verify**: `just verify`
**Commit**: `feat(acp): complete POC — terminal API, diff, auth, capabilities, close_session`

---

## Phase 2 — Refactoring + Error Handling

## [x] Task 2.1 — Add proper error types for terminal operations

**Do**: Replace `String` error returns in `TerminalManager` methods with a `TerminalError` enum: `NotFound`, `AlreadyExited`, `SpawnFailed(String)`, `KillFailed(String)`, `CwdEscaped`. Map to `agent_client_protocol::Error` variants in the Client impl.
**Files**: `crates/services/acp/bridge/terminal.rs`, `crates/services/acp/bridge.rs`
**Done when**: `cargo check` passes, terminal errors are typed
**Verify**: `cargo check`
**Commit**: `refactor(acp): add typed TerminalError enum for terminal operations`
_Requirements: FR-001, FR-002, FR-003, FR-004, FR-005_ / _Design: Section 4.1_

## [x] Task 2.2 — Extract MCP server filter functions from mapping.rs

**Do**: The `filter_compatible_mcp_servers` and `filter_sdk_mcp_servers` functions in `mapping.rs` (lines 415-490) are MCP-specific, not mapping logic. Move them to a new `mapping/mcp_filters.rs` submodule (mapping.rs already has a `mapping/` directory with `validation.rs`). Re-export from `mapping.rs`.
**Files**: `crates/services/acp/mapping/mcp_filters.rs` (create), `crates/services/acp/mapping.rs` (modify)
**Done when**: `cargo check` passes, mapping.rs is shorter, filter functions in their own file
**Verify**: `cargo check`
**Commit**: `refactor(acp): extract MCP server filter functions to mapping/mcp_filters.rs`
_Requirements: FR-007_ / _Design: Section 3_

## [x] Task 2.3 — Verify bridge.rs and terminal.rs are under monolith limit

**Do**: Run the monolith check script. If `bridge.rs` is over 500 lines after adding the terminal Client methods, extract the 5 terminal Client trait method implementations into `bridge/terminal_client.rs` (keeping the trait impl delegation in `bridge.rs` as one-liners). Check all modified files.
**Files**: `crates/services/acp/bridge.rs`, `crates/services/acp/bridge/terminal_client.rs` (create if needed)
**Done when**: All files under 500 lines, monolith check passes
**Verify**: `python3 scripts/enforce_monoliths.py`
**Commit**: `refactor(acp): ensure all ACP files pass monolith limit`
_Requirements: FR-007_ / _Design: Section 3_

---

- [x] V10 [VERIFY] Quality checkpoint: `cargo fmt --check && cargo clippy && cargo check && cargo test --lib`
  - **Do**: Run full quality suite including tests
  - **Verify**: All commands exit 0
  - **Done when**: No errors, all tests pass
  - **Commit**: `chore(acp): pass quality checkpoint` (if fixes needed)

---

## Task 2.4 — Add error handling to authenticate flow <!-- DONE -->

**Do**: Handle the case where `AXON_ACP_AUTH_TOKEN` is not set but auth is required. Emit a clear error event via `ServiceEvent::Log` with `LogLevel::Error` and return a descriptive error message.
**Files**: `crates/services/acp/session.rs`
**Done when**: Missing auth token produces clear error, `cargo check` passes
**Verify**: `cargo check`
**Commit**: `refactor(acp): improve auth error handling for missing AXON_ACP_AUTH_TOKEN`
_Requirements: FR-013_ / _Design: Section 4.4_

## Task 2.5 — Add logging to terminal operations <!-- DONE -->

**Do**: Add `tracing::info` / `tracing::warn` logging to each `TerminalManager` method: log terminal creation with command/args, log output drain sizes, log kill signals sent, log release cleanup. Use structured logging with `terminal_id` field.
**Files**: `crates/services/acp/bridge/terminal.rs`
**Done when**: All terminal operations produce trace logs, `cargo check` passes
**Verify**: `cargo check`
**Commit**: `refactor(acp): add structured logging to terminal operations`
_Requirements: FR-001, FR-002, FR-003, FR-004, FR-005_ / _Design: Section 4.1_

---

## Phase 3 — Testing (REFACTOR Phase — Edge Cases and Error Paths)

Phase 3 tests focus on REFACTOR-phase testing: edge cases, error paths, and capability defaults. The primary RED→GREEN lifecycle tests for terminal, Diff, and core features were already written and verified in Phase 1.

### Group A: Terminal Edge Case Tests

## Task 3.1 — Test: terminal output with large buffer truncation <!-- DONE -->

**Do**: Write a unit test in `bridge/terminal.rs`: create a terminal running a command that produces output exceeding the byte limit, verify output returns `truncated = true` and contains the most recent output (not the oldest).
**Files**: `crates/services/acp/bridge/terminal.rs`
**Done when**: Test passes with `cargo test terminal`
**Verify**: `cargo test terminal`
**Commit**: `test(acp): terminal output truncation edge case test`
_Requirements: FR-002_ / _Design: Section 4.1_

## Task 3.2 — Test: wait_for_exit on already-exited terminal <!-- DONE -->

**Do**: Write a unit test: create a terminal running `true` (exits immediately), sleep briefly, call `wait_for_exit` — should return 0 immediately without blocking.
**Files**: `crates/services/acp/bridge/terminal.rs`
**Done when**: Test passes with `cargo test terminal`
**Verify**: `cargo test terminal`
**Commit**: `test(acp): wait_for_exit on already-exited terminal test`
_Requirements: FR-003_ / _Design: Section 4.1_

## Task 3.3 — Test: kill on already-exited terminal is no-op <!-- DONE -->

**Do**: Write a unit test: create terminal running `true`, wait for exit, call kill — should return Ok without error.
**Files**: `crates/services/acp/bridge/terminal.rs`
**Done when**: Test passes with `cargo test terminal`
**Verify**: `cargo test terminal`
**Commit**: `test(acp): kill on already-exited terminal no-op test`
_Requirements: FR-004_ / _Design: Section 4.1_

---

- [ ] V11 [VERIFY] Quality checkpoint: `cargo fmt --check && cargo clippy && cargo test --lib`
  - **Do**: Run quality suite with tests
  - **Verify**: All commands exit 0
  - **Done when**: All tests pass
  - **Commit**: `chore(acp): pass quality checkpoint` (if fixes needed)

---

### Group B: Mapping Edge Case Tests

## Task 3.4 — Test: ToolCallContent::Diff with None old_text (new file creation)

**Do**: Write a unit test in `mapping.rs` tests: construct a `ToolCallContent::Diff` with `old_text: None` and non-empty `new_text`, call `extract_content_text`, verify it handles the None case gracefully.
**Files**: `crates/services/acp/mapping.rs`
**Done when**: Test passes
**Verify**: `cargo test extract_content`
**Commit**: `test(acp): Diff with None old_text edge case test`
_Requirements: FR-008_ / _Design: Section 4.3_

## Task 3.5 — Test: Boolean config maps to two options

**Do**: Write a unit test: create a `SessionConfigKind::Boolean` config option, call `map_config_options`, verify it produces an `AcpConfigOption` with two options ("true"/"false").
**Files**: `crates/services/acp/mapping.rs`
**Done when**: Test passes
**Verify**: `cargo test map_config`
**Commit**: `test(acp): Boolean SessionConfigKind mapping test`
_Requirements: FR-012_ / _Design: Section 4.3_

## Task 3.6 — Test: SessionInfoUpdate carries title and updated_at

**Do**: Write a unit test: construct a `SessionNotification` with `SessionUpdate::SessionInfoUpdate` that has title and updated_at, call `map_session_notification_event`, verify the event carries both fields.
**Files**: `crates/services/acp/mapping.rs`
**Done when**: Test passes
**Verify**: `cargo test session_info`
**Commit**: `test(acp): SessionInfoUpdate title and updated_at mapping test`
_Requirements: FR-009_ / _Design: Section 4.3_

---

- [ ] V12 [VERIFY] Quality checkpoint: `cargo fmt --check && cargo clippy && cargo test --lib`
  - **Do**: Run quality suite with tests
  - **Verify**: All commands exit 0
  - **Done when**: All tests pass
  - **Commit**: `chore(acp): pass quality checkpoint` (if fixes needed)

---

### Group C: Capability and Auth Tests

## Task 3.7 — Test: load_session_supported defaults to false

**Do**: Write a unit test: create default `AcpRuntimeState`, verify `load_session_supported` is false.
**Files**: `crates/services/acp/bridge/state.rs` (or `bridge.rs`)
**Done when**: Test passes
**Verify**: `cargo test load_session_supported`
**Commit**: `test(acp): load_session_supported defaults to false`
_Requirements: FR-014, FR-015_ / _Design: Section 4.4_

## Task 3.8 — Test: close_session_supported defaults to false

**Do**: Write a unit test: verify `close_session_supported` defaults to false.
**Files**: `crates/services/acp/bridge/state.rs`
**Done when**: Test passes
**Verify**: `cargo test close_session_supported`
**Commit**: `test(acp): close_session_supported defaults to false`
_Requirements: FR-017, FR-018_ / _Design: Section 4.4_

## Task 3.9 — Test: message_id forwarded in session update

**Do**: Write a unit test: construct a notification with `message_id` set, map it, verify the output event has `message_id`.
**Files**: `crates/services/acp/mapping.rs`
**Done when**: Test passes
**Verify**: `cargo test message_id`
**Commit**: `test(acp): message_id forwarding in session update events`
_Requirements: FR-019_ / _Design: Section 4.3_

## Task 3.10 — Test: TerminalError variants map to protocol errors

**Do**: Write unit tests verifying each `TerminalError` variant maps to the correct `agent_client_protocol::Error` (e.g., NotFound → resource_not_found, CwdEscaped → internal_error).
**Files**: `crates/services/acp/bridge/terminal.rs`
**Done when**: Tests pass
**Verify**: `cargo test terminal_error`
**Commit**: `test(acp): TerminalError to protocol error mapping tests`
_Requirements: FR-001, FR-002, FR-003, FR-004, FR-005_ / _Design: Section 4.1_

---

- [ ] V13 [VERIFY] Quality checkpoint: `just verify`
  - **Do**: Run full verification suite
  - **Verify**: `just verify` exits 0
  - **Done when**: All checks pass
  - **Commit**: `chore(acp): pass full verify checkpoint` (if fixes needed)

---

## Phase 4 — Protocol Completeness (LOW Priority FRs)

### Group A: MCP Subaction Stubs

## Task 4.1 — Implement fork_session and resume_session stubs (FR-021)

**Do**: In `handlers_acp.rs`, add `acp/fork_session` and `acp/resume_session` subaction handlers. These call `conn.fork_session()` and `conn.resume_session()` respectively. If the SDK methods are gated behind unstable, ensure the feature flag is enabled. Return the SDK response as JSON.
**Files**: `crates/mcp/server/handlers_acp.rs`
**Done when**: `cargo check` passes, subactions routed
**Verify**: `cargo check`
**Commit**: `feat(acp): add fork_session and resume_session MCP subactions`
_Requirements: FR-021_ / _Design: Section 4.5_

## Task 4.2 — Implement set_session_model stub (FR-022)

**Do**: Add `acp/set_model` subaction in `handlers_acp.rs`. Calls `conn.set_session_model(session_id, model)`.
**Files**: `crates/mcp/server/handlers_acp.rs`
**Done when**: `cargo check` passes
**Verify**: `cargo check`
**Commit**: `feat(acp): add set_model MCP subaction`
_Requirements: FR-022_ / _Design: Section 4.5_

## Task 4.3 — Wire subscribe() to event bus (FR-023, FR-029)

**Do**: In the persistent connection setup, call `conn.subscribe()` to get the notification stream. Forward events to the internal `ServiceEvent` channel as `AcpBridgeEvent::DebugFrame` (or similar). Use `tokio::broadcast` if multiple consumers need the stream.
**Files**: `crates/services/acp/persistent_conn.rs` (or relevant file)
**Done when**: `cargo check` passes, subscribe wired
**Verify**: `cargo check`
**Commit**: `feat(acp): wire subscribe() to internal event bus`
_Requirements: FR-023, FR-029_ / _Design: Section 4.5_

---

- [ ] V14 [VERIFY] Quality checkpoint: `cargo fmt --check && cargo clippy && cargo check`
  - **Do**: Run quality checks
  - **Verify**: All exit 0
  - **Done when**: Clean
  - **Commit**: `chore(acp): pass quality checkpoint` (if fixes needed)

---

## Task 4.4 — Send cancel_request on turn cancellation (FR-024)

**Do**: In the persistent connection cancel path (where `conn.cancel()` is already called), also send `unstable_cancel_request` if the SDK provides it as a separate method. If `cancel()` already covers this, document why and mark FR-024 as satisfied by existing code.
**Files**: `crates/services/acp/persistent_conn.rs`
**Done when**: `cargo check` passes, cancel_request wired or documented
**Verify**: `cargo check`
**Commit**: `feat(acp): send unstable_cancel_request on turn cancellation`
_Requirements: FR-024_ / _Design: Section 4.5_

## Task 4.5 — Implement inbound ext_method dispatch (FR-025)

**Do**: In the `impl Client for AcpBridgeClient` block, implement `ext_method` to dispatch to a registered handler. Store handlers as `Rc<RefCell<HashMap<String, Box<dyn Fn(...)>>>>` on `AcpRuntimeState` or `AcpBridgeClient`. For now, log and return `method_not_found` if no handler registered.
**Files**: `crates/services/acp/bridge.rs`
**Done when**: `cargo check` passes, ext_method dispatches or returns error
**Verify**: `cargo check`
**Commit**: `feat(acp): implement inbound ext_method dispatch on bridge client`
_Requirements: FR-025_ / _Design: Section 4.5_

## Task 4.6 — Implement inbound ext_notification dispatch (FR-026)

**Do**: Implement `ext_notification` in the Client impl. Dispatch to registered handler if present, WARN log if no handler registered.
**Files**: `crates/services/acp/bridge.rs`
**Done when**: `cargo check` passes
**Verify**: `cargo check`
**Commit**: `feat(acp): implement inbound ext_notification dispatch`
_Requirements: FR-026_ / _Design: Section 4.5_

---

- [ ] V15 [VERIFY] Quality checkpoint: `cargo fmt --check && cargo clippy && cargo check`
  - **Do**: Run quality commands and verify all pass
  - **Verify**: All commands exit 0
  - **Done when**: No lint errors, no type errors
  - **Commit**: `chore(acp): pass quality checkpoint` (if fixes needed)

---

## Task 4.7 — Implement outbound ext_method MCP subaction (FR-027)

**Do**: Add `acp/ext_method` subaction in `handlers_acp.rs`. Accepts method name and params, calls `conn.ext_method(method, params)`.
**Files**: `crates/mcp/server/handlers_acp.rs`
**Done when**: `cargo check` passes
**Verify**: `cargo check`
**Commit**: `feat(acp): add outbound ext_method MCP subaction`
_Requirements: FR-027_ / _Design: Section 4.5_

## Task 4.8 — Implement outbound ext_notification MCP subaction (FR-028)

**Do**: Add `acp/ext_notification` subaction in `handlers_acp.rs`. Calls `conn.ext_notification(method, params)`.
**Files**: `crates/mcp/server/handlers_acp.rs`
**Done when**: `cargo check` passes
**Verify**: `cargo check`
**Commit**: `feat(acp): add outbound ext_notification MCP subaction`
_Requirements: FR-028_ / _Design: Section 4.5_

---

- [ ] V16 [VERIFY] Quality checkpoint: `cargo fmt --check && cargo clippy && cargo check`
  - **Do**: Run quality checks
  - **Verify**: All exit 0
  - **Done when**: Clean
  - **Commit**: `chore(acp): pass quality checkpoint` (if fixes needed)

---

### Group B: Elicitation + Logout (FR-030, FR-031, FR-032)

## Task 4.9 — Enable unstable_elicitation; handle ElicitRequest (FR-031)

**Do**: In the `InitializeRequest` builder (wherever `ClientCapabilities` is constructed), enable `unstable_elicitation`. Implement the `elicit` method on the `Client` impl (if the SDK adds it as a callback) or handle `ElicitRequest` in the session notification path. Forward the elicitation prompt to the frontend via `ServiceEvent`.
**Files**: `crates/services/acp/session.rs` (or `bridge.rs`), `crates/services/types/acp.rs`
**Done when**: `cargo check` passes, elicitation enabled and handled
**Verify**: `cargo check`
**Commit**: `feat(acp): enable unstable_elicitation and handle ElicitRequest pass-through`
_Requirements: FR-031_ / _Design: Section 4.5_

## Task 4.10 — Enable unstable_logout; expose acp/logout subaction (FR-032)

**Do**: In the `InitializeRequest`, enable `unstable_logout`. Add `acp/logout` subaction in `handlers_acp.rs` that calls `conn.logout()` (or the equivalent SDK method). This is a clean session termination signal.
**Files**: `crates/mcp/server/handlers_acp.rs`, `crates/services/acp/session.rs`
**Done when**: `cargo check` passes, logout wired
**Verify**: `cargo check`
**Commit**: `feat(acp): enable unstable_logout and expose acp/logout subaction`
_Requirements: FR-032_ / _Design: Section 4.5_

---

- [ ] V17 [VERIFY] Quality checkpoint: `just verify`
  - **Do**: Full verify pass
  - **Verify**: `just verify` exits 0
  - **Done when**: All checks pass
  - **Commit**: `chore(acp): pass full verify` (if fixes needed)

---

### Group C: Tests for Phase 4 Features

## Task 4.11 — Test: list_sessions subaction routing

**Do**: Write a unit test (or compile-time verification) that the `acp/list_sessions` subaction is registered in the MCP router.
**Files**: `crates/mcp/server/handlers_acp.rs`
**Done when**: Test passes
**Verify**: `cargo test handlers_acp`
**Commit**: `test(acp): list_sessions subaction routing test`
_Requirements: FR-020_ / _Design: Section 4.5_

## Task 4.12 — Test: ext_method returns method_not_found when no handler

**Do**: Write a test: call `ext_method` with no handlers registered, verify it returns `method_not_found` error.
**Files**: `crates/services/acp/bridge.rs`
**Done when**: Test passes
**Verify**: `cargo test ext_method`
**Commit**: `test(acp): ext_method returns method_not_found with no registered handler`
_Requirements: FR-025_ / _Design: Section 4.5_

## Task 4.13 — Test: ext_notification logs warning with no handler

**Do**: Write a test: call `ext_notification` with no handler, verify it logs a warning (or check tracing output).
**Files**: `crates/services/acp/bridge.rs`
**Done when**: Test passes
**Verify**: `cargo test ext_notification`
**Commit**: `test(acp): ext_notification warns when no handler registered`
_Requirements: FR-026_ / _Design: Section 4.5_

---

- [ ] V18 [VERIFY] Quality checkpoint: `just verify`
  - **Do**: Full verify pass including all new tests
  - **Verify**: `just verify` exits 0
  - **Done when**: All checks pass
  - **Commit**: `chore(acp): pass full verify` (if fixes needed)

---

## Phase 5 — Quality Gate + PR

## Task 5.1 — Monolith compliance check

**Do**: Run `python3 scripts/enforce_monoliths.py` on all modified/created files. Verify no file exceeds 500 lines. If any do, split further.
**Files**: All modified ACP files
**Done when**: Monolith check passes
**Verify**: `python3 scripts/enforce_monoliths.py`
**Commit**: `chore(acp): fix monolith violations` (if needed)
_Requirements: FR-007_ / _Design: Section 3_

## Task 5.2 — Verify no backward-incompatible changes (NFR-008)

**Do**: Review all struct modifications in `types/acp.rs` — every new field must be `Option<T>`. Review `AcpBridgeEvent` — all new variants must serialize with unique `"type"` values. Verify existing wire tests still pass.
**Files**: `crates/services/types/acp.rs`
**Done when**: All new fields are Optional, wire tests pass
**Verify**: `cargo test services_acp_bridge_event`
**Commit**: none (verification only)
_Requirements: NFR-008_ / _Design: Section 3_

---

- [ ] V19 [VERIFY] Full local CI: `cargo fmt --check && cargo clippy && cargo test --lib && just verify`
  - **Do**: Run complete local CI suite
  - **Verify**: All commands pass
  - **Done when**: Build succeeds, all tests pass
  - **Commit**: `chore(acp): pass local CI` (if fixes needed)

- [ ] V20 [VERIFY] CI pipeline passes
  - **Do**: Verify GitHub Actions/CI passes after push
  - **Verify**: `gh pr checks` shows all green
  - **Done when**: CI pipeline passes
  - **Commit**: None

- [ ] V21 [VERIFY] AC checklist
  - **Do**: Verify each FR (FR-001 through FR-032) is satisfied by checking code and tests. Verify NFRs: NFR-001 (500L limit), NFR-003 (no mod.rs), NFR-004 (?Send safety), NFR-005 (just verify), NFR-006 (TDD tests exist), NFR-007 (>=3 terminal tests), NFR-008 (backward compat), NFR-010 (no mod.rs for bridge).
  - **Verify**: Grep codebase for each FR implementation
  - **Done when**: All FRs and NFRs confirmed met
  - **Commit**: None
