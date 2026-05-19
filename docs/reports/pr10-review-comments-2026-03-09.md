# PR #10 Review Comments by File

**Branch:** `refactor/acp-performance-modern-rust`  
**Unresolved threads:** 114 across 67 files  

---

## Table of Contents

- [crates/jobs/ingest/process.rs](#cratesjobsingestprocessrs) (7)
- [apps/web/lib/sessions/session-scanner.ts](#appsweblibsessionssessionscannerts) (4)
- [crates/services/acp/persistent_conn.rs](#cratesservicesacppersistentconnrs) (4)
- [crates/web.rs](#crateswebrs) (4)
- [crates/web/execute/sync_mode/acp_adapter.rs](#crateswebexecutesyncmodeacpadapterrs) (3)
- [crates/web/execute/sync_mode/types.rs](#crateswebexecutesyncmodetypesrs) (3)
- [crates/mcp/server/artifacts/lifecycle.rs](#cratesmcpserverartifactslifecyclers) (3)
- [docs/commands/ingest.md](#docscommandsingestmd) (3)
- [Justfile](#justfile) (2)
- [apps/web/components/reboot/axon-editor-artifact.tsx](#appswebcomponentsrebootaxoneditorartifacttsx) (2)
- [apps/web/components/reboot/axon-shell.tsx](#appswebcomponentsrebootaxonshelltsx) (2)
- [apps/web/lib/sessions/codex-scanner.ts](#appsweblibsessionscodexscannerts) (2)
- [apps/web/lib/sessions/gemini-scanner.ts](#appsweblibsessionsgeminiscannerts) (2)
- [apps/web/components/pulse/pulse-editor-pane.tsx](#appswebcomponentspulsepulseeditorpanetsx) (2)
- [apps/web/hooks/use-axon-acp.ts](#appswebhooksuseaxonacpts) (2)
- [crates/mcp/server/artifacts/respond.rs](#cratesmcpserverartifactsrespondrs) (2)
- [crates/ingest/CLAUDE.md](#cratesingestclaudemd) (2)
- [crates/web/execute/sync_mode/subprocess.rs](#crateswebexecutesyncmodesubprocessrs) (2)
- [docs/commands/github.md](#docscommandsgithubmd) (2)
- [crates/mcp/server/handlers_system.rs](#cratesmcpserverhandlerssystemrs) (2)
- [crates/mcp/server/oauth_google/tests.rs](#cratesmcpserveroauthgoogletestsrs) (2)
- [crates/services/acp/bridge.rs](#cratesservicesacpbridgers) (2)
- [apps/web/__tests__/use-axon-acp-editor.test.ts](#appswebtestsuseaxonacpeditortestts) (2)
- [apps/web/CLAUDE.md](#appswebclaudemd) (2)
- [apps/web/hooks/use-axon-session.ts](#appswebhooksuseaxonsessionts) (2)
- [CLAUDE.md](#claudemd) (2)
- [crates/ingest/reddit/meta.rs](#cratesingestredditmetars) (2)
- [crates/ingest/youtube.rs](#cratesingestyoutubers) (2)
- [crates/ingest/youtube/vtt.rs](#cratesingestyoutubevttrs) (2)
- [crates/services/acp.rs](#cratesservicesacprs) (2)
- [crates/web/execute/sync_mode/pulse_chat.rs](#crateswebexecutesyncmodepulsechatrs) (2)
- [crates/services/acp/runtime.rs](#cratesservicesacpruntimers) (2)
- [apps/web/components/ui/diff-node-static.tsx](#appswebcomponentsuidiffnodestatictsx) (1)
- [crates/web/execute/tests/acp_ws_event_tests.rs](#crateswebexecutetestsacpwseventtestsrs) (1)
- [docs/ARCHITECTURE.md](#docsarchitecturemd) (1)
- [docs/REBOOT-UI.md](#docsrebootuimd) (1)
- [apps/web/__tests__/axon-message-list-editor-blocks.test.ts](#appswebtestsaxonmessagelisteditorblockstestts) (1)
- [apps/web/__tests__/use-axon-session-retry.test.ts](#appswebtestsuseaxonsessionretrytestts) (1)
- [apps/web/components/ai-elements/artifact.tsx](#appswebcomponentsaielementsartifacttsx) (1)
- [apps/web/components/editor/plugins/diff-kit.tsx](#appswebcomponentseditorpluginsdiffkittsx) (1)
- [apps/web/app/api/sessions/list/route.ts](#appswebappapisessionslistroutets) (1)
- [apps/web/lib/sessions/session-utils.ts](#appsweblibsessionssessionutilsts) (1)
- [apps/web/lib/sessions/gemini-json-parser.ts](#appsweblibsessionsgeminijsonparserts) (1)
- [docs/ingest/github.md](#docsingestgithubmd) (1)
- [docs/commands/reddit.md](#docscommandsredditmd) (1)
- [docs/commands/youtube.md](#docscommandsyoutubemd) (1)
- [docs/MCP-TOOL-SCHEMA.md](#docsmcptoolschemamd) (1)
- [docs/MCP.md](#docsmcpmd) (1)
- [crates/mcp/server/artifacts/path.rs](#cratesmcpserverartifactspathrs) (1)
- [crates/mcp/server/artifacts/shape.rs](#cratesmcpserverartifactsshapers) (1)
- [crates/vector/ops/tei.rs](#cratesvectoropsteirs) (1)
- [crates/web/execute/sync_mode/dispatch.rs](#crateswebexecutesyncmodedispatchrs) (1)
- [apps/web/components/reboot/axon-sidebar.tsx](#appswebcomponentsrebootaxonsidebartsx) (1)
- [apps/web/components/reboot/axon-ui-config.ts](#appswebcomponentsrebootaxonuiconfigts) (1)
- [crates/core/config/cli.rs](#cratescoreconfigclirs) (1)
- [crates/ingest/github/meta.rs](#cratesingestgithubmetars) (1)
- [docs/ingest/youtube.md](#docsingestyoutubemd) (1)
- [crates/web/execute/sync_mode/params.rs](#crateswebexecutesyncmodeparamsrs) (1)
- [crates/services/acp/config.rs](#cratesservicesacpconfigrs) (1)
- [crates/services/acp/mapping.rs](#cratesservicesacpmappingrs) (1)
- [crates/services/acp/session.rs](#cratesservicesacpsessionrs) (1)
- [crates/web/execute/session_guard.rs](#crateswebexecutesessionguardrs) (1)
- [tests/services_acp_security.rs](#testsservicesacpsecurityrs) (1)
- [apps/web/app/api/sessions/[id]/route.ts](#appswebappapisessionsidroutets) (1)
- [Cargo.toml](#cargotoml) (1)
- [crates/web/execute/events.rs](#crateswebexecuteeventsrs) (1)
- [crates/web/execute/sync_mode.rs](#crateswebexecutesyncmoders) (1)

---

## `crates/jobs/ingest/process.rs` (7)

### 1. Line 89 — `minor`
**Thread:** `PRRT_kwDORS2O8s5zGZ39`  

**Unreachable code at line 88.**

The loop `for attempt in 0..=RETRY_429_MAX_ATTEMPTS` always returns from within — either `return Ok(n)` on success or `return Err(msg)` on the final attempt when the continue condition `attempt < RETRY_429_MAX_ATTEMPTS` fails. Line 88 is never executed.



[verification script omitted]

[verification script omitted]

<!-- fingerprinting:phantom:medusa:ocelot -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

### 2. Line 101 — `nitpick`
**Thread:** `PRRT_kwDORS2O8s5zGZ3_`  

**Function exceeds 80-line threshold and naming guideline violated.**

`ingest_youtube_playlist` is 102 lines, exceeding the 80-line warning threshold. Additionally, the function takes `&PgPool` but is not named `*_with_pool()`.

Consider extracting the concurrent processing logic (lines 144-195) into a separate helper to reduce complexity.



As per coding guidelines: "Function size should warn at 80 lines" and "Helper functions should be named `*_with_pool()`."

[verification script omitted]

<!-- fingerprinting:phantom:medusa:ocelot -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

### 3. Line 142 — `nitpick`
**Thread:** `PRRT_kwDORS2O8s5zGZ4D`  

**SQL update errors silently ignored.**

The progress update failure is swallowed with `let _ = ...`. While this may be intentional to avoid failing the job on a progress-tracking error, consider logging the failure for observability.



[verification script omitted]

<!-- suggestion_start -->

[verification script omitted]

<!-- suggestion_end -->

[verification script omitted]

<!-- fingerprinting:phantom:medusa:ocelot -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

### 4. Line 166 — `minor`
**Thread:** `PRRT_kwDORS2O8s5zGZ4R`  

**Minor: unnecessary clone of owned `video_url`.**

`video_url` is already an owned `String` from the tuple destructure. The clone on line 162 is needed only because `video_url` is used again in the log. Consider reordering to avoid the clone:



[verification script omitted]

<!-- suggestion_start -->

[verification script omitted]

<!-- suggestion_end -->

[verification script omitted]

<!-- fingerprinting:phantom:medusa:ocelot -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

### 5. Line 185 — `nitpick`
**Thread:** `PRRT_kwDORS2O8s5zGZ4W`  

**Same silent error suppression as initial progress write.**

Apply the same logging pattern here for consistency and debuggability.



[verification script omitted]

<!-- suggestion_start -->

[verification script omitted]

<!-- suggestion_end -->

[verification script omitted]

<!-- fingerprinting:phantom:medusa:ocelot -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

### 6. Line 26 — `nitpick`
**Thread:** `PRRT_kwDORS2O8s5zGZ32`  

**Helper function naming does not follow guideline.**

Functions taking `&PgPool` should be named with `*_with_pool()` suffix per coding guidelines.



[verification script omitted]

As per coding guidelines: "Helper functions should be named `*_with_pool()`."

[verification script omitted]

<!-- fingerprinting:phantom:medusa:ocelot -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

### 7. Line 62 — `nitpick`
**Thread:** `PRRT_kwDORS2O8s5zGZ36`  

**Imports inside function bodies reduce readability.**

Consider moving `use crate::crates::ingest` and `use futures_util::stream::{FuturesUnordered, StreamExt}` to the top of the file with other imports for consistency and clarity.




Also applies to: 102-103

[verification script omitted]

<!-- fingerprinting:phantom:medusa:ocelot -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

## `apps/web/lib/sessions/session-scanner.ts` (4)

### 1. Line 252 — `minor`
**Thread:** `PRRT_kwDORS2O8s5zIDeG`  

**Per-agent guarantee may exceed the requested `limit`.**

The loop at lines 239-246 guarantees up to `minPerAgent` (3) sessions from each agent. With 3 agents, this could add up to 9 guaranteed sessions. If `limit` is less than 9 (e.g., `limit=5`), the result could exceed the requested limit since the filler at line 251 uses `limit - guaranteed.length`, which would be negative and slice nothing, but `guaranteed` itself would already have more than `limit` items.


[verification script omitted]

<!-- suggestion_start -->

[verification script omitted]

<!-- suggestion_end -->

[verification script omitted]

<!-- fingerprinting:phantom:medusa:ocelot -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

### 2. Line 241 — `info`
**Thread:** `PRRT_kwDORS2O8s5zH21l`  

<!-- metadata:{"confidence":7} -->
P2: `scanSessions` can return more than `limit` because the min-per-agent loop never caps `guaranteed` by `limit`. This breaks the contract for small limits (e.g., limit=1).

[verification script omitted]

```suggestion
    if (count < minPerAgent && guaranteed.length < limit) {
```

<a href="https://www.cubic.dev/action/fix/violation/c6a9235d-707a-4c25-8149-8571a3e03d36" target="_blank" rel="noopener noreferrer" data-no-image-dialog="true">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://cubic.dev/buttons/fix-with-cubic-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="https://cubic.dev/buttons/fix-with-cubic-light.svg">
    <img alt="Fix with Cubic" src="https://cubic.dev/buttons/fix-with-cubic-dark.svg">
  </picture>
</a>

---

### 3. Line 193 — `MAJOR`
**Thread:** `PRRT_kwDORS2O8s5zF_M9`  

**Avoid emitting the full local path in warnings.**

`absolutePath` includes the home directory and project names, so every unreadable session file leaks local filesystem details into logs. Prefer an opaque identifier such as `sessionId(absolutePath)` and keep the error payload path-free.

[verification script omitted]

<!-- suggestion_start -->

[verification script omitted]

<!-- suggestion_end -->

[verification script omitted]

<!-- fingerprinting:phantom:medusa:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

### 4. Line 156 — `MAJOR`
**Thread:** `PRRT_kwDORS2O8s5zF_M4`  

[verification script omitted]

**Use `fs.lstat()` instead of `fs.stat()` and validate file type before extraction.**

Line 197 relies on `fs.stat()` via `isDirEntry()`, which follows symlinks—a symlinked project directory can point outside `~/.claude/projects`. More critically, lines 212–214 call `extractPreview()` (which opens the file at line 53) in parallel with the `fs.stat()` check. The file type validation at line 216 (`stat.isFile()`) happens after the file has already been opened, allowing symlinks to FIFOs, sockets, or other non-regular files to be opened. Replace `fs.stat()` calls with `fs.lstat()` to detect symlinks, and call `extractPreview()` only after the `stat.isFile()` check passes.

[verification script omitted]

[verification script omitted]

<!-- fingerprinting:phantom:medusa:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

## `crates/services/acp/persistent_conn.rs` (4)

### 1. Line 153 — `MAJOR`
**Thread:** `PRRT_kwDORS2O8s5zIDeL`  

**Fail buffered turns explicitly when the adapter loop exits.**

These exit paths stop the loop immediately, so any queued `TurnRequest`s are dropped with `rx` and their `result_tx` senders disappear without sending. Callers that already enqueued work will get oneshot cancellation instead of a deterministic ACP error. Drain the queue and `send(Err(...))` to each pending turn before returning or breaking.



Also applies to: 178-184

[verification script omitted]

<!-- fingerprinting:phantom:medusa:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

### 2. Line 348 — `MAJOR`
**Thread:** `PRRT_kwDORS2O8s5zIDeM`  

**Preserve editor block content verbatim.**

`trim()` rewrites the payload before `EditorWrite`, which drops leading indentation and trailing newlines from code/markdown edits. That can corrupt the actual file content being written.

[verification script omitted]

<!-- suggestion_start -->

[verification script omitted]

<!-- suggestion_end -->

[verification script omitted]

<!-- fingerprinting:phantom:medusa:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

### 3. Line 189 — `nitpick`
**Thread:** `PRRT_kwDORS2O8s5zF_Nc`  

**Split the lifecycle helpers before they grow further.**

`adapter_loop()` and `run_turn_on_conn()` are both already past the 80-line warning threshold, which makes the setup/error/result flow harder to audit. Extract session establishment, per-turn execution, and result emission into smaller helpers.


As per coding guidelines, `**/*.rs`: `Function size should warn at 80 lines and hard fail at 120 lines (via monolith policy); exempt test, bench, and config files`.


Also applies to: 197-298

[verification script omitted]

<!-- fingerprinting:phantom:medusa:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

### 4. Line 132 — `MAJOR`
**Thread:** `PRRT_kwDORS2O8s5zF_Ng`  

**Apply later model changes per turn, or reject them explicitly.**

The session model is captured only from `first_turn.req.model` during setup, and later calls to `run_turn_on_conn()` never touch `req.model` again. After turn 1, model switches on the same socket are silently ignored and prompts keep running on the original model.



Also applies to: 164-170, 206-232

[verification script omitted]

<!-- fingerprinting:phantom:medusa:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

## `crates/web.rs` (4)

### 1. Line 35 — `minor`
**Thread:** `PRRT_kwDORS2O8s5zGbEU`  

**Comment says default 5, code uses 8.**

Line 27 states "default 5" but line 33 uses `.unwrap_or(8)`. Update the comment or the default.

[verification script omitted]

<!-- suggestion_start -->

[verification script omitted]

<!-- suggestion_end -->

[verification script omitted]

<!-- fingerprinting:phantom:medusa:ocelot -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

### 2. Line 314 — `minor`
**Thread:** `PRRT_kwDORS2O8s5zGbEf`  

**TODO comment appears stale — composite key already implemented.**

The TODO on line 309 says to change to `(session_id, tool_call_id)` composite key, but line 485 already uses this exact key format in `permission_responders.remove(&(session_id.clone(), tool_call_id.clone()))`. Remove or update the TODO.

[verification script omitted]

<!-- suggestion_start -->

[verification script omitted]

<!-- suggestion_end -->

[verification script omitted]

<!-- fingerprinting:phantom:medusa:ocelot -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

### 3. Line 440 — `MAJOR`
**Thread:** `PRRT_kwDORS2O8s5zF_Nr`  

**Blank `session_id` now drops permission responses silently.**

The responder map is keyed by `(session_id, tool_call_id)`, but this path still accepts an omitted `session_id` as `""` and only logs when the lookup misses. Any client that has not started sending the new field will now fail every permission response with no client-visible error. Reject blank `session_id` before routing, or make it required for `permission_response` so this protocol break is explicit.



Also applies to: 462-482

[verification script omitted]

<!-- fingerprinting:phantom:poseidon:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

### 4. Line 485 — `info`
**Thread:** `PRRT_kwDORS2O8s5zF30v`  

<!-- metadata:{"confidence":9} -->
P1: Composite-key lookup silently fails when the client omits `session_id`. The bridge inserts with the real ACP session ID from `OnceLock`, but `WsClientMsg.session_id` defaults to `""` via `#[serde(default)]`. An empty `session_id` will never match a non-empty one, causing the `remove()` to return `None` and the permission request to time out after 60 seconds. Either fall back to a `tool_call_id`-only lookup when `session_id` is empty, or make `session_id` required on the wire.

[verification script omitted]

<a href="https://www.cubic.dev/action/fix/violation/05f8a91c-b0e5-477f-9998-c9cf12038a25" target="_blank" rel="noopener noreferrer" data-no-image-dialog="true">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://cubic.dev/buttons/fix-with-cubic-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="https://cubic.dev/buttons/fix-with-cubic-light.svg">
    <img alt="Fix with Cubic" src="https://cubic.dev/buttons/fix-with-cubic-dark.svg">
  </picture>
</a>

---

## `crates/web/execute/sync_mode/acp_adapter.rs` (3)

### 1. Line 51 — `minor`
**Thread:** `PRRT_kwDORS2O8s5zGbEi`  

**Misplaced doc comment — documents wrong function.**

The doc comment on lines 46-50 describes `resolve_acp_adapter_command` but is attached to `candidate_local_executable_paths`. Move it above line 118.

[verification script omitted]

[verification script omitted]

<!-- fingerprinting:phantom:medusa:ocelot -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

### 2. Line 113 — `nitpick`
**Thread:** `PRRT_kwDORS2O8s5zGZ5L`  

**Consider deduplicating codex/gemini fallback logic.**

The codex and gemini fallback blocks (lines 91-101 and 103-113) are nearly identical. A helper would reduce repetition.

[verification script omitted]

[verification script omitted]

<!-- fingerprinting:phantom:medusa:ocelot -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

### 3. Line 66 — `info`
**Thread:** `PRRT_kwDORS2O8s5zGIFa`  

<!-- metadata:{"confidence":6} -->
P2: Skip HOME-based candidates when HOME is empty to avoid resolving executables from relative `./.local/bin` or `./.cargo/bin` paths.

[verification script omitted]

<a href="https://www.cubic.dev/action/fix/violation/90965297-0119-48c4-b85b-bee0fd97ba92" target="_blank" rel="noopener noreferrer" data-no-image-dialog="true">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://cubic.dev/buttons/fix-with-cubic-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="https://cubic.dev/buttons/fix-with-cubic-light.svg">
    <img alt="Fix with Cubic" src="https://cubic.dev/buttons/fix-with-cubic-dark.svg">
  </picture>
</a>

---

## `crates/web/execute/sync_mode/types.rs` (3)

### 1. Line 135 — `nitpick`
**Thread:** `PRRT_kwDORS2O8s5zGbEj`  

**Consider implementing `std::str::FromStr` for `ServiceMode`.**

This would allow idiomatic `"scrape".parse::<ServiceMode>()` and better ecosystem integration. The current `from_str` method signature shadows the trait name.

[verification script omitted]

<!-- fingerprinting:phantom:medusa:ocelot -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

### 2. Line 149 — `minor`
**Thread:** `PRRT_kwDORS2O8s5zGbEn`  

**Potential truncation on 32-bit platforms.**

`v.as_u64().map(|n| n as usize)` silently truncates values > `usize::MAX` on 32-bit platforms. For pagination limits this is unlikely to matter in practice, but `usize::try_from(n).ok()` would be defensive.

[verification script omitted]

<!-- fingerprinting:phantom:medusa:ocelot -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

### 3. Line 142 — `info`
**Thread:** `PRRT_kwDORS2O8s5zGIFl`  

<!-- metadata:{"confidence":8} -->
P2: Unchecked `u64 -> usize` cast can silently truncate out-of-range values. Use a checked conversion before returning the parsed flag.

[verification script omitted]

```suggestion
        .and_then(|n| usize::try_from(n).ok())
```

<a href="https://www.cubic.dev/action/fix/violation/822860a3-ea43-4605-bf8a-5002d5c2ad2f" target="_blank" rel="noopener noreferrer" data-no-image-dialog="true">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://cubic.dev/buttons/fix-with-cubic-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="https://cubic.dev/buttons/fix-with-cubic-light.svg">
    <img alt="Fix with Cubic" src="https://cubic.dev/buttons/fix-with-cubic-dark.svg">
  </picture>
</a>

---

## `crates/mcp/server/artifacts/lifecycle.rs` (3)

### 1. Line 206 — `nitpick`
**Thread:** `PRRT_kwDORS2O8s5zGbEJ`  

**Entire file loaded into memory for regex search — consider streaming for large artifacts.**

`tokio::fs::read_to_string` loads the full file content before searching. Large artifacts (multi-MB JSON) could spike memory. For a lifecycle utility this is likely acceptable, but if artifacts can grow large, consider streaming line-by-line with `BufReader`.

[verification script omitted]

<!-- fingerprinting:phantom:medusa:ocelot -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

### 2. Line 212 — `info`
**Thread:** `PRRT_kwDORS2O8s5zGIFs`  

<!-- metadata:{"confidence":8} -->
P2: Check capacity before pushing so `matches` never exceeds the requested `limit`.

[verification script omitted]

<a href="https://www.cubic.dev/action/fix/violation/3617390f-c963-4738-a7a2-894b82e08d29" target="_blank" rel="noopener noreferrer" data-no-image-dialog="true">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://cubic.dev/buttons/fix-with-cubic-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="https://cubic.dev/buttons/fix-with-cubic-light.svg">
    <img alt="Fix with Cubic" src="https://cubic.dev/buttons/fix-with-cubic-dark.svg">
  </picture>
</a>

---

### 3. Line 78 — `info`
**Thread:** `PRRT_kwDORS2O8s5zGIFW`  

<!-- metadata:{"confidence":9} -->
P1: Use checked multiplication for `max_age_hours * 3600` to prevent overflow-driven incorrect cleanup behavior.

[verification script omitted]

<a href="https://www.cubic.dev/action/fix/violation/9acd4783-4e08-41a7-96cf-692edadf9c2e" target="_blank" rel="noopener noreferrer" data-no-image-dialog="true">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://cubic.dev/buttons/fix-with-cubic-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="https://cubic.dev/buttons/fix-with-cubic-light.svg">
    <img alt="Fix with Cubic" src="https://cubic.dev/buttons/fix-with-cubic-dark.svg">
  </picture>
</a>

---

## `docs/commands/ingest.md` (3)

### 1. Line 7 — `minor`
**Thread:** `PRRT_kwDORS2O8s5zGZ5b`  

**Use the required counterpart-link format here.**

This replaces the standard single ingest-counterpart link with a per-source list and adds “pipeline internals,” which the command-reference template explicitly excludes. If `docs/ingest/ingest.md` does not exist yet, add that wrapper page and link it here instead.

As per coding guidelines, `docs/commands/*.md`: Every command file in docs/commands/ must include a cross-link to its ingest counterpart with the text: '> For implementation details and troubleshooting see [`docs/ingest/<name>.md`](../ingest/<name>.md).'

[verification script omitted]

<!-- fingerprinting:phantom:medusa:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

### 2. Line 42 — `MAJOR`
**Thread:** `PRRT_kwDORS2O8s5zGZ5d`  

**Document the `yt-dlp` prerequisite on this page.**

The command now advertises YouTube targets, but there is still no one-line install note for the required external dependency, so a user can follow the examples and still fail on first run.

As per coding guidelines, `docs/commands/*.md`: CLI command reference files should include: synopsis/usage line, arguments table, all flags and defaults (including command-specific flags), job subcommands (status, cancel, list, cleanup, clear, recover, worker), concrete usage examples, required environment variables (brief), and one-line install instructions for external dependencies.

[verification script omitted]

<!-- fingerprinting:phantom:medusa:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

### 3. Line 122 — `minor`
**Thread:** `PRRT_kwDORS2O8s5zGZ5f`  

**Remove the storage-schema note from the CLI reference.**

`axon_ingest_jobs` and `source_type` are internal persistence details, not command-surface behavior. Keep this page user-facing and move storage internals to ingest implementation docs if they still need to be documented.

As per coding guidelines, `docs/commands/*.md`: CLI command reference files should NOT include: step-by-step pipeline internals, troubleshooting sections, known limitations tables, or implementation details (function names, data structures).

[verification script omitted]

</details>

[verification script omitted]

<!-- fingerprinting:phantom:medusa:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

## `Justfile` (2)

### 1. Line 188 — `MAJOR`
**Thread:** `PRRT_kwDORS2O8s5zIDec`  

[verification script omitted]

**Bare `wait` masks crashed children in both wrappers.**

Lines 188 and 212 use `wait` with no job list. In Bash, this waits for all background jobs but returns the exit status of only the last one to terminate. If any earlier process fails while a later one succeeds, the script exits with success despite the failure.

For example, in `workers` (lines 175-188), if the crawl worker crashes immediately but the refresh worker continues running, `wait` will return 0 even though crawl failed. Same issue in `dev` (lines 192-212) with seven spawned processes.



Also applies to: 212-212

[verification script omitted]

<!-- fingerprinting:phantom:medusa:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

### 2. Line 201 — `MAJOR`
**Thread:** `PRRT_kwDORS2O8s5zIDeg`  

**Keep `dev` cleanup PID-scoped.**

Line 201 already tracks the spawned child PIDs. Calling `just stop` here widens cleanup to repo-wide `pkill -f` patterns, so exiting one `just dev` session can kill another `axon` or `next dev` process running elsewhere on the machine.

[verification script omitted]

[verification script omitted]

<!-- fingerprinting:phantom:medusa:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

## `apps/web/components/reboot/axon-editor-artifact.tsx` (2)

### 1. Line 63 — `MAJOR`
**Thread:** `PRRT_kwDORS2O8s5zIDdk`  

**The parser currently treats literal `<axon:editor>` text as a control block.**

Any message that mentions this tag in documentation, examples, or code fences will have that text stripped from `displayText` and turned into an editor action. That corrupts normal content and can surface bogus “open in editor” artifacts. This needs a stronger framing mechanism than a global regex over free-form message text, or at least parsing that ignores fenced/code-literal content.

[verification script omitted]

<!-- fingerprinting:phantom:poseidon:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

### 2. Line 89 — `minor`
**Thread:** `PRRT_kwDORS2O8s5zIDdn`  

**Clear the copy timer when the card unmounts.**

`copyTimerRef` is reused, but the pending timeout is never cleaned up if the artifact card disappears before the 2-second reset runs. That leaves a stale timer around the component lifecycle and can fire `setCopied(false)` after unmount.

[verification script omitted]

<!-- fingerprinting:phantom:poseidon:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

## `apps/web/components/reboot/axon-shell.tsx` (2)

### 1. Line 175 — `MAJOR`
**Thread:** `PRRT_kwDORS2O8s5zIDdr`  

**This derived loading flag never settles for legitimately empty sessions.**

Once `useAxonSession` finishes loading an empty history, `sessionLoadingBase` becomes `false` but `historicalMessages.length === 0` stays true. That leaves the UI in a permanent loading state and keeps the sync effect from ever committing the loaded empty result.



[verification script omitted]

[verification script omitted]

<!-- fingerprinting:phantom:poseidon:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

### 2. Line 199 — `minor`
**Thread:** `PRRT_kwDORS2O8s5zIDdw`  

**`onEditorUpdate` still leaves the editor hidden on mobile.**

This only sets `editorOpen`, but the mobile layout is driven by `mobilePane`. ACP `editor_update` events will update the editor content while the user remains on chat/sidebar.



[verification script omitted]

[verification script omitted]

<!-- fingerprinting:phantom:poseidon:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

## `apps/web/lib/sessions/codex-scanner.ts` (2)

### 1. Line 91 — `MAJOR`
**Thread:** `PRRT_kwDORS2O8s5zIDd4`  

**Use `fs.lstat()` and validate file type before extracting preview.**

Lines 86-89 call `fs.stat()` and `extractCodexPreview()` in parallel. The file type validation at line 90 (`stat.isFile()`) happens after the file has already been opened and read by `extractCodexPreview()`. This allows symlinks to non-regular files to be processed. Use `fs.lstat()` to detect symlinks, and call `extractCodexPreview()` only after confirming the file type.


[verification script omitted]

<!-- suggestion_start -->

[verification script omitted]

<!-- suggestion_end -->

[verification script omitted]

<!-- fingerprinting:phantom:medusa:ocelot -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

### 2. Line 149 — `minor`
**Thread:** `PRRT_kwDORS2O8s5zIDd-`  

**Use `fs.lstat()` in `isDir` to avoid following symlinks.**

The `isDir` helper uses `fs.stat()` which follows symlinks. A symlinked directory could point outside `~/.codex/sessions`, potentially causing the scanner to traverse unintended locations.


[verification script omitted]

<!-- suggestion_start -->

[verification script omitted]

<!-- suggestion_end -->

[verification script omitted]

<!-- fingerprinting:phantom:medusa:ocelot -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

## `apps/web/lib/sessions/gemini-scanner.ts` (2)

### 1. Line 76 — `MAJOR`
**Thread:** `PRRT_kwDORS2O8s5zIDeB`  

**Use `fs.lstat()` and validate file type before reading content.**

Lines 71-74 call `fs.stat()` and `fs.readFile()` in parallel via `Promise.all`. The file type check at line 75 (`stat.isFile()`) occurs after the file has already been fully read. This allows symlinks to special files (FIFOs, sockets) to be opened and read. Additionally, `fs.stat()` follows symlinks, potentially reading files outside the expected directory.


[verification script omitted]

[verification script omitted]

<!-- fingerprinting:phantom:medusa:ocelot -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

### 2. Line 122 — `nitpick`
**Thread:** `PRRT_kwDORS2O8s5zIDeC`  

**Consider moving `safeReaddir` near the top or to session-utils.**

The `safeReaddir` helper is defined at the end of the file but used earlier at lines 55 and 62. While JavaScript hoisting makes this work, placing helpers before their first use improves readability. Additionally, this same helper is duplicated in `codex-scanner.ts` — consider extracting to `session-utils.ts` for reuse.

[verification script omitted]

<!-- fingerprinting:phantom:medusa:ocelot -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

## `apps/web/components/pulse/pulse-editor-pane.tsx` (2)

### 1. Line 118 — `nitpick`
**Thread:** `PRRT_kwDORS2O8s5zIDdf`  

**Consider adding development-mode logging in the catch block.**

The empty catch block makes it difficult to diagnose issues when `onChange` fails. While the comment explains that this is intentional for handling plugin normalizer failures on scraped content, completely silent failures could mask other bugs.


[verification script omitted]

<!-- suggestion_start -->

[verification script omitted]

<!-- suggestion_end -->

[verification script omitted]

<!-- fingerprinting:phantom:poseidon:ocelot -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

### 2. Line 118 — `info`
**Thread:** `PRRT_kwDORS2O8s5zH21Z`  

<!-- metadata:{"confidence":9} -->
P1: Do not swallow external-update errors and still mark markdown as applied; this can permanently desync editor content from props after a thrown normalization/update.

[verification script omitted]

```suggestion
    } catch {
      isApplyingExternalUpdateRef.current = false
      return
    }
```

<a href="https://www.cubic.dev/action/fix/violation/03a997a8-38f3-4194-b87f-25a63e2b2c80" target="_blank" rel="noopener noreferrer" data-no-image-dialog="true">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://cubic.dev/buttons/fix-with-cubic-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="https://cubic.dev/buttons/fix-with-cubic-light.svg">
    <img alt="Fix with Cubic" src="https://cubic.dev/buttons/fix-with-cubic-dark.svg">
  </picture>
</a>

---

## `apps/web/hooks/use-axon-acp.ts` (2)

### 1. Line 14 — `info`
**Thread:** `PRRT_kwDORS2O8s5zH21t`  

<!-- metadata:{"confidence":9} -->
P2: Invalid `editor_update.operation` values now cause the whole update to be dropped instead of falling back to `replace`.

[verification script omitted]

```suggestion
  operation: z.enum(['replace', 'append']).catch('replace'),
```

<a href="https://www.cubic.dev/action/fix/violation/f8bf9fe5-41d0-4141-8de6-6eed7ca4bc8d" target="_blank" rel="noopener noreferrer" data-no-image-dialog="true">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://cubic.dev/buttons/fix-with-cubic-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="https://cubic.dev/buttons/fix-with-cubic-light.svg">
    <img alt="Fix with Cubic" src="https://cubic.dev/buttons/fix-with-cubic-dark.svg">
  </picture>
</a>

---

### 2. Line 95 — `info`
**Thread:** `PRRT_kwDORS2O8s5zF30s`  

<!-- metadata:{"confidence":8} -->
P1: `wasActiveTurn` only checks for any active stream, so late results from a previous timed-out turn can terminate and overwrite the current turn state.

[verification script omitted]

<a href="https://www.cubic.dev/action/fix/violation/021e64c8-0a47-4b81-be58-53470eac9290" target="_blank" rel="noopener noreferrer" data-no-image-dialog="true">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://cubic.dev/buttons/fix-with-cubic-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="https://cubic.dev/buttons/fix-with-cubic-light.svg">
    <img alt="Fix with Cubic" src="https://cubic.dev/buttons/fix-with-cubic-dark.svg">
  </picture>
</a>

---

## `crates/mcp/server/artifacts/respond.rs` (2)

### 1. Line 81 — `nitpick`
**Thread:** `PRRT_kwDORS2O8s5zGbEP`  

**Extract magic number `12_000` to a named constant.**

The clip limit appears twice (lines 68, 81). A constant improves readability and ensures consistency.

```rust
const INLINE_CLIP_BYTES: usize = 12_000;
```

[verification script omitted]

<!-- fingerprinting:phantom:medusa:ocelot -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

### 2. Line 54 — `nitpick`
**Thread:** `PRRT_kwDORS2O8s5zGbEL`  

**Artifact written even when auto-inline path is taken.**

`write_json_artifact` is called unconditionally (line 31), so the file is always written to disk even when the auto-inline threshold triggers an early return. If the goal is to skip the artifact round-trip for small payloads, the size check should happen before writing.

[verification script omitted]

If the artifact is intentionally always written for audit/caching purposes, add a comment clarifying that behavior.

[verification script omitted]

<!-- fingerprinting:phantom:medusa:ocelot -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

## `crates/ingest/CLAUDE.md` (2)

### 1. Line 49 — `minor`
**Thread:** `PRRT_kwDORS2O8s5zGbEF`  

**Inconsistent documentation about Reddit comment depth.**

Line 47 states "depth configurable via `--depth`" but line 104 lists as a Known Gap: "Reddit comment depth | Fixed at top-level only — no recursive comment thread fetching." One of these is outdated.




Also applies to: 104-104

[verification script omitted]

<!-- fingerprinting:phantom:medusa:ocelot -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

### 2. Line 89 — `info`
**Thread:** `PRRT_kwDORS2O8s5zGIFe`  

<!-- metadata:{"confidence":9} -->
P2: The new helper-usage guidance is inconsistent: it says GitHub/Reddit should use `embed_text_with_metadata`, but earlier in the same doc those sources are documented as using `embed_text_with_extra_payload` for structured `gh_*`/`reddit_*` fields.

[verification script omitted]

```suggestion
Use `embed_text_with_extra_payload` when the source has structured metadata to store per-chunk (GitHub, Reddit, YouTube). Use `embed_text_with_metadata` for plain text sources (sessions).
```

<a href="https://www.cubic.dev/action/fix/violation/ddc30d70-d806-4bf9-b82d-4d838308fff1" target="_blank" rel="noopener noreferrer" data-no-image-dialog="true">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://cubic.dev/buttons/fix-with-cubic-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="https://cubic.dev/buttons/fix-with-cubic-light.svg">
    <img alt="Fix with Cubic" src="https://cubic.dev/buttons/fix-with-cubic-dark.svg">
  </picture>
</a>

---

## `crates/web/execute/sync_mode/subprocess.rs` (2)

### 1. Line 123 — `nitpick`
**Thread:** `PRRT_kwDORS2O8s5zGZ5Q`  

**Split `handle_sync_command` into stream readers and exit finalization.**

This one function is already doing stdout classification, stderr dedupe, screenshot accumulation, child waiting, and final WS bookkeeping. Breaking those phases apart will make the JSON/artifact paths much easier to reason about. As per coding guidelines, `**/*.rs`: Function size should warn at 80 lines and hard fail at 120 lines (via monolith policy); exempt test, bench, and config files.

[verification script omitted]

<!-- fingerprinting:phantom:medusa:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

### 2. Line 74 — `MAJOR`
**Thread:** `PRRT_kwDORS2O8s5zGZ5T`  

**Pretty-printed JSON is emitted twice, and screenshot fallback misses artifacts.**

Before `saw_json_line` flips, every non-JSON line is still sent as plain output immediately. A multi-line JSON object therefore gets streamed line-by-line first and then sent again as JSON in the EOF fallback, and screenshot mode loses artifacts there because `screenshot_jsons` is only populated on per-line JSON.

[verification script omitted]

[verification script omitted]

<!-- fingerprinting:phantom:medusa:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

## `docs/commands/github.md` (2)

### 1. Line 2 — `minor`
**Thread:** `PRRT_kwDORS2O8s5zGZ5W`  

**Fix markdown formatting and capitalization.**

Static analysis identified two issues:
1. The heading on line 1 should be followed by a blank line (MD022)
2. "github" should be capitalized as "GitHub" per proper noun conventions

[verification script omitted]

<!-- suggestion_start -->

[verification script omitted]

<!-- suggestion_end -->

[verification script omitted]
[verification script omitted]

</details>

[verification script omitted]

<!-- fingerprinting:phantom:poseidon:ocelot -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

### 2. Line 22 — `info`
**Thread:** `PRRT_kwDORS2O8s5zGIFx`  

<!-- metadata:{"confidence":8} -->
P3: Link to the ingest counterpart per the docs cross-link rule; this should point to `docs/ingest/github.md`, not the commands ingest doc.

[verification script omitted]

```suggestion
See [`docs/ingest/github.md`](../ingest/github.md) for full reference.
```

<a href="https://www.cubic.dev/action/fix/violation/442c912e-028e-4113-a2ce-ae3ce90598f1" target="_blank" rel="noopener noreferrer" data-no-image-dialog="true">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://cubic.dev/buttons/fix-with-cubic-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="https://cubic.dev/buttons/fix-with-cubic-light.svg">
    <img alt="Fix with Cubic" src="https://cubic.dev/buttons/fix-with-cubic-dark.svg">
  </picture>
</a>

---

## `crates/mcp/server/handlers_system.rs` (2)

### 1. Line 157 — `minor`
**Thread:** `PRRT_kwDORS2O8s5zGZ4v`  

**Reject malformed `viewport` values instead of silently defaulting.**

A bad `viewport` currently falls back to the server defaults, which hides invalid client input and makes the response nondeterministic. Return `invalid_params` when a caller supplies something other than `<width>x<height>`.

As per coding guidelines, `crates/mcp/server/**/*.rs`: Use MCP-native error handling: `ErrorData::invalid_params()` for invalid requests/params, validate required fields early in handlers and return deterministic error messages with action/subaction context.

[verification script omitted]

<!-- fingerprinting:phantom:medusa:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

### 2. Line 253 — `MAJOR`
**Thread:** `PRRT_kwDORS2O8s5zGZ40`  

**Thread `response_mode` through the artifact path operations.**

Line 216 parses `response_mode`, but Line 253 drops into `handle_artifacts_path_op(req)` without passing it through. That means the content-heavy `head`, `grep`, and `read` subactions bypass `respond_with_mode` and always return inline payloads.

As per coding guidelines, `crates/mcp/server/**/*.rs`: Default response mode to `path` for artifact-first responses, with large outputs persisted in `.cache/axon-mcp/` and inline responses capped with artifact pointers.


Also applies to: 257-323

[verification script omitted]

<!-- fingerprinting:phantom:medusa:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

## `crates/mcp/server/oauth_google/tests.rs` (2)

### 1. Line 74 — `nitpick`
**Thread:** `PRRT_kwDORS2O8s5zGZ44`  

**Test provides no coverage when CI runs inside Docker.**

The early return when `/.dockerenv` exists means this test won't exercise the normalization logic in Docker-based CI environments. If your CI runs in containers, consider an alternative approach such as:
- A separate integration test that explicitly tests the normalization function with mock data
- Using `#[ignore]` with a specific test runner configuration instead of a runtime skip

The current approach is acceptable if local development is the primary test environment.

[verification script omitted]

<!-- fingerprinting:phantom:poseidon:ocelot -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

### 2. Line 91 — `minor`
**Thread:** `PRRT_kwDORS2O8s5zGZ47`  

**Hardcoded port `53379` makes the test environment-specific.**

The assertion expects `axon-redis:6379` to be rewritten to port `53379`, which assumes a specific Docker Compose port mapping. This test will fail if the port mapping changes or in environments with different configurations.

Consider either:
1. Making the expected port configurable via an environment variable
2. Only asserting that the host is normalized (to `127.0.0.1`) and the port is *some* value (not necessarily 53379)
3. Documenting the required Docker Compose configuration for this test to pass

[verification script omitted]

<!-- fingerprinting:phantom:poseidon:ocelot -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

## `crates/services/acp/bridge.rs` (2)

### 1. Line 45 — `MAJOR`
**Thread:** `PRRT_kwDORS2O8s5zGZ4_`  

**Normalize `AXON_ACP_AUTO_APPROVE` instead of matching only `"false"`.**

Any explicit value besides the lowercase string `"false"` currently enables auto-approve, so `FALSE`, `False`, `0`, or a typo all fail open and grant tool permissions. Parse a real boolean here and default invalid values to the safer interactive path.

[verification script omitted]

[verification script omitted]

<!-- fingerprinting:phantom:medusa:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

### 2. Line 158 — `minor`
**Thread:** `PRRT_kwDORS2O8s5zF_NU`  

**Permission responder map entry not removed on successful resolution.**

When the frontend sends a valid `option_id` (lines 121-140) or an unknown one (lines 142-153), the corresponding entry in `permission_responders` is never removed. Only the timeout path (line 182) cleans up the map. Since the oneshot sender is consumed when `resp_rx` receives the value, the entry becomes a dangling `(session_id, tool_call_id) → consumed_sender` pair.

This causes a minor memory leak if the map is long-lived and many permissions are resolved interactively.


[verification script omitted]

[verification script omitted]

<!-- fingerprinting:phantom:medusa:ocelot -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

## `apps/web/__tests__/use-axon-acp-editor.test.ts` (2)

### 1. Line 18 — `minor`
**Thread:** `PRRT_kwDORS2O8s5zGZ26`  

**Add return type annotation to `handleEditorMsg`.**

The function is missing an explicit return type annotation.


[verification script omitted]

As per coding guidelines: "Use type annotations for function parameters and return types in TypeScript".

<!-- suggestion_start -->

[verification script omitted]

<!-- suggestion_end -->

[verification script omitted]

<!-- fingerprinting:phantom:poseidon:ocelot -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

### 2. Line 18 — `nitpick`
**Thread:** `PRRT_kwDORS2O8s5zGZ22`  

[verification script omitted]

**Consider extracting the editor_update case logic into a shared utility to prevent test/implementation divergence.**

The test mirror at lines 3–18 currently matches the actual `editor_update` handling in `use-axon-acp.ts` (lines 117–123) exactly. However, maintaining two copies of this logic risks future desynchronization. Extracting the core switch-case logic into a reusable utility that both the hook and test import would eliminate this maintenance burden and ensure they stay aligned.

[verification script omitted]

<!-- fingerprinting:phantom:poseidon:ocelot -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

## `apps/web/CLAUDE.md` (2)

### 1. Line 81 — `minor`
**Thread:** `PRRT_kwDORS2O8s5zGZ3C`  

**Synchronize the `/api/pulse/chat` description in this doc.**

This section now says the route uses ACP/WebSocket streaming, but the API Routes table above still documents `/api/pulse/chat` as NDJSON from a Claude CLI subprocess. The page is internally inconsistent until both descriptions are updated together.

[verification script omitted]

<!-- fingerprinting:phantom:poseidon:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

### 2. Line 76 — `info`
**Thread:** `PRRT_kwDORS2O8s5zGIF4`  

<!-- metadata:{"confidence":9} -->
P3: This new `/api/pulse/chat` description now conflicts with the API Routes table in the same doc, leaving two incompatible protocol descriptions for the same endpoint.

[verification script omitted]

<a href="https://www.cubic.dev/action/fix/violation/b7884c5e-625e-439d-9bf9-2a0f9e7b9abc" target="_blank" rel="noopener noreferrer" data-no-image-dialog="true">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://cubic.dev/buttons/fix-with-cubic-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="https://cubic.dev/buttons/fix-with-cubic-light.svg">
    <img alt="Fix with Cubic" src="https://cubic.dev/buttons/fix-with-cubic-dark.svg">
  </picture>
</a>

---

## `apps/web/hooks/use-axon-session.ts` (2)

### 1. Line 9 — `MAJOR`
**Thread:** `PRRT_kwDORS2O8s5zGZ3O`  

_🛠️ Refactor suggestion_ | _🟠 Major_

[verification script omitted]

**Prefer an interface for exported `ReasoningStep`.**

This is part of the hook's public model and should follow the repo's interface-first convention for exported object shapes.

[verification script omitted]

As per coding guidelines, `Use interfaces for all public data structures in TypeScript` and `Prefer interface over type for defining object shapes in TypeScript`.

<!-- suggestion_start -->

[verification script omitted]

<!-- suggestion_end -->

[verification script omitted]

<!-- fingerprinting:phantom:medusa:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

### 2. Line 15 — `MAJOR`
**Thread:** `PRRT_kwDORS2O8s5zGZ3Z`  

[verification script omitted]

**Don't stamp historical messages with load time.**

`AxonMessage.timestamp` is required, but `parseClaudeJsonl` extracts only `role` and `content` from the JSONL, so the parsed `SessionResponse` carries no timestamp. The hook fills this gap with `Date.now()` on every reload. Reloading the same transcript will rewrite every message's timestamp, so any consumer that sorts or renders by it (e.g., `formatTimestamp()` in `axon-message-list.tsx`) will treat old messages as new. Modify the parser to extract timestamps from the JSONL if available, or make `timestamp` optional in `AxonMessage` until the backend can provide a real value.

[verification script omitted]

<!-- fingerprinting:phantom:medusa:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

## `CLAUDE.md` (2)

### 1. Line 70 — `minor`
**Thread:** `PRRT_kwDORS2O8s5zGZ3b`  

**Remove the stale standalone `youtube` command from this table.**

`lib.rs` now routes source ingestion through `CommandKind::Ingest`, so keeping `youtube <url|playlist|channel>` here advertises a CLI entry point that no longer exists. Please replace the source-specific rows with the current `ingest` syntax and sweep the later `github`/`reddit`/`youtube` references in this file as well.

[verification script omitted]

</details>

[verification script omitted]

<!-- fingerprinting:phantom:medusa:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

### 2. Line 549 — `minor`
**Thread:** `PRRT_kwDORS2O8s5zGZ3d`  

**Add a language tag to this fenced block.**

`markdownlint-cli2` will keep warning here because the new fence has no info string. `text` or `plaintext` is enough.

[verification script omitted]

[verification script omitted]

</details>

[verification script omitted]

<!-- fingerprinting:phantom:medusa:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

## `crates/ingest/reddit/meta.rs` (2)

### 1. Line 23 — `MAJOR`
**Thread:** `PRRT_kwDORS2O8s5zGZ3h`  

[verification script omitted]

**Parse `created_utc` without losing floating-point timestamps.**

`serde_json::Value::as_u64()` returns `None` for JSON floating-point numbers. Reddit's API returns `created_utc` as `1710000000.0` (float), causing this code to silently store `0` instead of the actual post timestamp.

[verification script omitted]

<!-- suggestion_start -->

[verification script omitted]

<!-- suggestion_end -->

[verification script omitted]

<!-- fingerprinting:phantom:poseidon:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

### 2. Line 13 — `info`
**Thread:** `PRRT_kwDORS2O8s5zGIFn`  

<!-- metadata:{"confidence":8} -->
P2: `created_utc` is represented as a floating-point number in Reddit’s JSON; `as_u64()` will return `None` and default to `0` for real posts. Parse as `f64` and cast to `u64` to preserve the timestamp.

[verification script omitted]

<a href="https://www.cubic.dev/action/fix/violation/0b407113-3f6e-44f1-a05b-131d140ca522" target="_blank" rel="noopener noreferrer" data-no-image-dialog="true">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://cubic.dev/buttons/fix-with-cubic-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="https://cubic.dev/buttons/fix-with-cubic-light.svg">
    <img alt="Fix with Cubic" src="https://cubic.dev/buttons/fix-with-cubic-dark.svg">
  </picture>
</a>

---

## `crates/ingest/youtube.rs` (2)

### 1. Line 179 — `MAJOR`
**Thread:** `PRRT_kwDORS2O8s5zGZ3o`  

**Bare video IDs are rejected before they are canonicalized.**

Line 175 validates `url` as a full URL, but this function and the classifier both accept bare 11-character IDs. Inputs like `dQw4w9WgXcQ` will fail before Line 179 builds `safe_url`. Validate the canonicalized YouTube URL instead.

[verification script omitted]

<!-- suggestion_start -->

[verification script omitted]

<!-- suggestion_end -->

[verification script omitted]

<!-- fingerprinting:phantom:medusa:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

### 2. Line 102 — `info`
**Thread:** `PRRT_kwDORS2O8s5zGIFT`  

<!-- metadata:{"confidence":8} -->
P1: Validate playlist/channel URLs before invoking `yt-dlp`. Right now playlist enumeration bypasses the SSRF guard used by single-video ingest, so a malicious target can make `yt-dlp` fetch internal/private network resources.

[verification script omitted]

```suggestion
    validate_url(url)?;
    let output = tokio::process::Command::new("yt-dlp")
```

<a href="https://www.cubic.dev/action/fix/violation/b53ac3b2-149b-4b6d-aef3-881261def56a" target="_blank" rel="noopener noreferrer" data-no-image-dialog="true">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://cubic.dev/buttons/fix-with-cubic-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="https://cubic.dev/buttons/fix-with-cubic-light.svg">
    <img alt="Fix with Cubic" src="https://cubic.dev/buttons/fix-with-cubic-dark.svg">
  </picture>
</a>

---

## `crates/ingest/youtube/vtt.rs` (2)

### 1. Line 25 — `MAJOR`
**Thread:** `PRRT_kwDORS2O8s5zGZ3t`  

**Handle non-content WebVTT records before emitting transcript text.**

The current filter only skips an exact `WEBVTT` header, timestamps, and numeric cue IDs. Valid files with BOM/header text (`WEBVTT - title`), string cue IDs, or `NOTE`/`STYLE`/`REGION` blocks will leak metadata into the transcript and pollute the embedded text. Please harden the parser and add pure regression cases for those forms.

[verification script omitted]

As per coding guidelines, "Ingest unit tests must run without live services (pure logic: parsing, classification, ID extraction) via `cargo test ingest`."

[verification script omitted]

<!-- fingerprinting:phantom:medusa:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

### 2. Line 23 — `info`
**Thread:** `PRRT_kwDORS2O8s5zGIFY`  

<!-- metadata:{"confidence":9} -->
P2: Numeric-only subtitle content is always removed, causing real transcript text (e.g. `2024`) to be lost.

[verification script omitted]

<a href="https://www.cubic.dev/action/fix/violation/0ecd2ef0-e80a-47ff-add0-85e9534745ae" target="_blank" rel="noopener noreferrer" data-no-image-dialog="true">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://cubic.dev/buttons/fix-with-cubic-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="https://cubic.dev/buttons/fix-with-cubic-light.svg">
    <img alt="Fix with Cubic" src="https://cubic.dev/buttons/fix-with-cubic-dark.svg">
  </picture>
</a>

---

## `crates/services/acp.rs` (2)

### 1. Line 216 — `MAJOR`
**Thread:** `PRRT_kwDORS2O8s5zF_NQ`  

**Keep the validation bypass out of production builds.**

`spawn_adapter_skip_validation()` is still a normal public method, so any non-test caller can sidestep `validate_adapter_command()` and re-enable shell spawning through this escape hatch. Move it behind a test-only cfg/feature and keep the shared spawn logic private so the env allowlist cannot drift.

[verification script omitted]

<!-- fingerprinting:phantom:medusa:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

### 2. Line 170 — `info`
**Thread:** `PRRT_kwDORS2O8s5zF302`  

<!-- metadata:{"confidence":7} -->
P2: This test-only helper is still public in production, so it lets callers bypass `validate_adapter_command` and spawn arbitrary commands. Gate it with `#[cfg(test)]` and/or restrict visibility so it can’t be used outside tests.

[verification script omitted]

<a href="https://www.cubic.dev/action/fix/violation/754ee83a-4fa7-4352-ba16-044c827f741e" target="_blank" rel="noopener noreferrer" data-no-image-dialog="true">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://cubic.dev/buttons/fix-with-cubic-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="https://cubic.dev/buttons/fix-with-cubic-light.svg">
    <img alt="Fix with Cubic" src="https://cubic.dev/buttons/fix-with-cubic-dark.svg">
  </picture>
</a>

---

## `crates/web/execute/sync_mode/pulse_chat.rs` (2)

### 1. Line 211 — `nitpick`
**Thread:** `PRRT_kwDORS2O8s5zF_Ny`  

**System context preamble could be externalized.**

The `<axon:editor>` syntax guide is hardcoded as a string literal. If the syntax or instructions change, this will need to be updated here. Consider moving to a configuration file or constant module.



This is a minor maintainability suggestion — the current approach works fine for now.

[verification script omitted]

<!-- fingerprinting:phantom:medusa:ocelot -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

### 2. Line 304 — `nitpick`
**Thread:** `PRRT_kwDORS2O8s5zF_N1`  

**Consider extracting shared `run_acp_event_loop` to reduce duplication.**

This function is nearly identical to the one in `sync_mode.rs` (lines 695-748), differing only in the captured session ID handling. Both implement the same biased select pattern for ACP event dispatch.



Consider extracting to a shared module (e.g., `acp_event_loop.rs`) with a configurable callback or return type to avoid maintaining two copies of the same logic.

[verification script omitted]

<!-- fingerprinting:phantom:medusa:ocelot -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

## `crates/services/acp/runtime.rs` (2)

### 1. Line 51 — `MAJOR`
**Thread:** `PRRT_kwDORS2O8s5zBRKd`  

[verification script omitted]

**Reap the child on early-error cleanup.**

`start_kill()` only sends the signal; it does not reap the process. When stdio extraction fails in `spawn_adapter_with_io` (lines 48–60 in session.rs), the guard drops before `wait()` is ever spawned, leaving the child as a zombie. Repeated setup failures accumulate unreaped processes. Use `kill().await` in the error path or ensure the exit watcher is always spawned before returning.

[verification script omitted]

<!-- fingerprinting:phantom:medusa:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

### 2. Line 301 — `nitpick`
**Thread:** `PRRT_kwDORS2O8s5zBRKn`  

**Split `run_prompt_turn` back under the 80-line warning threshold.**

This function now mixes session establishment, prompt dispatch, select coordination, logging, and final bridge emission into one ~90-line block. Pulling the select/result-emission path into a helper would bring it back under policy and make the crash/clean-exit handling easier to reason about.

As per coding guidelines, `**/*.rs`: Function size should warn at 80 lines and hard fail at 120 lines (via monolith policy); exempt test, bench, and config files.

[verification script omitted]

<!-- fingerprinting:phantom:medusa:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

## `apps/web/components/ui/diff-node-static.tsx` (1)

### 1. Line 23 — `minor`
**Thread:** `PRRT_kwDORS2O8s5zIDdx`  

**Minor inconsistency: destructure `children` for consistency with the client component.**

The client-side `DiffLeaf` in `diff-node.tsx` destructures `children` from props, while this static version accesses `props.children` directly. Consider destructuring for consistency.



[verification script omitted]

<!-- suggestion_start -->

[verification script omitted]

<!-- suggestion_end -->

[verification script omitted]

<!-- fingerprinting:phantom:poseidon:ocelot -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

## `crates/web/execute/tests/acp_ws_event_tests.rs` (1)

### 1. Line 365 — `nitpick`
**Thread:** `PRRT_kwDORS2O8s5zIDeQ`  

**Snapshot the actual WS envelope, not just the inner payload.**

Lines 354-359 only snapshot the raw `editor_update` object, so changes to the `WsEventV2::CommandOutputJson` wrapper would bypass this guard even though the docstring says it protects the exact JSON the frontend receives.

[verification script omitted]

[verification script omitted]

<!-- fingerprinting:phantom:poseidon:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

## `docs/ARCHITECTURE.md` (1)

### 1. Line 307 — `nitpick`
**Thread:** `PRRT_kwDORS2O8s5zIDeV`  

**Consider adding a language identifier to the fenced code block.**

The directory tree structure at line 287 lacks a language identifier. While not strictly code, adding `text` or `plaintext` satisfies markdown linting and clarifies intent.


[verification script omitted]

[verification script omitted]

</details>

[verification script omitted]

<!-- fingerprinting:phantom:poseidon:ocelot -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

## `docs/REBOOT-UI.md` (1)

### 1. Line 6 — `nitpick`
**Thread:** `PRRT_kwDORS2O8s5zIDeY`  

**Consider adding a blank line after the heading.**

Markdown best practices suggest surrounding headings with blank lines for improved readability and linter compliance.


[verification script omitted]

<!-- suggestion_start -->

[verification script omitted]

<!-- suggestion_end -->

[verification script omitted]

</details>

[verification script omitted]

<!-- fingerprinting:phantom:poseidon:ocelot -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

## `apps/web/__tests__/axon-message-list-editor-blocks.test.ts` (1)

### 1. Line 10 — `MAJOR`
**Thread:** `PRRT_kwDORS2O8s5zIDdI`  

**Test the production parser instead of a copied regex.**

This suite currently verifies a local reimplementation, not the code that ships. If `axon-message-list.tsx` changes and this copied helper does not, these tests still pass and give false confidence. Please extract/export the production helper and assert against that implementation directly.

[verification script omitted]

<!-- fingerprinting:phantom:poseidon:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

## `apps/web/__tests__/use-axon-session-retry.test.ts` (1)

### 1. Line 31 — `MAJOR`
**Thread:** `PRRT_kwDORS2O8s5zIDdO`  

**Avoid mirroring `fetchSessionWithRetry` inside the test.**

These tests exercise a local copy of the retry algorithm, so they do not protect the real implementation in `use-axon-session.ts` from drifting. Extract the helper into a small shared module and import it here; otherwise a production regression can coexist with a green test suite.

[verification script omitted]

<!-- fingerprinting:phantom:poseidon:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

## `apps/web/components/ai-elements/artifact.tsx` (1)

### 1. Line 66 — `nitpick`
**Thread:** `PRRT_kwDORS2O8s5zIDdR`  

**Consider lifting `TooltipProvider` to a parent component.**

Each `ArtifactAction` creates its own `TooltipProvider`. When multiple actions are rendered together inside `ArtifactActions`, this creates redundant providers and may cause tooltip coordination issues. The provider should typically wrap the group of tooltips, not each individual one.


[verification script omitted]

[verification script omitted]

<!-- fingerprinting:phantom:poseidon:ocelot -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

## `apps/web/components/editor/plugins/diff-kit.tsx` (1)

### 1. Line 11 — `nitpick`
**Thread:** `PRRT_kwDORS2O8s5zIDdb`  

[verification script omitted]

**Consider moving `computeDiff` to a non-client module if server components need it.**

The `'use client'` directive on line 1 makes this a client boundary, and re-exporting `computeDiff` from a client module prevents server components from importing it. Currently, only `apps/web/components/editor/plugins/copilot-kit.tsx` (a client component) imports from this module. If server-side code needs access to `computeDiff` in the future, extract the re-export to a separate non-client module:

[verification script omitted]

[verification script omitted]

<!-- fingerprinting:phantom:medusa:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

## `apps/web/app/api/sessions/list/route.ts` (1)

### 1. Line 23 — `info`
**Thread:** `PRRT_kwDORS2O8s5zH21Y`  

<!-- metadata:{"confidence":9} -->
P1: `scanSessions` already includes Codex/Gemini, so calling it alongside `scanCodexSessions` and `scanGeminiSessions` double-scans and mixes datasets incorrectly.

[verification script omitted]

<a href="https://www.cubic.dev/action/fix/violation/4ec36940-5ae6-4144-84a7-d8ad17f0757c" target="_blank" rel="noopener noreferrer" data-no-image-dialog="true">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://cubic.dev/buttons/fix-with-cubic-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="https://cubic.dev/buttons/fix-with-cubic-light.svg">
    <img alt="Fix with Cubic" src="https://cubic.dev/buttons/fix-with-cubic-dark.svg">
  </picture>
</a>

---

## `apps/web/lib/sessions/session-utils.ts` (1)

### 1. Line 47 — `info`
**Thread:** `PRRT_kwDORS2O8s5zH21h`  

<!-- metadata:{"confidence":9} -->
P2: `mapWithConcurrency` does not validate `concurrency`, so `0`/negative values return an unprocessed sparse result array.

[verification script omitted]

<a href="https://www.cubic.dev/action/fix/violation/e08da322-1ca5-4d7a-867a-8319ac689b5f" target="_blank" rel="noopener noreferrer" data-no-image-dialog="true">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://cubic.dev/buttons/fix-with-cubic-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="https://cubic.dev/buttons/fix-with-cubic-light.svg">
    <img alt="Fix with Cubic" src="https://cubic.dev/buttons/fix-with-cubic-dark.svg">
  </picture>
</a>

---

## `apps/web/lib/sessions/gemini-json-parser.ts` (1)

### 1. Line 27 — `info`
**Thread:** `PRRT_kwDORS2O8s5zH21i`  

<!-- metadata:{"confidence":9} -->
P2: Guard `msg.content` with a runtime string check before calling `.trim()` to avoid crashes on malformed JSON.

[verification script omitted]

<a href="https://www.cubic.dev/action/fix/violation/9eb07ebc-7c66-43df-9c24-100c8dc36d87" target="_blank" rel="noopener noreferrer" data-no-image-dialog="true">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://cubic.dev/buttons/fix-with-cubic-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="https://cubic.dev/buttons/fix-with-cubic-light.svg">
    <img alt="Fix with Cubic" src="https://cubic.dev/buttons/fix-with-cubic-dark.svg">
  </picture>
</a>

---

## `docs/ingest/github.md` (1)

### 1. Line 60 — `info`
**Thread:** `PRRT_kwDORS2O8s5zH21v`  

<!-- metadata:{"confidence":10} -->
P3: The new repository-chunk heading incorrectly documents `gh_is_pr: false`; that field is not emitted for repository/file/wiki chunks.

[verification script omitted]

```suggestion
### Repository chunks (file and wiki chunks)
```

<a href="https://www.cubic.dev/action/fix/violation/c536d174-fc2a-46d6-9302-2f9f223951a5" target="_blank" rel="noopener noreferrer" data-no-image-dialog="true">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://cubic.dev/buttons/fix-with-cubic-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="https://cubic.dev/buttons/fix-with-cubic-light.svg">
    <img alt="Fix with Cubic" src="https://cubic.dev/buttons/fix-with-cubic-dark.svg">
  </picture>
</a>

---

## `docs/commands/reddit.md` (1)

### 1. Line 20 — `MAJOR`
**Thread:** `PRRT_kwDORS2O8s5zGZ5i`  

**Keep the deprecated Reddit page in the command-doc format.**

This replacement removes the required command reference sections and omits the mandated cross-link to `docs/ingest/reddit.md`, so users following old `axon reddit` links lose the flags, job subcommands, and env guidance the command docs are supposed to preserve during migrations.

As per coding guidelines, `docs/commands/*.md`: CLI command reference files should include synopsis/usage line, arguments table, all flags and defaults (including command-specific flags), job subcommands (status, cancel, list, cleanup, clear, recover, worker), concrete usage examples, required environment variables (brief), and one-line install instructions for external dependencies. Every command file in docs/commands/ must include a cross-link to its ingest counterpart with the text: '> For implementation details and troubleshooting see [`docs/ingest/<name>.md`](../ingest/<name>.md).'

[verification script omitted]

</details>

[verification script omitted]

<!-- fingerprinting:phantom:poseidon:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

## `docs/commands/youtube.md` (1)

### 1. Line 21 — `MAJOR`
**Thread:** `PRRT_kwDORS2O8s5zGZ5l`  

**Keep the deprecated YouTube page in the command-doc format.**

This page also drops the required synopsis/flags/job-subcommand/env/install sections, and the backlink at Line 21 is not the exact mandated ingest cross-link. Deprecated command pages still need the standard command reference contract.

As per coding guidelines, `docs/commands/*.md`: CLI command reference files should include synopsis/usage line, arguments table, all flags and defaults (including command-specific flags), job subcommands (status, cancel, list, cleanup, clear, recover, worker), concrete usage examples, required environment variables (brief), and one-line install instructions for external dependencies. Every command file in docs/commands/ must include a cross-link to its ingest counterpart with the text: '> For implementation details and troubleshooting see [`docs/ingest/<name>.md`](../ingest/<name>.md).'

[verification script omitted]
[verification script omitted]

</details>

[verification script omitted]

<!-- fingerprinting:phantom:poseidon:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

## `docs/MCP-TOOL-SCHEMA.md` (1)

### 1. Line 98 — `MAJOR`
**Thread:** `PRRT_kwDORS2O8s5zGZ5n`  

**Fix the generated artifact parameter contract.**

After adding `list|clean|search`, `path` is no longer universally required for `artifacts`. Emitting that from the auto-generated “source of truth” schema will steer clients into invalid request shapes for `list`/`clean` and still omits `max_age_hours` for `clean`. Please update the generator/source metadata rather than patching this markdown by hand.

[verification script omitted]

<!-- fingerprinting:phantom:medusa:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

## `docs/MCP.md` (1)

### 1. Line 238 — `MAJOR`
**Thread:** `PRRT_kwDORS2O8s5zGZ5o`  

**Don’t overload `response_mode` with `auto-inline` without updating the schema.**

`docs/MCP-TOOL-SCHEMA.md` still defines `ResponseMode` as `path|inline|both`. Documenting successful responses with `"response_mode": "auto-inline"` makes the published contract inconsistent and will break clients that deserialize that field as the documented enum. Either rename the response field or extend the canonical schema.

[verification script omitted]

<!-- fingerprinting:phantom:medusa:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

## `crates/mcp/server/artifacts/path.rs` (1)

### 1. Line 132 — `CRITICAL`
**Thread:** `PRRT_kwDORS2O8s5zGZ4c`  

**Reject symlink-backed output paths too.**

Lines 120-132 only block lexical traversal. A client can still target `nested/out.json` where `nested` or an existing `out.json` is a symlink under the artifact root, and the subsequent write will follow it outside the sandbox. Please canonicalize the parent chain against the artifact root and perform the create/write without following symlinks.

[verification script omitted]

<!-- fingerprinting:phantom:medusa:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

## `crates/mcp/server/artifacts/shape.rs` (1)

### 1. Line 69 — `minor`
**Thread:** `PRRT_kwDORS2O8s5zGZ4k`  

**Use character counts here, not UTF-8 byte lengths.**

Lines 68-69 use `s.len()`, so non-ASCII strings get summarized earlier than the documented 100-character cutoff and `<string N>` reports bytes rather than displayed characters. `clip_inline_json()` already uses `chars()`, so Unicode payloads will get inconsistent previews.



[verification script omitted]

<!-- suggestion_start -->

[verification script omitted]

<!-- suggestion_end -->

[verification script omitted]

<!-- fingerprinting:phantom:medusa:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

## `crates/vector/ops/tei.rs` (1)

### 1. Line 69 — `CRITICAL`
**Thread:** `PRRT_kwDORS2O8s5zGZ5G`  

**Do not delete the existing document before the replacement upsert succeeds.**

Line 69 clears every point for this URL before Line 106 writes the new set. If the later upsert fails or times out, the previously indexed document is lost entirely. Please switch this to an upsert-first + stale-tail cleanup flow.



Also applies to: 106-106

[verification script omitted]

<!-- fingerprinting:phantom:medusa:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

## `crates/web/execute/sync_mode/dispatch.rs` (1)

### 1. Line 159 — `nitpick`
**Thread:** `PRRT_kwDORS2O8s5zGZ5O`  

**Split `dispatch_service` before it grows further.**

This dispatcher is already over 100 lines and mixes routing with payload shaping for every mode. Pulling the simple passthrough arms into small helpers will keep future mode additions out of one async match body. As per coding guidelines, `**/*.rs`: Function size should warn at 80 lines and hard fail at 120 lines (via monolith policy); exempt test, bench, and config files.

[verification script omitted]

<!-- fingerprinting:phantom:medusa:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

## `apps/web/components/reboot/axon-sidebar.tsx` (1)

### 1. Line 66 — `nitpick`
**Thread:** `PRRT_kwDORS2O8s5zGZ3D`  

**Keep the sidebar metadata in the shared UI config.**

`PAGE_ITEMS` and `AGENT_ITEMS` are now hard-coded here while `RAIL_MODES` and other reboot UI metadata live in separate modules. That recreates multiple sources of truth, so route or agent updates will be easy to miss. Consider moving these collections into `axon-ui-config.ts` or a dedicated shared config module and importing them here.

[verification script omitted]

<!-- fingerprinting:phantom:poseidon:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

## `apps/web/components/reboot/axon-ui-config.ts` (1)

### 1. Line 11 — `MAJOR`
**Thread:** `PRRT_kwDORS2O8s5zGZ3K`  

_🛠️ Refactor suggestion_ | _🟠 Major_

**Export a named interface for the shared rail items.**

`RAIL_MODES` is a public config surface, but its element shape is still an anonymous `Array<{ ... }>` type. Please promote that shape to an exported `interface` and expose the collection as `ReadonlyArray` so other consumers can reuse it without repeating the structure.

[verification script omitted]



As per coding guidelines, `**/*.{ts,tsx}`: Use interfaces for all public data structures in TypeScript` and `apps/web/**/*.{ts,tsx}`: Prefer interface over type for defining object shapes in TypeScript.

[verification script omitted]

<!-- fingerprinting:phantom:poseidon:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

## `crates/core/config/cli.rs` (1)

### 1. Line 243 — `MAJOR`
**Thread:** `PRRT_kwDORS2O8s5zGZ3f`  

[verification script omitted]

**Use `ArgAction::SetTrue` instead of `ArgAction::Set` for these bool flags.**

With `ArgAction::Set`, clap requires an explicit value (e.g., `--include-source true`), so bare `--include-source` and `--scrape-links` will fail. `SetTrue` restores the standard presence-flag behavior where the flag's presence alone sets the value to `true`.

[verification script omitted]

Also applies to: 261–263

[verification script omitted]

<!-- fingerprinting:phantom:medusa:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

## `crates/ingest/github/meta.rs` (1)

### 1. Line 53 — `info`
**Thread:** `PRRT_kwDORS2O8s5zGIFv`  

<!-- metadata:{"confidence":6} -->
P2: `gh_comment_count` is missing from the PR extra payload, even though the GitHub metadata contract documents comment_count for PR chunks. Add the PR comment count so PR and issue payloads stay consistent.

[verification script omitted]

<a href="https://www.cubic.dev/action/fix/violation/e6c060b7-87cf-4d95-b0aa-e1e067aaec39" target="_blank" rel="noopener noreferrer" data-no-image-dialog="true">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://cubic.dev/buttons/fix-with-cubic-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="https://cubic.dev/buttons/fix-with-cubic-light.svg">
    <img alt="Fix with Cubic" src="https://cubic.dev/buttons/fix-with-cubic-dark.svg">
  </picture>
</a>

---

## `docs/ingest/youtube.md` (1)

### 1. Line 4 — `info`
**Thread:** `PRRT_kwDORS2O8s5zGIF0`  

<!-- metadata:{"confidence":8} -->
P3: The ingest docs must link to their command counterpart (`docs/commands/<name>.md`). This should point to `docs/commands/youtube.md`, not `ingest.md`, to follow the documented cross-linking rule.

[verification script omitted]

<a href="https://www.cubic.dev/action/fix/violation/63f3304b-4097-4b58-8209-ecdcb7edc552" target="_blank" rel="noopener noreferrer" data-no-image-dialog="true">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://cubic.dev/buttons/fix-with-cubic-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="https://cubic.dev/buttons/fix-with-cubic-light.svg">
    <img alt="Fix with Cubic" src="https://cubic.dev/buttons/fix-with-cubic-dark.svg">
  </picture>
</a>

---

## `crates/web/execute/sync_mode/params.rs` (1)

### 1. Line 67 — `info`
**Thread:** `PRRT_kwDORS2O8s5zGIFi`  

<!-- metadata:{"confidence":7} -->
P2: Use the `max_points` flag for retrieve options. Reading `limit` here ignores the actual `max_points` field and prevents callers from setting it independently.

[verification script omitted]

<a href="https://www.cubic.dev/action/fix/violation/4f846c0b-4a6d-4a59-963f-020110b50142" target="_blank" rel="noopener noreferrer" data-no-image-dialog="true">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://cubic.dev/buttons/fix-with-cubic-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="https://cubic.dev/buttons/fix-with-cubic-light.svg">
    <img alt="Fix with Cubic" src="https://cubic.dev/buttons/fix-with-cubic-dark.svg">
  </picture>
</a>

---

## `crates/services/acp/config.rs` (1)

### 1. Line 110 — `minor`
**Thread:** `PRRT_kwDORS2O8s5zF_NX`  

**Don't return a `current_value` that's missing from `options`.**

`read_codex_default_model().await` can yield a slug that is no longer present in `models_cache.json`. In that case this returns a select option whose `current_value` cannot actually be selected, which leaves the client in an invalid state. Clamp the fallback to a value present in `options` or inject the missing model before returning.

[verification script omitted]

[verification script omitted]

<!-- fingerprinting:phantom:poseidon:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

## `crates/services/acp/mapping.rs` (1)

### 1. Line 374 — `minor`
**Thread:** `PRRT_kwDORS2O8s5zF_NY`  

**Reject unusable session CWDs at this boundary.**

`validate_session_cwd()` still accepts absolute file paths and missing directories, so `prepare_session_*()` can build ACP requests that only fail later during session startup. Require the path to exist and be a directory here.

[verification script omitted]

<!-- suggestion_start -->

[verification script omitted]

<!-- suggestion_end -->

[verification script omitted]

<!-- fingerprinting:phantom:medusa:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

## `crates/services/acp/session.rs` (1)

### 1. Line 400 — `nitpick`
**Thread:** `PRRT_kwDORS2O8s5zF_Nm`  

**Use already-computed `requested_model` instead of recomputing.**

Line 397 calls `normalized_requested_model(model)` again when `requested_model` was already computed at line 328. This is a minor inefficiency.


[verification script omitted]

[verification script omitted]

<!-- fingerprinting:phantom:medusa:ocelot -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

## `crates/web/execute/session_guard.rs` (1)

### 1. Line 17 — `MAJOR`
**Thread:** `PRRT_kwDORS2O8s5zF_Nw`  

**`HOME` environment variable is not set on Windows.**

`std::env::var("HOME")` returns the Unix home directory. On Windows, the equivalent is `USERPROFILE` or `HOMEPATH`. This function will always return `None` on Windows, preventing session file polling from working.


[verification script omitted]

[verification script omitted]

<!-- fingerprinting:phantom:medusa:ocelot -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

## `tests/services_acp_security.rs` (1)

### 1. Line 236 — `MAJOR`
**Thread:** `PRRT_kwDORS2O8s5zF_N3`  

**Restore the original environment instead of always removing it.**

These tests overwrite process env vars and then unconditionally call `remove_var(...)`. If the runner already had proxy vars or `CLAUDECODE` set, this permanently strips them for the rest of the suite; and if a panic happens before cleanup, the poisoned values leak forward instead. Capture the previous values and restore them in a drop guard so the test process exits each case with the same environment it started with.



Also applies to: 252-277

[verification script omitted]

<!-- fingerprinting:phantom:poseidon:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

## `apps/web/app/api/sessions/[id]/route.ts` (1)

### 1. Line 23 — `nitpick`
**Thread:** `PRRT_kwDORS2O8s5zF_Mv`  

**Consider removing `X-Retry-After` header for 404 responses.**

The `X-Retry-After` header is typically used with 429 (Too Many Requests) or 503 (Service Unavailable) to indicate when the client should retry. For a 404, the resource doesn't exist, so retrying won't help unless there's an expectation of eventual consistency. If the intent is to handle race conditions during session creation, consider documenting that rationale.

[verification script omitted]

<!-- fingerprinting:phantom:poseidon:ocelot -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

## `Cargo.toml` (1)

### 1. Line 57 — `nitpick`
**Thread:** `PRRT_kwDORS2O8s5zBRKW`  

[verification script omitted]

**Trim `toml` dependency to parse-only feature.**

The ACP code only uses `content.parse()` and `value.get().as_str()` for deserialization. The default `display` feature (which includes serialization support) is unnecessary. Change to:

```toml
toml = { version = "0.8", default-features = false, features = ["parse"] }
```

This removes serialization surface without impacting parsing functionality.

[verification script omitted]

<!-- fingerprinting:phantom:poseidon:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

## `crates/web/execute/events.rs` (1)

### 1. Line 170 — `MAJOR`
**Thread:** `PRRT_kwDORS2O8s5zBRKq`  

**Reconcile this helper with the documented WS type contract.**

`serialize_raw_output_event()` hardcodes `{"type":"command.output.json", ...}`, so this change adds another top-level server event outside the contract for this file. If the `command.*` schema is still intended, the source of truth needs to be updated before we duplicate the old shape again.

As per coding guidelines, `crates/web/execute/events.rs`: Server-to-Client types must be `output`, `log`, `done`, `error`, or `stats` on the top-level `type` field.

[verification script omitted]

<!-- fingerprinting:phantom:medusa:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

## `crates/web/execute/sync_mode.rs` (1)

### 1. Line 427 — `MAJOR`
**Thread:** `PRRT_kwDORS2O8s5zBRKx`  

**Normalize `.exe` before checking the shell blocklist.**

This catches `cmd.exe` and `powershell.exe`, but `bash.exe`, `sh.exe`, `zsh.exe`, `pwsh.exe`, and similar Windows shell executables still pass because the comparison is against the full basename. Strip a trailing `.exe` once and compare the stem.

[verification script omitted]

[verification script omitted]

<!-- fingerprinting:phantom:medusa:grasshopper -->

<!-- This is an auto-generated comment by CodeRabbit -->

---

