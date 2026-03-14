# MCP Elicitation Demo Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a minimal `elicit` action to the axon MCP server so the feature can be exercised live inside Claude Code and the full server→client→user→server round-trip is visible.

**Architecture:** Wire the rmcp `peer.elicit::<T>()` API into the existing `axon` tool's action/subaction router. A new `ElicitDemoRequest` struct carries an optional message and form-field config; the handler pauses tool execution, presents a form to Claude Code, and echoes back whatever the user submitted. The server handler receives `Peer<RoleServer>` via rmcp's `FromContextPart` extractor alongside the existing `Parameters` argument — no refactor required.

**Tech Stack:** Rust, rmcp 1.1.0 (`elicitation` + `schemars` features), schemars 0.8 (already in dep tree via rmcp), serde/serde_json.

---

## Chunk 1: Cargo + Schema

### Task 1: Enable `elicitation` feature in Cargo.toml

**Files:**
- Modify: `Cargo.toml` (rmcp dependency, one line)

- [ ] **Step 1: Add the feature**

  Open `Cargo.toml`. Find:
  ```toml
  rmcp = { version = "1.1.0", features = ["server", "macros", "transport-io", "transport-streamable-http-server", "schemars"] }
  ```
  Change to:
  ```toml
  rmcp = { version = "1.1.0", features = ["server", "macros", "transport-io", "transport-streamable-http-server", "schemars", "elicitation"] }
  ```

- [ ] **Step 2: Verify it compiles**

  ```bash
  cargo check --bin axon 2>&1 | tail -5
  ```
  Expected: `Finished` with 0 errors. The `elicitation` feature gates code behind `#[cfg(all(feature = "schemars", feature = "elicitation"))]` — nothing breaks yet.

- [ ] **Step 3: Commit**

  ```bash
  git add Cargo.toml
  git commit -m "chore(mcp): enable rmcp elicitation feature"
  ```

---

### Task 2: Add `ElicitDemo` to the schema

**Files:**
- Modify: `crates/mcp/schema.rs` (add ~25 lines)
- Test: `crates/mcp/schema.rs` inline `#[cfg(test)]` block

The new action slots in alongside `Help` and `Status` — it's a direct action (no subaction needed).

- [ ] **Step 1: Write the failing test first**

  At the bottom of the `#[cfg(test)]` block in `crates/mcp/schema.rs`, add:

  ```rust
  #[test]
  fn serde_elicit_demo_default_fields() {
      let raw = obj(json!({ "action": "elicit_demo" }));
      let result = parse_axon_request(raw);
      assert!(result.is_ok(), "elicit_demo should parse with no fields");
      assert!(matches!(result.unwrap(), AxonRequest::ElicitDemo(_)));
  }

  #[test]
  fn serde_elicit_demo_with_message() {
      let raw = obj(json!({ "action": "elicit_demo", "message": "hello" }));
      let result = parse_axon_request(raw);
      assert!(result.is_ok(), "elicit_demo should parse with message");
  }
  ```

- [ ] **Step 2: Run the tests — expect compile failure**

  ```bash
  cargo test -p axon -- serde_elicit_demo 2>&1 | tail -10
  ```
  Expected: compile error — `ElicitDemo` variant doesn't exist yet.

- [ ] **Step 3: Add `ElicitDemoRequest` struct**

  In `crates/mcp/schema.rs`, after the `StatusRequest` struct (around line 190), add:

  ```rust
  /// Request for the `elicit_demo` action.
  ///
  /// Triggers an MCP elicitation form in the connected client (Claude Code).
  /// The server pauses, the user fills in the form, and the result is returned.
  #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, rmcp::schemars::JsonSchema)]
  #[serde(deny_unknown_fields)]
  pub struct ElicitDemoRequest {
      /// Prompt shown to the user above the form. Defaults to a generic message.
      #[serde(default)]
      pub message: Option<String>,

      /// Optional label for the "name" field. Defaults to "Your name".
      #[serde(default)]
      pub name_label: Option<String>,

      /// Optional label for the "favorite_color" field. Defaults to "Favorite color".
      #[serde(default)]
      pub color_label: Option<String>,
  }
  ```

- [ ] **Step 4: Add `ElicitDemo` variant to `AxonRequest`**

  In `crates/mcp/schema.rs`, in the `AxonRequest` enum (around line 7), add the variant after `Help`:

  ```rust
  ElicitDemo(ElicitDemoRequest),
  ```

- [ ] **Step 5: Run the tests — expect pass**

  ```bash
  cargo test -p axon -- serde_elicit_demo 2>&1 | tail -5
  ```
  Expected:
  ```
  test serde_elicit_demo_default_fields ... ok
  test serde_elicit_demo_with_message ... ok
  ```

- [ ] **Step 6: Run full test suite — no regressions**

  ```bash
  cargo test --lib 2>&1 | tail -5
  ```
  Expected: all tests pass.

- [ ] **Step 7: Commit**

  ```bash
  git add crates/mcp/schema.rs
  git commit -m "feat(mcp): add ElicitDemo action to AxonRequest schema"
  ```

---

## Chunk 2: Handler + Wire-up

### Task 3: Implement the elicitation handler

**Files:**
- Create: `crates/mcp/server/handlers_elicit.rs`

This file contains the form struct, the `elicit_safe!` marker, and the handler function. It must stay under 120 lines (monolith policy).

- [ ] **Step 1: Write the file**

  Create `crates/mcp/server/handlers_elicit.rs`:

  ```rust
  //! MCP elicitation demo handler.
  //!
  //! Demonstrates the server→client elicitation round-trip using rmcp's typed
  //! `Peer::elicit::<T>()` API. When Claude Code calls `action: "elicit_demo"`,
  //! this handler suspends tool execution, presents a two-field form to the user,
  //! and returns the submitted values (or a message if the user declines/cancels).

  use rmcp::{Peer, RoleServer, service::ElicitationError};
  use serde::{Deserialize, Serialize};
  use serde_json::json;

  use crate::crates::mcp::schema::ElicitDemoRequest;
  use crate::crates::mcp::server::common::internal_error;
  use rmcp::ErrorData;

  /// The form fields that Claude Code will present to the user.
  ///
  /// Field names become form labels (snake_case converted to title case by Claude Code).
  /// Descriptions are shown as helper text beneath each field.
  #[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
  struct ElicitDemoForm {
      /// Your name
      name: String,
      /// Favorite color
      color: String,
  }

  // Marks the struct as safe for typed elicitation (generates an object schema, not a primitive).
  rmcp::elicit_safe!(ElicitDemoForm);

  pub(crate) async fn handle_elicit_demo(
      peer: &Peer<RoleServer>,
      req: ElicitDemoRequest,
  ) -> Result<serde_json::Value, ErrorData> {
      let message = req
          .message
          .unwrap_or_else(|| "Please fill in the form to continue.".to_string());

      match peer.elicit::<ElicitDemoForm>(&message).await {
          Ok(Some(form)) => Ok(json!({
              "ok": true,
              "action": "elicit_demo",
              "data": {
                  "action": "accept",
                  "name": form.name,
                  "color": form.color,
                  "message": format!(
                      "Hi {}! Your favorite color is {}.",
                      form.name, form.color
                  )
              }
          })),

          Ok(None) => Ok(json!({
              "ok": true,
              "action": "elicit_demo",
              "data": {
                  "action": "accept_empty",
                  "message": "User accepted but provided no content."
              }
          })),

          Err(ElicitationError::UserDeclined) => Ok(json!({
              "ok": true,
              "action": "elicit_demo",
              "data": {
                  "action": "decline",
                  "message": "User explicitly declined to fill in the form."
              }
          })),

          Err(ElicitationError::UserCancelled) => Ok(json!({
              "ok": true,
              "action": "elicit_demo",
              "data": {
                  "action": "cancel",
                  "message": "User dismissed the form without responding."
              }
          })),

          Err(ElicitationError::CapabilityNotSupported) => Ok(json!({
              "ok": false,
              "action": "elicit_demo",
              "error": {
                  "code": "capability_not_supported",
                  "message": "Client does not support elicitation. Claude Code 2.1.76+ required."
              }
          })),

          Err(e) => {
              tracing::warn!(error = %e, "elicitation failed");
              Err(internal_error(format!("elicitation error: {e}")))
          }
      }
  }
  ```

- [ ] **Step 2: Check it compiles in isolation**

  ```bash
  cargo check --bin axon 2>&1 | grep "handlers_elicit\|error" | head -10
  ```
  Expected: errors about "module not declared" (we haven't wired it yet). No type errors in the handler itself.

---

### Task 4: Wire the handler into `server.rs`

**Files:**
- Modify: `crates/mcp/server.rs`

Two changes: declare the module + add `Peer<RoleServer>` param to the axon tool + add dispatch arm.

- [ ] **Step 1: Declare the new module**

  At the top of `crates/mcp/server.rs`, alongside the other module declarations, add:

  ```rust
  #[path = "server/handlers_elicit.rs"]
  mod handlers_elicit;
  ```

- [ ] **Step 2: Add `Peer<RoleServer>` to the tool handler signature**

  The current `axon` tool handler (line ~80) is:
  ```rust
  async fn axon<'a>(
      &'a self,
      Parameters(raw): Parameters<serde_json::Map<String, serde_json::Value>>,
  ) -> Result<String, ErrorData> {
  ```

  Change it to:
  ```rust
  async fn axon<'a>(
      &'a self,
      peer: rmcp::Peer<rmcp::RoleServer>,
      Parameters(raw): Parameters<serde_json::Map<String, serde_json::Value>>,
  ) -> Result<String, ErrorData> {
  ```

  rmcp's macro extracts `Peer<RoleServer>` from the `ToolCallContext` via `FromContextPart` — the order of parameters after `&self` is flexible.

- [ ] **Step 3: Add the dispatch arm**

  In the `match request { ... }` block inside the `axon` handler, add after `AxonRequest::Help(req) => self.handle_help(req).await?,`:

  ```rust
  AxonRequest::ElicitDemo(req) => {
      handlers_elicit::handle_elicit_demo(&peer, req)
          .await
          .map(|v| serde_json::to_string(&v).map_err(|e| internal_error(e.to_string())))?
  }
  ```

  Wait — `handle_elicit_demo` already returns `Result<serde_json::Value, ErrorData>`. Map it to JSON string:

  ```rust
  AxonRequest::ElicitDemo(req) => {
      let v = handlers_elicit::handle_elicit_demo(&peer, req).await?;
      serde_json::to_string(&v).map_err(|e| internal_error(e.to_string()))?
  }
  ```

- [ ] **Step 4: Verify it compiles**

  ```bash
  cargo check --bin axon 2>&1 | tail -8
  ```
  Expected: `Finished` — no errors.

- [ ] **Step 5: Run the full test suite**

  ```bash
  cargo test --lib 2>&1 | tail -5
  ```
  Expected: all tests pass. The existing tests don't exercise `Peer<RoleServer>` so adding the param doesn't break them.

- [ ] **Step 6: Clippy clean**

  ```bash
  cargo clippy --bin axon 2>&1 | grep "^error\|warning\[" | head -10
  ```
  Expected: 0 errors, 0 new warnings.

- [ ] **Step 7: Commit**

  ```bash
  git add crates/mcp/server.rs crates/mcp/server/handlers_elicit.rs
  git commit -m "feat(mcp): add elicit_demo action with typed rmcp elicitation"
  ```

---

## Chunk 3: Docs + End-to-End Test

### Task 5: Update MCP schema doc

**Files:**
- Modify: `docs/MCP-TOOL-SCHEMA.md`

The CLAUDE.md mandates keeping schema doc in sync with code changes. Note: the doc header says it's auto-generated by `scripts/generate_mcp_schema_doc.py`, but adding a manual section for new actions is acceptable until the generator is updated.

- [ ] **Step 1: Add elicit_demo section**

  Find the `## Preferred Client Actions` section in `docs/MCP-TOOL-SCHEMA.md`. Add `elicit_demo` to the "Direct actions" list:

  ```
  - Direct actions: `ask`, `doctor`, `domains`, `elicit_demo`, `help`, `map`, `query`, `research`, `retrieve`, `scrape`, `screenshot`, `search`, `sources`, `stats`, `status`
  ```

  Then at the bottom of the doc, add:

  ```markdown
  ## `elicit_demo` — Elicitation Demo

  **Purpose:** Exercises the MCP elicitation round-trip. The server requests a two-field form
  from Claude Code; the user fills it in; the server returns the submitted values.
  Requires Claude Code 2.1.76+ (first version to support `elicitation` client capability).

  **Request:**
  ```json
  { "action": "elicit_demo" }
  { "action": "elicit_demo", "message": "Custom prompt text shown above the form" }
  ```

  **Response (user accepted):**
  ```json
  {
    "ok": true,
    "action": "elicit_demo",
    "data": {
      "action": "accept",
      "name": "Alice",
      "color": "midnight blue",
      "message": "Hi Alice! Your favorite color is midnight blue."
    }
  }
  ```

  **Response (user declined or cancelled):**
  ```json
  {
    "ok": true,
    "action": "elicit_demo",
    "data": { "action": "decline", "message": "User explicitly declined to fill in the form." }
  }
  ```
  ```

- [ ] **Step 2: Commit**

  ```bash
  git add docs/MCP-TOOL-SCHEMA.md
  git commit -m "docs(mcp): document elicit_demo action"
  ```

---

### Task 6: End-to-end test with Claude Code

This is the payoff. Wire up the running server and exercise the feature interactively.

- [ ] **Step 1: Start the axon MCP server**

  In a terminal (keep it visible):
  ```bash
  cargo build --bin axon && ./target/debug/axon mcp
  ```
  Expected: server starts, prints nothing (stdio MCP transport — it waits for JSON-RPC on stdin).

- [ ] **Step 2: Reconnect the axon MCP server in Claude Code**

  In Claude Code, run:
  ```
  /mcp reconnect axon
  ```
  Expected: "Successfully reconnected to axon".

- [ ] **Step 3: Ask Claude to call elicit_demo**

  In Claude Code, send:
  ```
  Please call the axon tool with action: "elicit_demo" and message: "Tell me about yourself"
  ```
  Expected:
  - Claude Code presents a dialog/form with two fields: "name" and "color"
  - You fill them in and submit
  - Claude Code shows the response: `"Hi <name>! Your favorite color is <color>."`

- [ ] **Step 4: Test the decline path**

  Send:
  ```
  Call axon with action: "elicit_demo" again but this time I will click Decline
  ```
  Expected response:
  ```json
  { "action": "decline", "message": "User explicitly declined to fill in the form." }
  ```

- [ ] **Step 5: Final commit tag**

  ```bash
  git log --oneline -5
  ```
  Review the commit trail. If everything looks good:
  ```bash
  git tag v0.24.0-elicitation-demo
  ```

---

## Troubleshooting

| Symptom | Likely cause | Fix |
|---------|-------------|-----|
| `ElicitationError::CapabilityNotSupported` | Claude Code older than 2.1.76 | Update Claude Code (`claude update`) |
| Compile error: `elicit_safe not found` | rmcp `elicitation` feature not enabled | Check `Cargo.toml` features list |
| Compile error: `elicit` not a method on `Peer` | Both `schemars` AND `elicitation` features required | Ensure both are in Cargo.toml |
| Form shows raw field names (`name`, `color`) | Expected — Claude Code uses JSON Schema field names | Optionally add `title` via `#[schemars(title = "...")]` |
| `error[E0277]: the trait ElicitationSafe is not implemented` | `elicit_safe!(ElicitDemoForm)` macro missing | Add `rmcp::elicit_safe!(ElicitDemoForm);` after the struct |
| Server not showing `elicit_demo` in tool list | Schema doc generator hasn't been re-run | Manual doc update is sufficient; generator is optional |
