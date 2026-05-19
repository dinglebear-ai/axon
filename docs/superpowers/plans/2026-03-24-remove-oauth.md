# Remove OAuth Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove all Google OAuth / MCP auth broker code from the codebase — auth is now handled externally by an OAuth gateway + SWAG reverse proxy.

**Architecture:** Delete the `oauth_google` module (~2,850 lines across 9 files), strip all OAuth routes and the `require_google_auth` middleware from `run_http_server`, and clean up docs/env references. The `/mcp` HTTP endpoint becomes unauthenticated internally (trusted behind the external gateway).

**Tech Stack:** Rust, Axum, axon MCP server (`crates/mcp/`)

---

## File Map

| Action | Path |
|--------|------|
| **Delete** | `crates/mcp/server/oauth_google.rs` |
| **Delete** | `crates/mcp/server/oauth_google/` (entire directory — 9 files) |
| **Delete** | `docs/auth/` (entire directory — 3 files: API-TOKEN.md, MCP-AUTH.md, README.md) |
| **Modify** | `crates/mcp/server.rs` — strip OAuth module, imports, routes, state, middleware layer |
| **Modify** | `docs/MCP.md` — remove OAuth sections / env vars |
| **Modify** | `crates/mcp/CLAUDE.md` — remove `oauth_google/` from module layout |
| **Modify** | `.env.example` — remove the OAuth comment on `AXON_MCP_API_KEY` |

---

## Task 1: Delete the `oauth_google` module files

**Files:**
- Delete: `crates/mcp/server/oauth_google.rs`
- Delete: `crates/mcp/server/oauth_google/config.rs`
- Delete: `crates/mcp/server/oauth_google/handlers_broker.rs`
- Delete: `crates/mcp/server/oauth_google/handlers_google.rs`
- Delete: `crates/mcp/server/oauth_google/handlers_protected.rs`
- Delete: `crates/mcp/server/oauth_google/helpers.rs`
- Delete: `crates/mcp/server/oauth_google/rate_limit.rs`
- Delete: `crates/mcp/server/oauth_google/state.rs`
- Delete: `crates/mcp/server/oauth_google/tests.rs`
- Delete: `crates/mcp/server/oauth_google/types.rs`

- [ ] **Step 1: Delete the entire oauth_google module**

```bash
rm -rf crates/mcp/server/oauth_google crates/mcp/server/oauth_google.rs
```

- [ ] **Step 2: Verify deletions**

```bash
ls crates/mcp/server/oauth_google* 2>&1 | grep "No such file"
```

Expected: `ls: cannot access 'crates/mcp/server/oauth_google*': No such file or directory`

---

## Task 2: Update `crates/mcp/server.rs`

**Files:**
- Modify: `crates/mcp/server.rs`

Remove the OAuth module declaration, all imports from `oauth_google`, and rewrite `run_http_server` to remove all OAuth routes, state, and the `require_google_auth` middleware.

- [ ] **Step 1: Verify current compile error** (expected after Task 1)

```bash
cargo check --bin axon 2>&1 | grep "oauth_google" | head -20
```

Expected: multiple `unresolved module` and `cannot find` errors referencing `oauth_google`.

- [ ] **Step 2: Remove the `mod oauth_google` declaration**

In `crates/mcp/server.rs`, delete lines 21–22:

```rust
#[path = "server/oauth_google.rs"]
mod oauth_google;
```

- [ ] **Step 3: Remove the `use oauth_google::...` import block**

Delete lines 42–47 (the full `use oauth_google::{ ... }` block):

```rust
use oauth_google::{
    GoogleOAuthState, oauth_authorization_server_metadata, oauth_authorization_server_metadata_mcp,
    oauth_authorize, oauth_google_callback, oauth_google_login, oauth_google_logout,
    oauth_google_status, oauth_google_token, oauth_protected_resource_metadata,
    oauth_register_client, oauth_token, require_google_auth,
};
```

- [ ] **Step 4: Rewrite `run_http_server`**

Replace the current `run_http_server` function (lines 245–311) with this simplified version:

```rust
pub async fn run_http_server(host: &str, port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let cors_cfg = Arc::new(load_mcp_config());

    let mcp_service: StreamableHttpService<AxonMcpServer, LocalSessionManager> =
        StreamableHttpService::new(
            || Ok(AxonMcpServer::new(load_mcp_config())),
            Default::default(),
            StreamableHttpServerConfig {
                stateful_mode: true,
                sse_keep_alive: None,
                ..Default::default()
            },
        );

    let app = Router::new()
        .nest_service("/mcp", mcp_service)
        .layer(middleware::from_fn_with_state(
            cors_cfg,
            mcp_http_cors_middleware,
        ));

    let listener = tokio::net::TcpListener::bind((host, port)).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
```

- [ ] **Step 5: Verify the file compiles**

```bash
cargo check --bin axon 2>&1 | grep -E "^error" | head -20
```

Expected: No errors referencing `oauth_google` or `GoogleOAuthState`.

- [ ] **Step 6: Run the full test suite**

```bash
cargo test 2>&1 | tail -10
```

Expected: All tests pass (or same pass count as before — no OAuth tests existed outside the deleted module).

- [ ] **Step 7: Commit**

```bash
git add crates/mcp/server.rs
git add -u crates/mcp/server/
git commit -m "feat(mcp): remove Google OAuth broker — auth now handled externally by SWAG gateway"
```

---

## Task 3: Delete auth docs directory

**Files:**
- Delete: `docs/auth/API-TOKEN.md`
- Delete: `docs/auth/MCP-AUTH.md`
- Delete: `docs/auth/README.md`

- [ ] **Step 1: Delete the auth docs directory**

```bash
rm -rf docs/auth
```

- [ ] **Step 2: Verify**

```bash
ls docs/auth 2>&1 | grep "No such file"
```

---

## Task 4: Update `docs/MCP.md`

**Files:**
- Modify: `docs/MCP.md`

Remove the OAuth env var section and the "OAuth Endpoints and Flow" section (and any OAuth bearer token examples). Keep the `AXON_MCP_API_KEY` mention only as a removed/deprecated note, or remove entirely since auth is external.

- [ ] **Step 1: Read current OAuth sections**

```bash
grep -n "oauth\|OAuth\|AXON_MCP_API_KEY\|bearer\|Bearer\|atk_" docs/MCP.md
```

- [ ] **Step 2: Remove OAuth sections from docs/MCP.md**

Delete all lines/sections in `docs/MCP.md` that reference:
- `AXON_MCP_API_KEY` env var (or update comment to note auth is external)
- `GOOGLE_OAUTH_*` env vars
- "OAuth bearer token" examples (`atk_...`)
- "OAuth Endpoints and Flow" section (lines ~167–200+)
- Any `.well-known/oauth-*` endpoints
- Any `Authorization: Bearer atk_...` examples

Replace the auth section with a simple note:

```markdown
## Authentication

Authentication is handled externally by the OAuth gateway and SWAG reverse proxy.
The `/mcp` endpoint is unauthenticated at the application level — all auth enforcement
happens at the ingress layer.
```

- [ ] **Step 3: Commit**

```bash
git add docs/MCP.md docs/auth
git commit -m "docs(mcp): remove OAuth auth docs — auth is now external via SWAG"
```

---

## Task 5: Update `crates/mcp/CLAUDE.md` and `.env.example`

**Files:**
- Modify: `crates/mcp/CLAUDE.md`
- Modify: `.env.example`

- [ ] **Step 1: Update `crates/mcp/CLAUDE.md`**

In the module layout section, remove the `oauth_google/` entry:

Delete:
```
└── oauth_google/               # Google OAuth2 integration
```

- [ ] **Step 2: Update `.env.example`**

Line 70 currently reads:
```
# If unset, MCP HTTP auth can still use Google OAuth bearer tokens.
```

Change it to:
```
# Static bearer key for /mcp (optional — auth is handled externally by the OAuth gateway).
```

Or remove the comment entirely if `AXON_MCP_API_KEY` is also being removed.

- [ ] **Step 3: Run full quality gate**

```bash
just verify
```

Expected: `fmt-check` + `clippy` + `check` + tests all pass.

- [ ] **Step 4: Final commit**

```bash
git add crates/mcp/CLAUDE.md .env.example
git commit -m "chore(mcp): remove OAuth references from CLAUDE.md and .env.example"
```

---

## Verification

After all tasks complete:

```bash
# No OAuth references remain in Rust source
grep -r "oauth_google\|GoogleOAuth\|require_google_auth\|oauth_authorize\|oauth_token\|atk_" crates/ --include="*.rs"

# Docs auth directory is gone
ls docs/auth 2>&1

# Binary still builds
cargo build --bin axon

# All tests pass
cargo test
```

Expected: no grep output, auth dir missing, clean build and test run.
