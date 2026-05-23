# DVO5A MCP Ingest Parser Extraction Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove duplicated ingest target parsing from the MCP handler and route MCP ingest.start through the existing service-owned parser.

**Architecture:** `src/services/ingest/request.rs` remains the single parser for MCP-shaped ingest requests. MCP keeps only transport concerns: request dispatch, service invocation, and `String` to `rmcp::ErrorData` mapping. Parser behavior tests move to the service side; MCP keeps only transport/error-shape tests where useful.

**Deferred architecture debt:** the service parser still accepts `crate::mcp::schema::IngestRequest`. That dependency already exists and is documented in `src/services/CLAUDE.md`; do not solve it in this slice. A later cleanup should introduce a service-owned DTO so `services` no longer imports MCP schema types.

**Security boundary:** this refactor must preserve existing provider URL validation behavior and does not claim to solve SSRF or outbound egress policy for GitLab, Gitea, generic Git, Reddit, or YouTube ingest.

**Tech Stack:** Rust 2024, Tokio, rmcp, axon service layer, sidecar `_tests.rs` convention.

---

## Research Summary

Target bead: `axon_rust-dvo.5`.

Current code reality:

- `src/services/ingest/request.rs::source_from_mcp_request` already owns service-side parsing for MCP-shaped ingest requests.
- `src/services/action_api/commands/helpers.rs::parse_ingest_source` already delegates to `source_from_mcp_request`.
- `src/mcp/server/handlers_embed_ingest.rs` still has a duplicate private `parse_ingest_source` and `validate_mcp_reddit_target`.
- Existing MCP parser tests in `src/mcp/server/handlers_embed_ingest_tests.rs` mostly test parser behavior that should now live in `src/services/ingest_tests.rs`.
- This slice must not touch artifact persistence, SSRF validation, REST routing, or response envelope unification.

Important scope correction:

- `/v1/actions` is no longer a live external HTTP route, but `src/services/action_api` still exists internally and is used by the web panel command path. For this slice, only verify that it continues to call the service parser; do not redesign it.

## File Structure

Modify:

- `src/services/ingest_tests.rs`  
  Add service-owned parser coverage for the cases currently covered only by the MCP handler tests: GitLab, Gitea, generic git, YouTube handle, missing `source_type`, and sessions default mapping.

- `src/mcp/server/handlers_embed_ingest.rs`  
  Delete the private MCP parser and Reddit validator. Change `handle_ingest_start` to call `source_from_mcp_request(&req, self.cfg.as_ref()).map_err(invalid_params)?`.

- `src/mcp/server/handlers_embed_ingest_tests.rs`  
  Remove parser-behavior tests tied to the deleted private function. Keep one handler-level transport test proving service parser failures are mapped to MCP `INVALID_PARAMS`.

No new production files.

## Task 1: Move Parser Behavior Coverage To Services

**Files:**

- Modify: `src/services/ingest_tests.rs`
- Read-only reference: `src/mcp/server/handlers_embed_ingest_tests.rs`

- [ ] **Step 1: Add service parser tests for MCP-only cases**

Add these tests after `source_from_mcp_request_rejects_invalid_youtube_target` in `src/services/ingest_tests.rs`:

```rust
#[test]
fn source_from_mcp_request_normalizes_supported_git_targets() {
    let cfg = Config::test_default();

    let gitlab = source_from_mcp_request(
        &ingest_req(
            IngestSourceType::Gitlab,
            "https://gitlab.com/group/subgroup/project/-/issues/1",
        ),
        &cfg,
    )
    .expect("valid gitlab target");
    assert!(matches!(
        gitlab,
        IngestSource::Gitlab {
            target,
            include_source,
        } if target == "gitlab.com/group/subgroup/project"
            && include_source == cfg.github_include_source
    ));

    let gitea = source_from_mcp_request(
        &ingest_req(
            IngestSourceType::Gitea,
            "gitea:gitea.example.com/org/repo.git",
        ),
        &cfg,
    )
    .expect("valid gitea target");
    assert!(matches!(
        gitea,
        IngestSource::Gitea {
            target,
            include_source,
        } if target == "gitea.example.com/org/repo"
            && include_source == cfg.github_include_source
    ));

    let generic = source_from_mcp_request(
        &ingest_req(
            IngestSourceType::Git,
            "git:https://example.com/org/repo.git",
        ),
        &cfg,
    )
    .expect("valid generic git target");
    assert!(matches!(
        generic,
        IngestSource::GenericGit {
            target,
            include_source,
        } if target == "https://example.com/org/repo.git"
            && include_source == cfg.github_include_source
    ));
}

#[test]
fn source_from_mcp_request_respects_include_source_override() {
    let mut cfg = Config::test_default();
    cfg.github_include_source = true;
    let mut req = ingest_req(IngestSourceType::Github, "https://github.com/owner/repo.git");
    req.include_source = Some(false);

    let source = source_from_mcp_request(&req, &cfg).expect("valid github target");

    assert!(matches!(
        source,
        IngestSource::Github {
            repo,
            include_source: false,
        } if repo == "owner/repo"
    ));
}

#[test]
fn source_from_mcp_request_accepts_youtube_handle() {
    let cfg = Config::test_default();
    let source = source_from_mcp_request(
        &ingest_req(
            IngestSourceType::Youtube,
            "https://www.youtube.com/@SpaceinvaderOne",
        ),
        &cfg,
    )
    .expect("valid youtube channel target");

    assert!(
        matches!(source, IngestSource::Youtube { target } if target.contains("@SpaceinvaderOne"))
    );
}

#[test]
fn source_from_mcp_request_accepts_youtube_playlist_url() {
    let cfg = Config::test_default();
    let source = source_from_mcp_request(
        &ingest_req(
            IngestSourceType::Youtube,
            "https://www.youtube.com/playlist?list=PL1234567890abcdef",
        ),
        &cfg,
    )
    .expect("valid youtube playlist target");

    assert!(matches!(source, IngestSource::Youtube { target } if target.contains("playlist")));
}

#[test]
fn source_from_mcp_request_rejects_non_reddit_comments_url() {
    let cfg = Config::test_default();
    let err = source_from_mcp_request(
        &ingest_req(
            IngestSourceType::Reddit,
            "https://example.com/r/rust/comments/abc/title",
        ),
        &cfg,
    )
    .expect_err("non-reddit thread URL should fail");

    assert!(err.contains("Reddit") || err.contains("reddit"));
}

#[test]
fn source_from_mcp_request_requires_source_type() {
    let cfg = Config::test_default();
    let req = IngestRequest {
        target: Some("owner/repo".to_string()),
        ..Default::default()
    };

    let err = source_from_mcp_request(&req, &cfg).expect_err("missing source type");

    assert!(err.contains("source_type is required"));
}

#[test]
fn source_from_mcp_request_requires_target_for_github() {
    let cfg = Config::test_default();
    let req = IngestRequest {
        source_type: Some(IngestSourceType::Github),
        ..Default::default()
    };

    let err = source_from_mcp_request(&req, &cfg).expect_err("missing github target");

    assert!(err.contains("target is required"));
}

#[test]
fn source_from_mcp_request_maps_default_sessions_options() {
    let cfg = Config::test_default();
    let req = IngestRequest {
        source_type: Some(IngestSourceType::Sessions),
        ..Default::default()
    };

    let source = source_from_mcp_request(&req, &cfg).expect("default sessions options");

    assert!(matches!(
        source,
        IngestSource::Sessions {
            sessions_claude: false,
            sessions_codex: false,
            sessions_gemini: false,
            sessions_project: None,
        }
    ));
}
```

- [ ] **Step 2: Run the service parser tests**

Run:

```bash
cargo test source_from_mcp_request --lib
```

Expected: all `source_from_mcp_request_*` tests pass.

- [ ] **Step 3: Confirm behavior parity before deleting MCP parser**

Confirm service coverage includes GitHub, GitLab, Gitea, generic Git, Reddit subreddit/thread rejection, YouTube handle, YouTube playlist/channel-style URL, sessions defaults, missing source type, missing target, and `include_source` override/default behavior.

## Task 2: Route MCP Ingest Start Through The Service Parser

**Files:**

- Modify: `src/mcp/server/handlers_embed_ingest.rs`

- [ ] **Step 1: Replace the private parser imports**

In `src/mcp/server/handlers_embed_ingest.rs`, update the imports near the top.

Replace:

```rust
use crate::core::config::Config;
use crate::mcp::schema::{
    AxonToolResponse, EmbedRequest, EmbedSubaction, IngestRequest, IngestSourceType,
    IngestSubaction, ResponseMode, SessionsIngestOptions,
};
```

with:

```rust
use crate::mcp::schema::{
    AxonToolResponse, EmbedRequest, EmbedSubaction, IngestRequest, IngestSubaction, ResponseMode,
};
```

Then replace:

```rust
use crate::services::ingest::{
    IngestSource, ingest_cancel, ingest_cleanup, ingest_clear, ingest_list, ingest_recover,
    ingest_start_with_context, ingest_status,
};
```

with:

```rust
use crate::services::ingest::{
    ingest_cancel, ingest_cleanup, ingest_clear, ingest_list, ingest_recover,
    ingest_start_with_context, ingest_status, source_from_mcp_request,
};
```

- [ ] **Step 2: Delete the private MCP parser**

Delete these functions completely from `src/mcp/server/handlers_embed_ingest.rs`:

```rust
fn parse_ingest_source(
    source_type: Option<IngestSourceType>,
    target: Option<String>,
    sessions: Option<SessionsIngestOptions>,
    include_source: Option<bool>,
    cfg: &Config,
) -> Result<IngestSource, ErrorData> {
    ...
}

fn validate_mcp_reddit_target(target: &str) -> Result<(), ErrorData> {
    ...
}
```

- [ ] **Step 3: Change `handle_ingest_start` to call the service parser**

Replace:

```rust
async fn handle_ingest_start(
    &self,
    mut req: IngestRequest,
) -> Result<AxonToolResponse, ErrorData> {
    let source = parse_ingest_source(
        req.source_type.take(),
        req.target.take(),
        req.sessions.take(),
        req.include_source,
        self.cfg.as_ref(),
    )?;
```

with:

```rust
async fn handle_ingest_start(&self, req: IngestRequest) -> Result<AxonToolResponse, ErrorData> {
    let source =
        source_from_mcp_request(&req, self.cfg.as_ref()).map_err(|message| invalid_params(message))?;
```

If `cargo fmt` wraps the line differently, keep the same behavior.

- [ ] **Step 4: Verify the deleted symbols are gone**

Run:

```bash
rg -n "fn parse_ingest_source|fn validate_mcp_reddit_target" src/mcp/server/handlers_embed_ingest.rs
rg -n "source_from_mcp_request" src/mcp/server/handlers_embed_ingest.rs
```

Expected: first command has no output; second command shows the service parser import and handler call.

- [ ] **Step 5: Run focused compile/test check**

Run:

```bash
cargo test source_from_mcp_request --lib
```

Expected: tests pass.

- [ ] **Step 6: Leave changes uncommitted until final validation**

Do not make task-level commits. Commit once after the full focused validation passes.

## Task 3: Replace MCP Parser Tests With Transport-Focused Coverage

**Files:**

- Modify: `src/mcp/server/handlers_embed_ingest_tests.rs`

- [ ] **Step 1: Replace parser unit tests with one handler-level MCP error-mapping test**

Replace the contents of `src/mcp/server/handlers_embed_ingest_tests.rs` with:

```rust
use super::*;

#[tokio::test]
async fn mcp_ingest_start_maps_service_parser_errors_to_invalid_params() {
    let server = crate::mcp::server::AxonMcpServer::new(crate::core::config::Config::default());
    let req = IngestRequest {
        source_type: Some(crate::mcp::schema::IngestSourceType::Github),
        target: Some("owner/repo/extra".to_string()),
        ..Default::default()
    };

    let err = server
        .handle_ingest(req)
        .await
        .expect_err("invalid target should fail");

    assert_eq!(err.code, rmcp::model::ErrorCode::INVALID_PARAMS);
}
```

This must call `handle_ingest`, not `source_from_mcp_request` directly, so the test fails if MCP stops using the service parser or maps parser failures as internal errors.

- [ ] **Step 2: Run MCP ingest handler tests**

Run:

```bash
cargo test handlers_embed_ingest --lib
```

Expected: the transport-focused test passes.

- [ ] **Step 3: Run the combined focused test set**

Run:

```bash
cargo test source_from_mcp_request --lib
cargo test handlers_embed_ingest --lib
```

Expected: both commands pass.

- [ ] **Step 4: Leave changes uncommitted until final validation**

Do not make task-level commits. Commit once after the full focused validation passes.

## Task 4: Final Validation And Tracker Update

**Files:**

- Tracker: `axon_rust-dvo.5`

- [ ] **Step 1: Format the touched files**

Run:

```bash
cargo fmt
```

Expected: command exits 0.

- [ ] **Step 2: Run focused validation**

Run:

```bash
cargo test source_from_mcp_request --lib
cargo test handlers_embed_ingest --lib
```

Expected: all pass.

- [ ] **Step 3: Run repo-level check**

Run:

```bash
cargo check
```

Expected: command exits 0.

- [ ] **Step 4: Verify handler no longer owns parser logic**

Run:

```bash
rg -n "parse_ingest_source|validate_mcp_reddit_target" src/mcp/server/handlers_embed_ingest.rs src/services/action_api/commands/helpers.rs src/services/ingest/request.rs
```

Expected:

```text
src/services/action_api/commands/helpers.rs:6:pub(super) fn parse_ingest_source(
```

`src/mcp/server/handlers_embed_ingest.rs` must not appear. `src/services/ingest/request.rs` may appear only for validation/helper names if implementation names include the phrase; if it does, confirm the logic is service-owned.

- [ ] **Step 5: Add tracker comment**

Run:

```bash
bd comments add axon_rust-dvo.5 "IMPLEMENTED: MCP ingest.start now routes target parsing through services::ingest::source_from_mcp_request; duplicate MCP parser and Reddit validator removed. Parser behavior coverage lives in src/services/ingest_tests.rs; MCP keeps only transport error mapping coverage."
```

- [ ] **Step 6: Commit final validated changes**

Run:

```bash
git status --short
git add src/services/ingest_tests.rs src/mcp/server/handlers_embed_ingest.rs src/mcp/server/handlers_embed_ingest_tests.rs
git commit -m "refactor(ingest): centralize MCP target parsing in services"
```

If there are no remaining staged changes because earlier task commits already captured everything, skip this commit.

## Self-Review Checklist

- [ ] `src/mcp/server/handlers_embed_ingest.rs` contains no private ingest target parser or Reddit validator.
- [ ] `src/services/ingest/request.rs` remains free of `rmcp::ErrorData`, `invalid_params`, or MCP handler helpers.
- [ ] `src/services/action_api/commands/helpers.rs` still delegates to `source_from_mcp_request`.
- [ ] Parser behavior tests live in `src/services/ingest_tests.rs`.
- [ ] MCP tests only verify transport-specific behavior.
- [ ] No artifact, SSRF, REST routing, or envelope changes are included.
