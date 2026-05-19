# ACP MCP Full Support Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement full support for all MCP capabilities exposed by the `agent-client-protocol` SDK — SSE transport, HTTP/SSE headers, `McpCapabilities` validation against adapter-advertised support, and MCP server preservation across all session fallback paths.

**Architecture:** Three gaps exist today — (1) `McpServer::Sse` is a valid SDK type but has no axon representation; (2) `InitializeResponse.agent_capabilities.mcp_capabilities` is never read, so axon blindly sends Http/Sse servers to adapters that only support stdio; (3) when a load-session falls back to new-session, MCP servers from the original request are silently dropped. This plan closes all three.

**Tech Stack:** Rust, `agent-client-protocol` SDK v0.10.2 (schema v0.10.8), tokio, serde_json, axon monolith policy (≤500 lines/file)

---

## File Map

| File | Change |
|------|--------|
| `crates/services/types/acp.rs` | Add `Sse` variant + `headers` field to `AcpMcpServerConfig` |
| `crates/services/acp/mapping.rs` | Add `Sse` case + headers to `convert_mcp_servers`; add `filter_compatible_mcp_servers` |
| `crates/services/acp/bridge.rs` | Add `mcp_http_supported: Cell<bool>` + `mcp_sse_supported: Cell<bool>` to `AcpRuntimeState` |
| `crates/services/acp/session.rs` | Read `McpCapabilities` after `initialize`; preserve MCP servers in one-shot load-session fallback |
| `crates/services/acp/persistent_conn/turn.rs` | Pass MCP servers to `LoadSessionRequest`, `NewSessionRequest` fallback, and `create_new_session` |
| `crates/web/execute/mcp_config.rs` | Parse `transport: "sse"` and `headers` from `mcp.json` disk format |
| `.monolith-allowlist` | Add `crates/services/types/acp.rs` (currently 547 L; grows ~20 L) |
| `ACP-GAP-ANALYSIS.md` | Add MCP section documenting pre/post state |

---

## Task 1: Extend `AcpMcpServerConfig` with SSE variant and headers

> **Context:** `crates/services/types/acp.rs` is the internal type that flows from the web layer through to the ACP mapping layer. It currently has `Stdio` and `Http` variants. The SDK's `McpServer::Sse` has the same shape as `Http` (name + URL + headers) but is a distinct transport. `Http` is also missing headers support today.

**Files:**
- Modify: `crates/services/types/acp.rs` (around line 86–108)
- Modify: `.monolith-allowlist`

- [ ] **Step 1.1: Add `types/acp.rs` to the monolith allowlist**

The file is currently 547 lines (over the 500-line limit) with no existing allowlist entry. This plan adds ~20 lines. Add an entry now so CI doesn't fail mid-implementation.

In `.monolith-allowlist`, append:
```
crates/services/types/acp.rs  # expires: 2026-04-30  -- 547L+; Sse variant + headers pending split
```

- [ ] **Step 1.2: Write failing tests for the new variants**

In `crates/services/types/acp.rs`, add a `#[cfg(test)]` block at the bottom (these lines don't count against the monolith limit):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_variant_name_returns_name() {
        let cfg = AcpMcpServerConfig::Sse {
            name: "my-sse".to_string(),
            url: "http://localhost:3000/sse".to_string(),
            headers: vec![],
        };
        assert_eq!(cfg.name(), "my-sse");
    }

    #[test]
    fn http_variant_with_headers_roundtrips_serde() {
        let cfg = AcpMcpServerConfig::Http {
            name: "my-http".to_string(),
            url: "http://localhost:3000/mcp".to_string(),
            headers: vec![("Authorization".to_string(), "Bearer tok".to_string())],
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let roundtrip: AcpMcpServerConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg, roundtrip);
    }

    #[test]
    fn sse_variant_roundtrips_serde() {
        let cfg = AcpMcpServerConfig::Sse {
            name: "my-sse".to_string(),
            url: "http://localhost:3000/sse".to_string(),
            headers: vec![("X-Api-Key".to_string(), "secret".to_string())],
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let roundtrip: AcpMcpServerConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg, roundtrip);
    }
}
```

- [ ] **Step 1.3: Run tests — verify they fail**

```bash
cd /home/jmagar/workspace/axon_rust
cargo test --lib 'types::tests' 2>&1 | grep -E "FAILED|error"
```

Expected: compile error — `Sse` variant doesn't exist yet.

- [ ] **Step 1.4: Add `Sse` variant and `headers` to `Http` in `AcpMcpServerConfig`**

In `crates/services/types/acp.rs`, replace the `AcpMcpServerConfig` enum and its `name()` impl:

```rust
/// MCP server configuration passed through to ACP NewSessionRequest.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "transport", rename_all = "snake_case")]
pub enum AcpMcpServerConfig {
    Stdio {
        name: String,
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: Vec<(String, String)>,
    },
    Http {
        name: String,
        url: String,
        /// HTTP headers to send with every request (name, value) pairs.
        #[serde(default)]
        headers: Vec<(String, String)>,
    },
    Sse {
        name: String,
        url: String,
        /// HTTP headers to send with every SSE request (name, value) pairs.
        #[serde(default)]
        headers: Vec<(String, String)>,
    },
}

impl AcpMcpServerConfig {
    /// Returns the server name regardless of transport variant.
    pub fn name(&self) -> &str {
        match self {
            Self::Stdio { name, .. } | Self::Http { name, .. } | Self::Sse { name, .. } => name,
        }
    }
}
```

- [ ] **Step 1.5: Run tests — verify they pass**

```bash
cargo test --lib 'types::tests' 2>&1 | grep -E "test.*ok|FAILED|error"
```

Expected: 3 tests pass (`sse_variant_name_returns_name`, `http_variant_with_headers_roundtrips_serde`, `sse_variant_roundtrips_serde`).

- [ ] **Step 1.6: Fix any compile errors in callers**

```bash
cargo check 2>&1 | grep "error\[" | head -20
```

The `name()` pattern match and any exhaustive matches on `AcpMcpServerConfig` will need the new `Sse` arm. Fix all compile errors. Common locations: `crates/web/execute/mcp_config.rs` (if it pattern-matches the enum), `crates/services/acp/mapping.rs`.

- [ ] **Step 1.7: Commit**

```bash
git add crates/services/types/acp.rs .monolith-allowlist
git commit -m "feat(acp): add Sse variant and headers to AcpMcpServerConfig"
```

---

## Task 2: Update `convert_mcp_servers` and add `filter_compatible_mcp_servers`

> **Context:** `mapping.rs` has `convert_mcp_servers` (lines 353–381) which converts `AcpMcpServerConfig` → SDK `McpServer`. It currently handles `Stdio` and `Http` but ignores the Sse variant (compile error after Task 1). It also doesn't pass `headers` for Http. This task fixes both and adds a new `filter_compatible_mcp_servers` function that drops servers with unsupported transports and warns.

**Files:**
- Modify: `crates/services/acp/mapping.rs` (currently 406 lines; grows ~50 lines)

- [ ] **Step 2.1: Write failing tests for `filter_compatible_mcp_servers`**

Add to the end of `crates/services/acp/mapping.rs` inside a `#[cfg(test)]` block:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates::services::types::AcpMcpServerConfig;

    #[test]
    fn filter_keeps_stdio_always() {
        let servers = vec![AcpMcpServerConfig::Stdio {
            name: "s".into(), command: "/bin/srv".into(), args: vec![], env: vec![],
        }];
        let filtered = filter_compatible_mcp_servers(&servers, false, false);
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn filter_drops_http_when_not_supported() {
        let servers = vec![AcpMcpServerConfig::Http {
            name: "h".into(), url: "http://localhost/mcp".into(), headers: vec![],
        }];
        let filtered = filter_compatible_mcp_servers(&servers, false, false);
        assert!(filtered.is_empty());
    }

    #[test]
    fn filter_keeps_http_when_supported() {
        let servers = vec![AcpMcpServerConfig::Http {
            name: "h".into(), url: "http://localhost/mcp".into(), headers: vec![],
        }];
        let filtered = filter_compatible_mcp_servers(&servers, true, false);
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn filter_drops_sse_when_not_supported() {
        let servers = vec![AcpMcpServerConfig::Sse {
            name: "s".into(), url: "http://localhost/sse".into(), headers: vec![],
        }];
        let filtered = filter_compatible_mcp_servers(&servers, false, false);
        assert!(filtered.is_empty());
    }

    #[test]
    fn filter_keeps_sse_when_supported() {
        let servers = vec![AcpMcpServerConfig::Sse {
            name: "s".into(), url: "http://localhost/sse".into(), headers: vec![],
        }];
        let filtered = filter_compatible_mcp_servers(&servers, false, true);
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn convert_http_with_headers() {
        let servers = vec![AcpMcpServerConfig::Http {
            name: "h".into(),
            url: "http://localhost/mcp".into(),
            headers: vec![("Authorization".to_string(), "Bearer tok".to_string())],
        }];
        let sdk = convert_mcp_servers(&servers);
        assert_eq!(sdk.len(), 1);
        match &sdk[0] {
            agent_client_protocol::McpServer::Http(h) => {
                assert_eq!(h.headers.len(), 1);
                assert_eq!(h.headers[0].name, "Authorization");
            }
            _ => panic!("expected Http"),
        }
    }

    #[test]
    fn convert_sse_maps_correctly() {
        let servers = vec![AcpMcpServerConfig::Sse {
            name: "s".into(),
            url: "http://localhost/sse".into(),
            headers: vec![],
        }];
        let sdk = convert_mcp_servers(&servers);
        assert_eq!(sdk.len(), 1);
        assert!(matches!(sdk[0], agent_client_protocol::McpServer::Sse(_)));
    }
}
```

- [ ] **Step 2.2: Run — verify they fail**

```bash
cargo test --lib 'acp::mapping::tests' 2>&1 | grep -E "FAILED|error"
```

Expected: compile error — `filter_compatible_mcp_servers` doesn't exist; `convert_mcp_servers` missing Sse arm.

- [ ] **Step 2.3: Implement Sse arm + headers in `convert_mcp_servers`**

In `crates/services/acp/mapping.rs`, update `convert_mcp_servers`:

```rust
pub(super) fn convert_mcp_servers(configs: &[AcpMcpServerConfig]) -> Vec<McpServer> {
    configs
        .iter()
        .map(|cfg| match cfg {
            AcpMcpServerConfig::Stdio { name, command, args, env } => {
                let mut server = McpServerStdio::new(name.clone(), command.clone());
                if !args.is_empty() {
                    server = server.args(args.clone());
                }
                if !env.is_empty() {
                    server = server.env(
                        env.iter()
                            .map(|(k, v)| EnvVariable::new(k.clone(), v.clone()))
                            .collect(),
                    );
                }
                McpServer::Stdio(server)
            }
            AcpMcpServerConfig::Http { name, url, headers } => {
                let mut server = McpServerHttp::new(name.clone(), url.clone());
                if !headers.is_empty() {
                    server = server.headers(
                        headers.iter()
                            .map(|(n, v)| HttpHeader::new(n.clone(), v.clone()))
                            .collect(),
                    );
                }
                McpServer::Http(server)
            }
            AcpMcpServerConfig::Sse { name, url, headers } => {
                let mut server = McpServerSse::new(name.clone(), url.clone());
                if !headers.is_empty() {
                    server = server.headers(
                        headers.iter()
                            .map(|(n, v)| HttpHeader::new(n.clone(), v.clone()))
                            .collect(),
                    );
                }
                McpServer::Sse(server)
            }
        })
        .collect()
}
```

Also update the import line in `mapping.rs` to add `McpServerSse, HttpHeader`:

```rust
use agent_client_protocol::{
    ContentBlock, EnvVariable, HttpHeader, LoadSessionRequest, McpServer, McpServerHttp,
    McpServerSse, McpServerStdio, NewSessionRequest, SessionConfigKind,
    SessionConfigOption as SdkConfigOption, SessionUpdate,
};
```

- [ ] **Step 2.4: Add `filter_compatible_mcp_servers`**

Add this function to `mapping.rs` (after `convert_mcp_servers`, before `build_session_setup`):

```rust
/// Filter MCP servers to only those whose transport the adapter supports.
///
/// Stdio is always supported (no capability flag). Http requires
/// `mcp_capabilities.http = true`. Sse requires `mcp_capabilities.sse = true`.
///
/// Unsupported servers are logged at WARN and dropped so session setup
/// doesn't fail with an opaque adapter error.
pub(super) fn filter_compatible_mcp_servers(
    configs: &[AcpMcpServerConfig],
    http_supported: bool,
    sse_supported: bool,
) -> Vec<AcpMcpServerConfig> {
    configs
        .iter()
        .filter(|cfg| match cfg {
            AcpMcpServerConfig::Stdio { .. } => true,
            AcpMcpServerConfig::Http { name, .. } => {
                if !http_supported {
                    crate::crates::core::logging::log_warn(&format!(
                        "ACP: dropping MCP server '{name}' — adapter does not advertise http transport support"
                    ));
                }
                http_supported
            }
            AcpMcpServerConfig::Sse { name, .. } => {
                if !sse_supported {
                    crate::crates::core::logging::log_warn(&format!(
                        "ACP: dropping MCP server '{name}' — adapter does not advertise sse transport support"
                    ));
                }
                sse_supported
            }
        })
        .cloned()
        .collect()
}
```

Also export it from `mapping.rs` at the pub use site (or make it `pub(super)` — callers are in the same `acp` module tree so `pub(super)` is fine).

- [ ] **Step 2.5: Run tests — verify they pass**

```bash
cargo test --lib 'acp::mapping::tests' 2>&1 | grep -E "test.*ok|FAILED|error"
```

Expected: all 7 mapping tests pass.

- [ ] **Step 2.6: Commit**

```bash
git add crates/services/acp/mapping.rs
git commit -m "feat(acp): add Sse + headers to convert_mcp_servers; add filter_compatible_mcp_servers"
```

---

## Task 3: Store `McpCapabilities` in `AcpRuntimeState` and validate on session setup

> **Context:** After `conn.initialize(initialize).await` in `session.rs:226`, the response contains `resp.agent_capabilities.mcp_capabilities: McpCapabilities { http: bool, sse: bool }`. This information is currently discarded. `AcpRuntimeState` (in `bridge.rs`) needs two new `Cell<bool>` fields to hold these flags. Then `establish_acp_session` (in `session.rs`) calls `filter_compatible_mcp_servers` before building session setup.
>
> **Monolith warning:** `bridge.rs` is 504 lines (4 over limit). Adding 2 lines is fine. `session.rs` is 488 lines — adding a filter call adds ~10 lines, keeping it under 500.

**Files:**
- Modify: `crates/services/acp/bridge.rs` (around lines 33–65)
- Modify: `crates/services/acp/session.rs` (around lines 226–240)
- Modify: `crates/services/acp/runtime.rs` (around line 100)

- [ ] **Step 3.1: Write failing tests for capability storage**

In `crates/services/acp/bridge.rs`, in the existing `#[cfg(test)]` block, add:

```rust
#[test]
fn runtime_state_default_mcp_capabilities_are_false() {
    let state = AcpRuntimeState::default();
    assert!(!state.mcp_http_supported.get());
    assert!(!state.mcp_sse_supported.get());
}

#[test]
fn runtime_state_mcp_capabilities_can_be_set() {
    let state = AcpRuntimeState::default();
    state.mcp_http_supported.set(true);
    state.mcp_sse_supported.set(true);
    assert!(state.mcp_http_supported.get());
    assert!(state.mcp_sse_supported.get());
}
```

- [ ] **Step 3.2: Run — verify they fail**

```bash
cargo test --lib 'acp::bridge::tests' 2>&1 | grep -E "FAILED|error"
```

Expected: compile error — fields don't exist.

- [ ] **Step 3.3: Add capability fields to `AcpRuntimeState`**

In `crates/services/acp/bridge.rs`, after the `current_mode` field (around line 60), add:

```rust
    /// Whether the adapter advertises HTTP MCP transport support.
    /// Set from `InitializeResponse.agent_capabilities.mcp_capabilities.http`.
    pub(super) mcp_http_supported: std::cell::Cell<bool>,
    /// Whether the adapter advertises SSE MCP transport support.
    /// Set from `InitializeResponse.agent_capabilities.mcp_capabilities.sse`.
    pub(super) mcp_sse_supported: std::cell::Cell<bool>,
```

The `Default` derive will set both to `false` automatically.

- [ ] **Step 3.4: Run tests — verify they pass**

```bash
cargo test --lib 'acp::bridge::tests' 2>&1 | grep -E "test.*ok|FAILED"
```

Expected: new tests pass. All other bridge tests still pass.

- [ ] **Step 3.5: Read capabilities in `initialize_connection` and call `filter_compatible_mcp_servers` in `establish_acp_session`**

In `crates/services/acp/session.rs`, after the `conn.initialize(initialize).await` call (around line 226), add:

```rust
    // Store adapter's MCP transport capabilities so session setup can filter
    // servers to only those the adapter supports.
    runtime_state
        .mcp_http_supported
        .set(resp.agent_capabilities.mcp_capabilities.http);
    runtime_state
        .mcp_sse_supported
        .set(resp.agent_capabilities.mcp_capabilities.sse);
```

Then in `crates/services/acp/runtime.rs`, in `establish_acp_session`, after `initialize_connection` returns and before `setup_session`, filter the MCP servers in the session setup request:

```rust
    // Filter MCP servers to transports the adapter actually supports.
    // This mutates session_setup before it is consumed by setup_session.
    let session_setup = filter_session_setup_mcp_servers(session_setup, &runtime_state);
```

Add this helper at the bottom of `runtime.rs` (before the test module):

```rust
/// Drop MCP servers from a session setup request that the adapter doesn't support.
fn filter_session_setup_mcp_servers(
    setup: AcpSessionSetupRequest,
    runtime_state: &Arc<super::bridge::AcpRuntimeState>,
) -> AcpSessionSetupRequest {
    let http = runtime_state.mcp_http_supported.get();
    let sse = runtime_state.mcp_sse_supported.get();
    match setup {
        AcpSessionSetupRequest::New(mut req) => {
            let filtered = super::mapping::filter_compatible_mcp_servers(
                &req.mcp_servers
                    .iter()
                    .filter_map(|s| sdk_server_to_config(s))
                    .collect::<Vec<_>>(),
                http,
                sse,
            );
            req.mcp_servers = super::mapping::convert_mcp_servers(&filtered);
            AcpSessionSetupRequest::New(req)
        }
        AcpSessionSetupRequest::Load(mut req) => {
            let filtered = super::mapping::filter_compatible_mcp_servers(
                &req.mcp_servers
                    .iter()
                    .filter_map(|s| sdk_server_to_config(s))
                    .collect::<Vec<_>>(),
                http,
                sse,
            );
            req.mcp_servers = super::mapping::convert_mcp_servers(&filtered);
            AcpSessionSetupRequest::Load(req)
        }
    }
}
```

**Note:** `filter_compatible_mcp_servers` takes `&[AcpMcpServerConfig]` (axon type) not `&[McpServer]` (SDK type). Since `session_setup` already holds SDK `McpServer` objects (they were converted in `build_session_setup`), we need a reverse mapping. The simplest approach is to NOT go through the SDK type at all — instead, store the original `Vec<AcpMcpServerConfig>` alongside the SDK request, or filter BEFORE calling `convert_mcp_servers`.

**Better approach:** Move the filtering into `build_session_setup` in `mapping.rs`, where we still have the `AcpMcpServerConfig` slice. Add `http_supported: bool, sse_supported: bool` parameters:

```rust
pub(super) fn build_session_setup(
    session_id: Option<&str>,
    cwd: impl AsRef<Path>,
    mcp_servers: &[AcpMcpServerConfig],
    http_supported: bool,
    sse_supported: bool,
) -> Result<AcpSessionSetupRequest, Box<dyn Error>> {
    let cwd = validate_session_cwd(cwd.as_ref())?;
    let compatible = filter_compatible_mcp_servers(mcp_servers, http_supported, sse_supported);
    let sdk_mcp_servers = convert_mcp_servers(&compatible);
    // ... rest unchanged
}
```

Update all callers of `build_session_setup` to pass the two bool parameters. Initially pass `true, true` (permissive) until capabilities are known. After `establish_acp_session` has the capabilities, callers go through a second filter call or pass the capabilities from `AcpRuntimeState`. Since `build_session_setup` is called BEFORE `initialize_connection` (capabilities aren't yet known), the filtering should happen AFTER initialize. So the cleanest architecture is: capabilities are unknown at build time, filter is applied after initialize.

**Simplest correct solution:** Keep `build_session_setup` unchanged. After `initialize_connection`, if capabilities indicate http=false or sse=false, rebuild the session_setup with filtered MCP servers using a helper that takes `&AcpSessionSetupRequest` + capabilities and returns a new one. Since the SDK's `McpServer` type is `Clone`, we can reconstruct `AcpMcpServerConfig` from it — or, simpler, just pre-store the original `Vec<AcpMcpServerConfig>` in the `AcpSessionSetupRequest` types.

**Pragmatic decision for this plan:** Add `original_mcp_configs: Vec<AcpMcpServerConfig>` to `AcpSessionSetupRequest::New` and `::Load` — this lets the post-initialize filter step re-filter from the original axon types. If this would violate the monolith limit on types/acp.rs, instead filter at build time (pass `true, true`) and add a post-initialize filtering call that iterates the SDK type back — the SDK types are inspectable via pattern matching.

**Chosen approach (minimal code change):** Add a `filter_sdk_mcp_servers(servers: &[McpServer], http: bool, sse: bool) -> Vec<McpServer>` helper in `mapping.rs` that filters the already-converted SDK types. This avoids modifying `AcpSessionSetupRequest` at all:

```rust
pub(super) fn filter_sdk_mcp_servers(
    servers: &[McpServer],
    http_supported: bool,
    sse_supported: bool,
) -> Vec<McpServer> {
    servers.iter().filter(|s| match s {
        McpServer::Stdio(_) => true,
        McpServer::Http(h) => {
            if !http_supported {
                crate::crates::core::logging::log_warn(&format!(
                    "ACP: dropping HTTP MCP server '{}' — adapter lacks http capability",
                    h.name
                ));
            }
            http_supported
        }
        McpServer::Sse(s) => {
            if !sse_supported {
                crate::crates::core::logging::log_warn(&format!(
                    "ACP: dropping SSE MCP server '{}' — adapter lacks sse capability",
                    s.name
                ));
            }
            sse_supported
        }
    }).cloned().collect()
}
```

Then in `establish_acp_session`, after `initialize_connection`:

```rust
    // Apply capability filter to MCP servers in session setup.
    let http = runtime_state.mcp_http_supported.get();
    let sse = runtime_state.mcp_sse_supported.get();
    let session_setup = apply_mcp_capability_filter(session_setup, http, sse);
```

Where `apply_mcp_capability_filter` is a small helper in `runtime.rs`:

```rust
fn apply_mcp_capability_filter(
    setup: AcpSessionSetupRequest,
    http: bool,
    sse: bool,
) -> AcpSessionSetupRequest {
    use super::mapping::filter_sdk_mcp_servers;
    match setup {
        AcpSessionSetupRequest::New(mut req) => {
            req.mcp_servers = filter_sdk_mcp_servers(&req.mcp_servers, http, sse);
            AcpSessionSetupRequest::New(req)
        }
        AcpSessionSetupRequest::Load(mut req) => {
            req.mcp_servers = filter_sdk_mcp_servers(&req.mcp_servers, http, sse);
            AcpSessionSetupRequest::Load(req)
        }
    }
}
```

- [ ] **Step 3.6: Add tests for `filter_sdk_mcp_servers`**

In `mapping.rs` test block, add:

```rust
#[test]
fn filter_sdk_drops_http_when_not_supported() {
    use agent_client_protocol::{McpServer, McpServerHttp};
    let servers = vec![McpServer::Http(McpServerHttp::new("h", "http://localhost/mcp"))];
    let filtered = filter_sdk_mcp_servers(&servers, false, false);
    assert!(filtered.is_empty());
}

#[test]
fn filter_sdk_keeps_stdio_always() {
    use agent_client_protocol::{McpServer, McpServerStdio};
    let servers = vec![McpServer::Stdio(McpServerStdio::new("s", "/bin/srv"))];
    let filtered = filter_sdk_mcp_servers(&servers, false, false);
    assert_eq!(filtered.len(), 1);
}
```

- [ ] **Step 3.7: Run all tests — verify they pass**

```bash
cargo test --lib 2>&1 | tail -5
```

Expected: all tests pass.

- [ ] **Step 3.8: Commit**

```bash
git add crates/services/acp/bridge.rs crates/services/acp/session.rs \
        crates/services/acp/runtime.rs crates/services/acp/mapping.rs
git commit -m "feat(acp): read McpCapabilities from InitializeResponse; filter unsupported MCP transports"
```

---

## Task 4: Preserve MCP servers on load-session fallback (one-shot path)

> **Context:** In `crates/services/acp/session.rs:setup_session`, when `load_session` fails, the fallback creates a new session via `NewSessionRequest::new(fallback_cwd)` with no MCP servers. This silently drops any MCP servers the caller requested. The fix: clone the SDK `mcp_servers` from the `LoadSessionRequest` before it is consumed, and pass them to the fallback `NewSessionRequest`.

**Files:**
- Modify: `crates/services/acp/session.rs` (around lines 297–342)

- [ ] **Step 4.1: Write failing test**

Add to the test module in `session.rs` (create `#[cfg(test)]` block if absent):

```rust
#[cfg(test)]
mod tests {
    // Note: full session integration tests require live adapter processes.
    // This test validates that build_session_setup preserves MCP servers.
    use super::*;
    use crate::crates::services::acp::mapping::build_session_setup;
    use crate::crates::services::types::AcpMcpServerConfig;

    #[test]
    fn build_session_setup_load_preserves_mcp_servers() {
        let cwd = std::env::temp_dir();
        let servers = vec![AcpMcpServerConfig::Stdio {
            name: "test-srv".into(),
            command: "/bin/echo".into(),
            args: vec![],
            env: vec![],
        }];
        let setup = build_session_setup(Some("existing-session"), &cwd, &servers)
            .expect("build_session_setup failed");
        match setup {
            crate::crates::services::acp::AcpSessionSetupRequest::Load(req) => {
                assert_eq!(req.mcp_servers.len(), 1);
            }
            _ => panic!("expected Load variant"),
        }
    }
}
```

- [ ] **Step 4.2: Run — verify passes (this test already passes via `build_session_setup`)**

```bash
cargo test --lib 'acp::session::tests' 2>&1 | grep -E "test.*ok|FAILED"
```

This should already pass since `build_session_setup` already passes MCP servers to `LoadSessionRequest`. The real fix is in the runtime fallback.

- [ ] **Step 4.3: Write integration-style test for the fallback path**

Since `setup_session` requires a live `ClientSideConnection`, we can't unit test it directly. Instead, read `session.rs:setup_session` and verify the fallback path manually. The bug is on line ~325:

```rust
// CURRENT (buggy):
let r = conn
    .new_session(NewSessionRequest::new(fallback_cwd))
    .await
    ...
```

Document the fix as an in-code assertion check — add a `// INVARIANT` comment after the fix confirming intent.

- [ ] **Step 4.4: Fix the fallback path in `setup_session`**

In `crates/services/acp/session.rs`, in the `AcpSessionSetupRequest::Load` match arm, before calling `conn.load_session(load_session)`, extract the MCP servers:

```rust
AcpSessionSetupRequest::Load(load_session) => {
    validate_cwd_usable(&load_session.cwd)?;
    // ...log...
    let requested_id = load_session.session_id.clone();
    let fallback_cwd = load_session.cwd.clone();
    let fallback_mcp_servers = load_session.mcp_servers.clone(); // ← NEW
    match conn.load_session(load_session).await {
        Ok(r) => Ok((requested_id, r.config_options)),
        Err(err) => {
            // ...warn log...
            let mut fallback_req = NewSessionRequest::new(fallback_cwd);
            if !fallback_mcp_servers.is_empty() {
                fallback_req = fallback_req.mcp_servers(fallback_mcp_servers); // ← NEW
            }
            let r = conn
                .new_session(fallback_req)
                .await
                .map_err(|e| e.to_string())?;
            // INVARIANT: fallback session has same MCP servers as the failed load request
            // ...emit SessionFallback event...
            Ok((r.session_id, r.config_options))
        }
    }
}
```

- [ ] **Step 4.5: Verify compile and tests pass**

```bash
cargo check && cargo test --lib 2>&1 | tail -5
```

Expected: compiles cleanly, all tests pass.

- [ ] **Step 4.6: Commit**

```bash
git add crates/services/acp/session.rs
git commit -m "fix(acp): preserve MCP servers on load-session fallback in one-shot path"
```

---

## Task 5: Pass MCP servers through persistent-connection session management

> **Context:** In `persistent_conn/turn.rs`, three call sites don't pass MCP servers:
> 1. `load_or_fallback_session` calls `LoadSessionRequest::new(...)` with no MCP servers
> 2. `load_or_fallback_session`'s fallback calls `create_new_session` which also ignores MCP servers
> 3. `create_new_session` itself calls `NewSessionRequest::new(...)` with no MCP servers
>
> The MCP servers are available on `turn_ctx.req.mcp_servers`. They need to be converted via `convert_mcp_servers` and threaded through.

**Files:**
- Modify: `crates/services/acp/persistent_conn/turn.rs` (currently 347 lines; grows ~30 lines)
- Modify: `crates/services/acp/mapping.rs` (re-export `convert_mcp_servers` as `pub(super)` — already is)

- [ ] **Step 5.1: Write failing test for `create_new_session` MCP passthrough**

In `turn.rs` test module (create if absent at bottom of file):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates::services::types::AcpMcpServerConfig;

    // Note: `create_new_session` requires a live ClientSideConnection so it
    // can't be unit-tested here. Instead, test that `sdk_mcp_servers_from_turn`
    // correctly converts the turn's MCP server list.

    #[test]
    fn sdk_mcp_servers_from_empty_list_is_empty() {
        let configs: Vec<AcpMcpServerConfig> = vec![];
        let sdk = super::super::mapping::convert_mcp_servers(&configs);
        assert!(sdk.is_empty());
    }

    #[test]
    fn sdk_mcp_servers_from_stdio_has_one_entry() {
        let configs = vec![AcpMcpServerConfig::Stdio {
            name: "s".into(), command: "/bin/echo".into(), args: vec![], env: vec![],
        }];
        let sdk = super::super::mapping::convert_mcp_servers(&configs);
        assert_eq!(sdk.len(), 1);
    }
}
```

- [ ] **Step 5.2: Run — verify they pass (conversion already works)**

```bash
cargo test --lib 'acp::persistent_conn::turn::tests' 2>&1 | grep -E "test.*ok|FAILED"
```

Expected: both tests pass.

- [ ] **Step 5.3: Update `create_new_session` to accept and pass MCP servers**

In `persistent_conn/turn.rs`, change `create_new_session` signature and body:

```rust
async fn create_new_session(
    conn: &mut ClientSideConnection,
    session_cwd: &Path,
    runtime_state: &Arc<AcpRuntimeState>,
    service_tx: &Option<mpsc::Sender<ServiceEvent>>,
    mcp_servers: Vec<agent_client_protocol::McpServer>,  // ← NEW
) -> Result<SessionId, String> {
    let mut req = NewSessionRequest::new(session_cwd.to_path_buf());
    if !mcp_servers.is_empty() {
        req = req.mcp_servers(mcp_servers);
    }
    let response = conn
        .new_session(req)
        .await
        .map_err(|err| err.to_string())?;
    // ...rest unchanged...
}
```

- [ ] **Step 5.4: Update `load_or_fallback_session` to pass MCP servers**

Update signature and body:

```rust
async fn load_or_fallback_session(
    conn: &mut ClientSideConnection,
    session_cwd: &Path,
    runtime_state: &Arc<AcpRuntimeState>,
    service_tx: &Option<mpsc::Sender<ServiceEvent>>,
    requested_id: &str,
    mcp_servers: Vec<agent_client_protocol::McpServer>,  // ← NEW
) -> Result<SessionId, String> {
    let mut load_req = LoadSessionRequest::new(SessionId::new(requested_id), session_cwd.to_path_buf());
    if !mcp_servers.is_empty() {
        load_req = load_req.mcp_servers(mcp_servers.clone());
    }
    let load_result = conn.load_session(load_req).await;

    match load_result {
        Ok(response) => { /* unchanged */ }
        Err(err) => {
            // ...warn log...
            let fallback = create_new_session(conn, session_cwd, runtime_state, service_tx, mcp_servers)  // ← pass through
                .await
                ...
        }
    }
}
```

- [ ] **Step 5.5: Update `ensure_turn_session` to convert and pass MCP servers**

In `ensure_turn_session`, convert the turn's MCP server configs and pass them down:

```rust
async fn ensure_turn_session(
    conn: &mut ClientSideConnection,
    session_cwd: &Path,
    runtime_state: &Arc<AcpRuntimeState>,
    turn_ctx: &mut TurnContext,
) -> Result<(), String> {
    let sdk_servers = super::super::mapping::convert_mcp_servers(&turn_ctx.req.mcp_servers);
    let requested = turn_ctx
        .req
        .session_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    match requested {
        Some(requested_id) => load_or_fallback_session(
            conn, session_cwd, runtime_state, &turn_ctx.service_tx,
            requested_id, sdk_servers,       // ← pass servers
        )
        .await
        .map(|id| turn_ctx.turn_session_id = id),
        None => create_new_session(conn, session_cwd, runtime_state, &turn_ctx.service_tx, sdk_servers) // ← pass servers
            .await
            .map(|id| turn_ctx.turn_session_id = id)
            .map_err(|err| format!("ACP failed to create new session: {err}")),
    }
}
```

Add the mapping import at the top of `turn.rs` (it's already in the same crate so use the module path):

```rust
use super::super::mapping::convert_mcp_servers;
```

Or call it inline: `super::super::mapping::convert_mcp_servers(...)`.

- [ ] **Step 5.6: Fix compile errors and run tests**

```bash
cargo check && cargo test --lib 2>&1 | tail -5
```

Expected: compiles cleanly, all tests pass.

- [ ] **Step 5.7: Commit**

```bash
git add crates/services/acp/persistent_conn/turn.rs
git commit -m "fix(acp): pass MCP servers through load_session and create_new_session in persistent mode"
```

---

## Task 6: Update `mcp_config.rs` disk loader for SSE and headers

> **Context:** `crates/web/execute/mcp_config.rs` reads `$AXON_DATA_DIR/axon/mcp.json`. Currently it infers Stdio vs Http based on whether the entry has a `command` or `url`. There's no way to specify SSE (URL-based but SSE transport) or HTTP/SSE headers. This task adds `transport` and `headers` fields to the `McpServerEntry` internal type used during deserialization.
>
> **New `mcp.json` format** (backward-compatible — `transport` defaults to `"http"` for URL entries):
>
> ```json
> {
>   "mcpServers": {
>     "my-sse-server": {
>       "url": "http://localhost:3000/mcp/sse",
>       "transport": "sse"
>     },
>     "my-http-server": {
>       "url": "http://localhost:3000/mcp",
>       "headers": [{"name": "Authorization", "value": "Bearer token"}]
>     },
>     "my-stdio-server": {
>       "command": "/usr/local/bin/my-mcp",
>       "args": ["--port", "3000"],
>       "env": { "API_KEY": "secret" }
>     }
>   }
> }
> ```

**Files:**
- Modify: `crates/web/execute/mcp_config.rs` (currently 191 lines)

- [ ] **Step 6.1: Write failing tests**

At the bottom of `mcp_config.rs`, add:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn parses_sse_entry_from_json() {
        let json = r#"{"mcpServers": {"my-sse": {"url": "http://localhost/sse", "transport": "sse"}}}"#;
        let path = tempfile::NamedTempFile::new().unwrap();
        tokio::fs::write(path.path(), json).await.unwrap();
        let servers = fetch_axon_mcp_servers_from_disk(path.path()).await;
        assert_eq!(servers.len(), 1);
        assert!(matches!(servers[0], AcpMcpServerConfig::Sse { .. }));
    }

    #[tokio::test]
    async fn parses_http_entry_with_headers() {
        let json = r#"{"mcpServers": {"my-http": {"url": "http://localhost/mcp", "headers": [{"name": "Authorization", "value": "Bearer tok"}]}}}"#;
        let path = tempfile::NamedTempFile::new().unwrap();
        tokio::fs::write(path.path(), json).await.unwrap();
        let servers = fetch_axon_mcp_servers_from_disk(path.path()).await;
        assert_eq!(servers.len(), 1);
        match &servers[0] {
            AcpMcpServerConfig::Http { headers, .. } => {
                assert_eq!(headers.len(), 1);
                assert_eq!(headers[0].0, "Authorization");
            }
            _ => panic!("expected Http"),
        }
    }

    #[tokio::test]
    async fn http_url_without_transport_defaults_to_http() {
        let json = r#"{"mcpServers": {"my-http": {"url": "http://localhost/mcp"}}}"#;
        let path = tempfile::NamedTempFile::new().unwrap();
        tokio::fs::write(path.path(), json).await.unwrap();
        let servers = fetch_axon_mcp_servers_from_disk(path.path()).await;
        assert_eq!(servers.len(), 1);
        assert!(matches!(servers[0], AcpMcpServerConfig::Http { .. }));
    }

    #[test]
    fn is_safe_mcp_command_rejects_shell() {
        assert!(!is_safe_mcp_command("bash"));
        assert!(!is_safe_mcp_command("sh"));
    }

    #[test]
    fn is_safe_mcp_command_accepts_absolute_path() {
        assert!(is_safe_mcp_command("/usr/local/bin/mcp-server"));
    }
}
```

Note: `tempfile` is already in `[dev-dependencies]` in `Cargo.toml`.

- [ ] **Step 6.2: Run — verify SSE-related tests fail**

```bash
cargo test --lib 'web::execute::mcp_config::tests' 2>&1 | grep -E "FAILED|ok"
```

Expected: `parses_sse_entry_from_json` and `parses_http_entry_with_headers` FAIL; the `is_safe_mcp_command` tests may pass already.

- [ ] **Step 6.3: Update `McpServerEntry` to support `transport` and `headers`**

In `fetch_axon_mcp_servers_from_disk`, update the internal `McpServerEntry` struct:

```rust
#[derive(serde::Deserialize)]
struct HeaderEntry {
    name: String,
    value: String,
}

#[derive(serde::Deserialize)]
struct McpServerEntry {
    command: Option<String>,
    args: Option<Vec<String>>,
    env: Option<std::collections::HashMap<String, String>>,
    url: Option<String>,
    /// "http" (default for URL entries) or "sse"
    transport: Option<String>,
    /// HTTP headers for http/sse transports
    #[serde(default)]
    headers: Vec<HeaderEntry>,
}
```

Then in the `filter_map` closure, update the URL-based branch:

```rust
if let Some(url) = url {
    let headers: Vec<(String, String)> = entry
        .headers
        .into_iter()
        .map(|h| (h.name, h.value))
        .collect();
    match entry.transport.as_deref() {
        Some("sse") => Some(AcpMcpServerConfig::Sse { name, url, headers }),
        _ => Some(AcpMcpServerConfig::Http { name, url, headers }),
    }
} else {
    // Stdio branch — unchanged
}
```

- [ ] **Step 6.4: Run tests — verify they all pass**

```bash
cargo test --lib 'web::execute::mcp_config::tests' 2>&1 | grep -E "test.*ok|FAILED"
```

Expected: all 5 tests pass.

- [ ] **Step 6.5: Run full test suite**

```bash
cargo test --lib 2>&1 | tail -5
```

Expected: all tests pass, 0 failures.

- [ ] **Step 6.6: Lint and format**

```bash
cargo clippy --lib -- -D warnings 2>&1 | grep "^error" | head -10
cargo fmt --check 2>&1 | head -5
```

Fix any warnings, then:

```bash
cargo fmt
```

- [ ] **Step 6.7: Commit**

```bash
git add crates/web/execute/mcp_config.rs
git commit -m "feat(mcp): support SSE transport and HTTP headers in mcp.json disk loader"
```

---

## Task 7: Update `ACP-GAP-ANALYSIS.md`

> **Context:** The gap analysis has no MCP section. Add one documenting the pre/post state.

**Files:**
- Modify: `ACP-GAP-ANALYSIS.md`

- [ ] **Step 7.1: Add MCP section to the gap analysis**

Add a new gap entry in the "What axon Currently Implements" table and a new detailed section.

In the **Table of Contents**, add:
```markdown
   - [MCP Server Management](#14-mcp-server-management)
```

In the **"What axon Currently Implements → Agent Calls"** table, add a row:

```markdown
| `new_session(NewSessionRequest{mcp_servers})` | ✅ **Implemented** | Stdio + Http + Sse (post this plan); capability-filtered against `McpCapabilities` from `InitializeResponse` |
| `load_session(LoadSessionRequest{mcp_servers})` | ✅ **Implemented** | MCP servers passed through; preserved on fallback to `new_session` |
```

Add a new **Detailed Gap Analysis** section:

```markdown
### 14. MCP Server Management

> **Status: ✅ Implemented** (after this implementation sprint)

**SDK types**: `McpServer` (enum: `Stdio`, `Http`, `Sse`), `McpCapabilities`, `McpServerHttp`, `McpServerSse`, `McpServerStdio`, `HttpHeader`

**What axon implements:**

| Feature | Status | Location |
|---------|--------|----------|
| `McpServer::Stdio` passthrough | ✅ | `mapping::convert_mcp_servers` |
| `McpServer::Http` passthrough | ✅ | `mapping::convert_mcp_servers` |
| `McpServer::Http` with headers | ✅ | `mapping::convert_mcp_servers` + `AcpMcpServerConfig::Http.headers` |
| `McpServer::Sse` passthrough | ✅ | `mapping::convert_mcp_servers` + `AcpMcpServerConfig::Sse` variant |
| `McpCapabilities` reading | ✅ | `session::initialize_connection` → `AcpRuntimeState.mcp_http/sse_supported` |
| Capability-based transport filtering | ✅ | `mapping::filter_sdk_mcp_servers` called in `runtime::establish_acp_session` |
| MCP servers on load-session fallback (one-shot) | ✅ | `session::setup_session` — clones servers before move, passes to fallback |
| MCP servers in persistent load/create | ✅ | `turn::load_or_fallback_session` + `create_new_session` |
| `mcp.json` SSE transport | ✅ | `mcp_config::fetch_axon_mcp_servers_from_disk` — `transport: "sse"` |
| `mcp.json` HTTP/SSE headers | ✅ | `mcp_config::fetch_axon_mcp_servers_from_disk` — `headers` array |
| `blocked_mcp_tools` per-turn | ✅ | `bridge::AcpRuntimeState.blocked_mcp_tools`, set in `turn::build_turn_context` |
```

- [ ] **Step 7.2: Commit**

```bash
git add ACP-GAP-ANALYSIS.md
git commit -m "docs(acp): update gap analysis with full MCP support status"
```

---

## Final Verification

- [ ] Run the full test suite and confirm zero failures:

```bash
cd /home/jmagar/workspace/axon_rust
cargo test --lib 2>&1 | tail -10
```

- [ ] Run clippy clean:

```bash
cargo clippy --lib -- -D warnings 2>&1 | grep "^error"
```

- [ ] Run the monolith check:

```bash
just precommit 2>&1 | grep -E "FAIL|ERROR|exceeded" | head -10
```

Expected: `crates/services/types/acp.rs` flagged but is in the allowlist, everything else passes.

- [ ] Final commit if anything was missed:

```bash
git add -p
git commit -m "chore(acp): final cleanup for MCP full support"
```
