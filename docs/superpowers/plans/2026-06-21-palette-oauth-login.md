# Palette OAuth Login Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a full browser-based OAuth 2.0 "Sign in with Google" login to the Axon Palette desktop app (Authorization Code + PKCE with loopback redirect and dynamic client registration), persist the issued tokens securely, auto-refresh them with single-flight safety, and use them for all Axon HTTP calls while keeping the existing static bearer-token path working simultaneously.

**Architecture:** The OAuth dance runs entirely in the Rust Tauri shell (`src-tauri/`), never the webview — the app's CSP locks `connect-src` to `'self' ipc:` and there is no shell/deep-link capability, so the webview cannot open a browser or receive a redirect. New Tauri commands (`axon_oauth_login`/`axon_oauth_logout`/`axon_oauth_status`) drive: RFC 8414 discovery → bind a `127.0.0.1:0` loopback listener → RFC 7591 dynamic client registration with the exact post-bind `redirect_uri` → open the system browser to `/authorize` → capture `?code&state` on the loopback → exchange the code at `/token` (PKCE S256) → persist `{client_id, access_token, refresh_token?, token_endpoint, expires_at, scope, server_url}` to a `0o600` file beside `settings.json`. Credentials are cached in a single Tauri-managed `OauthState` (a `tokio::sync::Mutex` over the cached credentials + a login guard); the shared `axon_http_request`/`axon_artifact_request`/`axon_http_stream_request` bridge resolves a token through `oauth::resolve_auth_token(...)`: prefer a valid OAuth access token (refreshing under the lock, single-flight, when expired) for the active server, else fall back to the static `settings.token`. A new "Authentication" block in the Connection settings tab drives the commands and shows sign-in status; the static Bearer token field stays.

**Tech Stack:** Rust 2024 (Tauri v2, `reqwest` 0.12 rustls, `tokio`), `sha2` (PKCE S256), `base64` 0.22 (base64url), `uuid` v1 (random verifier/state, OS CSPRNG via `getrandom`), `open` 5 (launch system browser), `url` 2 (URL/query building + endpoint validation). Frontend: React 19 + TypeScript, Vitest. Server side is axon's vendored `lab-auth` OAuth server (Google upstream IdP) — unchanged by this plan; the palette is purely a client.

---

## Review Applied (rev 2)

This plan was revised after a 4-agent engineering review (architecture / simplicity / security / performance). All server-contract claims were verified against `vendor/lab-auth/src/*` and the palette source. Changes folded in:

**Applied (must-fix):**
- **Single-flight refresh + in-memory cache.** A Tauri-managed `OauthState { creds: Mutex<CredCache>, login: Mutex<()> }` caches the active credentials and serializes refreshes, so N concurrent requests at token expiry produce exactly one `/token` call and one disk write (was: per-request file read + N racing refreshes/writes). [Tasks 2,5,6; lib.rs]
- **Persist `token_endpoint` from discovery** in `StoredCredentials`; refresh uses the stored endpoint instead of reconstructing `{server_url}/token` (which breaks behind reverse proxies where the server's `public_url` ≠ the dialed URL). [Tasks 2,3,5]
- **Redacted `Debug`** (manual impl) on `StoredCredentials` and `TokenResponse` — the derive would print live refresh tokens into crash logs. Mirrors `vendor/lab-auth/src/types.rs:163-190`. [Tasks 2,3]
- **`require_secure_url` guard:** reject OAuth network calls and the browser-open over cleartext `http://` for non-loopback hosts (refresh tokens are long-lived). Applied to the server URL and every server-supplied endpoint before use. [Tasks 3,5]
- **Validate server-supplied endpoints** (`authorization_endpoint`/`token_endpoint`/`registration_endpoint`) for safe scheme before `open::that` / HTTP, blocking a hostile discovery doc returning `file://`/`javascript:`/foreign cleartext. [Tasks 3,5]
- **`registration_endpoint` is `Option<String>`** (dropped unused `jwks_uri`); absent → a clear "this server doesn't support OAuth login (dynamic client registration disabled)" error instead of an opaque deserialize failure. [Task 3]

**Applied (hardening / correctness):**
- Callback responses send `Referrer-Policy: no-referrer` and `socket.shutdown()` after write. [Task 4]
- State mismatch on the loopback is **non-fatal** (respond 400 + keep accepting) so a racing local process can't abort a legitimate login; only a state-matching `code`/`error` ends the loop. [Task 4]
- `axon_oauth_status` cross-checks the stored credential's `server_url` against the current settings; a mismatch reports "signed in to a different server" rather than a misleading "Signed in". [Tasks 5,7]
- **Login concurrency guard** (`OauthState.login` `try_lock`): a second concurrent sign-in returns "a sign-in is already in progress". [Task 5]
- `LOGIN_TIMEOUT` is **240s** (< the server's 300s `AUTH_REQUEST_TTL`) so the client times out first with a clear message; on browser-open failure or timeout, the `authorize_url` is surfaced so the user can paste it manually. [Task 5]
- Token-endpoint error strings are **truncated and never echo token material**; the refresh-failure log omits the response body. [Task 3]
- `store::load` logs a warning on a non-`NotFound` read error (so a permissions problem isn't silently indistinguishable from "signed out"). [Task 2]
- Bridge token resolution is centralized in `oauth::resolve_auth_token(...)` (removes the 3× duplicated block). [Tasks 5,6]

**Skipped (with reason):**
- **Constant-time `state` comparison** — negligible local-only timing risk (an attacker who can time loopback TCP can read the socket directly) and would add a `subtle` dep to a separate workspace. Plain `==` retained.
- **`client_id` reuse across logins** — incompatible with ephemeral-port loopback redirects: the server pins each client to its exact registered `redirect_uri`, which changes per login. Documented as a known (rate-limited, server-bounded) tradeoff instead.
- **Strict same-host endpoint check** — would break legitimate reverse-proxy deployments where the server's `public_url` host differs from the dialed host (the same scenario the `token_endpoint` persistence fixes). Scheme validation (`require_secure_url`) is enforced; host equality is not.
- **Folding `pkce.rs` into `flow.rs`** — kept separate; PKCE crypto is a cohesive, independently-testable unit and the focused-file split aids readability with negligible cost.
- **401-triggered reactive refresh in the bridge** — deferred to a follow-up bead (see Notes). Proactive expiry with a 60s skew + single-flight covers the common case; full 401-retry across three bridge functions is out of scope for this slice.

---

## Global Constraints

These apply to **every** task. Copied verbatim from the repo + palette conventions.

- **Monolith policy (CI + lefthook hard-fail):** changed `.rs` files ≤ **500 lines**; functions warn at 80 lines, **hard fail at 120**. Exempt: `tests/**`, `benches/**`, `**/*_tests.*`. Keep new functions under 80 lines — split orchestration into helpers.
- **Rust module layout (ENFORCED):** never `mod.rs`. Module root is `oauth.rs`; submodules live in `oauth/<name>.rs` declared `mod <name>;` inside `oauth.rs`.
- **Rust tests (ENFORCED):** sidecar `_tests.rs` files, one per `#[cfg(test)] mod` block, declared with `#[cfg(test)] #[path = "<file>_tests.rs"] mod tests;`. Use `use super::*;` inside. NOT inline `mod tests {}`.
- **Frontend tests (palette-specific, NOT the Rust rule):** co-located `foo.test.ts(x)` beside `foo.ts(x)`. Run with `pnpm test` (`vitest run`).
- **Frontend invoke seam:** never import `@tauri-apps/api/*` directly in app code — always go through `src/lib/invoke.ts` so the browser-dev path keeps working. Add browser-dev stubs there for every new command.
- **Edition/MSRV:** `apps/palette-tauri/src-tauri` is `edition = "2024"`, `rust-version = "1.94.0"`. Separate cargo workspace (`[workspace]` in its `Cargo.toml`) — it does NOT share the root axon lockfile; new deps land in `apps/palette-tauri/src-tauri/Cargo.lock`.
- **Redirect URI rule (server contract):** must be loopback `http://127.0.0.1:<port>/callback` (scheme MUST be `http`, host `127.0.0.1`). The **same exact string** must be sent to `/register`, `/authorize` (`redirect_uri`), and `/token` (`redirect_uri`) — lab-auth compares with byte equality (`vendor/lab-auth/src/authorize.rs:197-211`, `vendor/lab-auth/src/token.rs:293-303`).
- **PKCE:** `code_challenge = base64url-nopad(SHA256(code_verifier))`, `code_challenge_method = "S256"` (lab-auth rejects anything else: `authorize.rs:212-222`, `token.rs:304-313`).
- **Token endpoint is form-encoded** (`application/x-www-form-urlencoded`), not JSON (`vendor/lab-auth/src/token.rs:22-25` uses `Form<TokenRequest>`). Registration endpoint is JSON.
- **Scope:** request `axon:read axon:write` (the server's `scopes_supported`; `validate_scope` accepts it: `authorize.rs:451-477`).
- **Refresh token is not guaranteed:** lab-auth only returns a `refresh_token` when Google returned an upstream refresh token (`token.rs:105-143`), which depends on server-side Google params the client cannot control. Handle the no-refresh-token case gracefully (require re-login on access-token expiry).
- **OAuth secrets must not cross cleartext:** discovery/registration/token/refresh and the browser-open URL must be `https`, or `http` only for loopback hosts (`require_secure_url`).
- **Never log token values.** `StoredCredentials`/`TokenResponse` use a hand-written redacted `Debug`. Error strings derived from token-endpoint responses must not echo response bodies.
- **Dual-mode:** OAuth and static bearer token coexist (axon sets `disable_static_token_with_oauth(false)`). OAuth wins when a valid token exists for the active server; otherwise the static token is used.
- **Versioning:** the palette is versioned independently from the CLI. This is a `feat` → **minor** bump: `5.10.4` → `5.11.0`, applied together to `apps/palette-tauri/src-tauri/tauri.conf.json`, `apps/palette-tauri/package.json`, and `apps/palette-tauri/src-tauri/Cargo.toml`. No CLI version files change.
- **Gates (run before claiming done):**
  - Rust: `cargo test --manifest-path apps/palette-tauri/src-tauri/Cargo.toml`, `cargo clippy --manifest-path apps/palette-tauri/src-tauri/Cargo.toml --all-targets -- -D warnings`, `cargo fmt --manifest-path apps/palette-tauri/src-tauri/Cargo.toml -- --check`.
  - Frontend (from `apps/palette-tauri/`): `pnpm test`, `pnpm typecheck`, `pnpm lint`.
- **Secrets:** the credentials file holds a refresh token — write it `0o600` atomically (reuse `persistence::atomic_write`). Never commit a real token.

---

## File Structure

**New (Rust — `apps/palette-tauri/src-tauri/src/`):**
- `oauth.rs` — module root. Tauri commands (`axon_oauth_login`, `axon_oauth_logout`, `axon_oauth_status`), the `OauthStatus` DTO, the `OauthState` managed-state type (cached creds + login guard), login orchestration helpers, the bridge-facing `resolve_auth_token(...)` + `effective_access_token(...)` (single-flight refresh), and the pure `pick_token(...)`. Declares the submodules below.
- `oauth/pkce.rs` — `generate_code_verifier()`, `code_challenge_s256()`, `generate_state()`. Pure.
- `oauth/flow.rs` — `AuthServerMetadata`/`TokenResponse` DTOs (redacted `Debug`), pure URL/form/body/validation builders (`discovery_url`, `build_authorize_url`, `registration_body`, `authorization_code_form`, `refresh_form`, `require_secure_url`), and thin async network helpers (`discover`, `register_client`, `exchange_code`, `refresh_access_token`).
- `oauth/callback_server.rs` — loopback listener: `bind()`, `await_code()`, and the pure parsers `parse_request_target()` / `parse_callback_params()`.
- `oauth/store.rs` — `StoredCredentials` (redacted `Debug`), `load`/`save`/`clear`, `credentials_path(app)`, `is_expired`/`matches_server`.
- Sidecar tests: `oauth/pkce_tests.rs`, `oauth/flow_tests.rs`, `oauth/callback_server_tests.rs`, `oauth/store_tests.rs`, `oauth_tests.rs`.

**Modified (Rust):**
- `src-tauri/Cargo.toml` — add `sha2`, `open`; extend `tokio` features (`net`, `io-util`, `time`).
- `src-tauri/src/lib.rs` — `mod oauth;`, register the 3 commands, `.manage(oauth::OauthState::new())`.
- `src-tauri/src/persistence.rs` — make `atomic_write` `pub(crate)` so `oauth::store` reuses it.
- `src-tauri/src/axon_bridge.rs` — resolve OAuth token via `oauth::resolve_auth_token` in `axon_http_request` + `axon_artifact_request` (each gains an `OauthState` param).
- `src-tauri/src/stream.rs` — same resolution in `axon_http_stream_request` (gains an `OauthState` param).

**New / modified (Frontend — `apps/palette-tauri/src/`):**
- `lib/oauthClient.ts` (new) — typed wrappers (`oauthStatus`, `oauthLogin`, `oauthLogout`), `OauthStatus` type, pure `describeOauthStatus()` formatter. `lib/oauthClient.test.ts` (new).
- `lib/invoke.ts` — browser-dev stubs for the 3 new commands.
- `components/palette/SettingsPanel.tsx` — "Authentication" block in `ConnectionPanel`. `components/palette/SettingsPanel.test.tsx` (new) for the auth block.

**Version files (Task 9):** `tauri.conf.json`, `package.json`, `src-tauri/Cargo.toml`, `README.md`.

---

### Task 1: PKCE + state helpers

**Files:**
- Create: `apps/palette-tauri/src-tauri/src/oauth.rs` (temporary minimal root — just declares `pkce`)
- Create: `apps/palette-tauri/src-tauri/src/oauth/pkce.rs`
- Create: `apps/palette-tauri/src-tauri/src/oauth/pkce_tests.rs`
- Modify: `apps/palette-tauri/src-tauri/Cargo.toml` (add `sha2`)
- Modify: `apps/palette-tauri/src-tauri/src/lib.rs` (add `mod oauth;`)

**Interfaces:**
- Produces: `oauth::pkce::generate_code_verifier() -> String` (43-char base64url), `oauth::pkce::code_challenge_s256(verifier: &str) -> String`, `oauth::pkce::generate_state() -> String`.

- [ ] **Step 1: Add the `sha2` dependency**

In `apps/palette-tauri/src-tauri/Cargo.toml`, under `[dependencies]`, add (after `serde_json`):

```toml
sha2 = "0.10"
```

- [ ] **Step 2: Declare the module and create the minimal root**

In `apps/palette-tauri/src-tauri/src/lib.rs`, add `mod oauth;` to the module declarations near the top (after `mod axon_bridge;`, before `mod persistence;`):

```rust
mod axon_bridge;
mod oauth;
mod persistence;
mod stream;
mod window_events;
```

Create `apps/palette-tauri/src-tauri/src/oauth.rs` with just:

```rust
//! OAuth 2.0 (Authorization Code + PKCE) login client for the Axon server.
//!
//! The full flow runs in the Rust shell because the webview CSP forbids
//! outbound HTTP and there is no shell/deep-link capability. See the submodules
//! for the pieces; the Tauri commands and bridge glue are added in later tasks.

pub(crate) mod pkce;
```

- [ ] **Step 3: Write the failing test for PKCE + state**

Create `apps/palette-tauri/src-tauri/src/oauth/pkce_tests.rs`:

```rust
use super::*;

#[test]
fn code_challenge_matches_rfc7636_test_vector() {
    // RFC 7636 Appendix B canonical pair.
    let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
    assert_eq!(
        code_challenge_s256(verifier),
        "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
    );
}

#[test]
fn generated_verifier_is_valid_pkce_shape() {
    let verifier = generate_code_verifier();
    // RFC 7636 §4.1: 43..=128 chars from the unreserved set.
    assert!(
        (43..=128).contains(&verifier.len()),
        "verifier length {} out of range",
        verifier.len()
    );
    assert!(
        verifier
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~')),
        "verifier contains a reserved character: {verifier}"
    );
}

#[test]
fn generated_values_are_unique_per_call() {
    assert_ne!(generate_code_verifier(), generate_code_verifier());
    assert_ne!(generate_state(), generate_state());
}
```

- [ ] **Step 4: Run the test to verify it fails**

Run: `cargo test --manifest-path apps/palette-tauri/src-tauri/Cargo.toml pkce`
Expected: FAIL — `oauth/pkce.rs` does not exist / functions undefined.

- [ ] **Step 5: Implement `oauth/pkce.rs`**

Create `apps/palette-tauri/src-tauri/src/oauth/pkce.rs`:

```rust
//! PKCE (RFC 7636) code-verifier/challenge and CSRF `state` generation.
//! Randomness is sourced from v4 UUIDs (`getrandom` → OS CSPRNG).

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};

/// 32 cryptographically-random bytes sourced from two v4 UUIDs (`getrandom`).
fn random_bytes_32() -> [u8; 32] {
    let mut out = [0u8; 32];
    out[..16].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
    out[16..].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
    out
}

/// A fresh PKCE code verifier: base64url(no-pad) of 32 random bytes → 43 chars.
pub(crate) fn generate_code_verifier() -> String {
    URL_SAFE_NO_PAD.encode(random_bytes_32())
}

/// The S256 challenge for a verifier: base64url(no-pad) of SHA-256(verifier).
/// Matches lab-auth's `pkce_challenge` (vendor/lab-auth/src/token.rs:271-273).
pub(crate) fn code_challenge_s256(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// A random CSRF `state` value (base64url of 16 random bytes → 22 chars).
pub(crate) fn generate_state() -> String {
    URL_SAFE_NO_PAD.encode(uuid::Uuid::new_v4().as_bytes())
}

#[cfg(test)]
#[path = "pkce_tests.rs"]
mod tests;
```

- [ ] **Step 6: Run the test to verify it passes**

Run: `cargo test --manifest-path apps/palette-tauri/src-tauri/Cargo.toml pkce`
Expected: PASS (3 tests).

- [ ] **Step 7: Commit**

```bash
git add apps/palette-tauri/src-tauri/Cargo.toml apps/palette-tauri/src-tauri/Cargo.lock apps/palette-tauri/src-tauri/src/lib.rs apps/palette-tauri/src-tauri/src/oauth.rs apps/palette-tauri/src-tauri/src/oauth/pkce.rs apps/palette-tauri/src-tauri/src/oauth/pkce_tests.rs
git commit -m "feat(palette): add PKCE verifier/challenge helpers for OAuth login"
```

---

### Task 2: OAuth credential store

**Files:**
- Create: `apps/palette-tauri/src-tauri/src/oauth/store.rs`
- Create: `apps/palette-tauri/src-tauri/src/oauth/store_tests.rs`
- Modify: `apps/palette-tauri/src-tauri/src/oauth.rs` (declare `store`)
- Modify: `apps/palette-tauri/src-tauri/src/persistence.rs` (expose `atomic_write`)

**Interfaces:**
- Consumes: `persistence::atomic_write(path: &Path, data: &[u8]) -> Result<(), Box<dyn std::error::Error>>`.
- Produces:
  - `oauth::store::StoredCredentials { client_id: String, access_token: String, refresh_token: Option<String>, token_endpoint: String, expires_at_unix: i64, scope: String, server_url: String }` (derives `Clone, Serialize, Deserialize`; **hand-written redacted `Debug`**).
  - `StoredCredentials::is_expired(&self, now_unix: i64, skew_secs: i64) -> bool`
  - `StoredCredentials::matches_server(&self, server_url: &str) -> bool`
  - `oauth::store::load(path: &Path) -> Option<StoredCredentials>`
  - `oauth::store::save(path: &Path, creds: &StoredCredentials) -> Result<(), String>`
  - `oauth::store::clear(path: &Path) -> Result<(), String>`
  - `oauth::store::credentials_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String>`

- [ ] **Step 1: Expose `atomic_write` from persistence**

In `apps/palette-tauri/src-tauri/src/persistence.rs`, change:

```rust
fn atomic_write(path: &Path, data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
```

to:

```rust
pub(crate) fn atomic_write(path: &Path, data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
```

- [ ] **Step 2: Declare the `store` submodule**

In `apps/palette-tauri/src-tauri/src/oauth.rs`, add under the existing `pkce` declaration:

```rust
pub(crate) mod pkce;
pub(crate) mod store;
```

- [ ] **Step 3: Write the failing test**

Create `apps/palette-tauri/src-tauri/src/oauth/store_tests.rs`:

```rust
use super::*;
use std::env;

fn sample(server: &str, refresh: Option<&str>, expires_at: i64) -> StoredCredentials {
    StoredCredentials {
        client_id: "client-123".to_string(),
        access_token: "access-abc".to_string(),
        refresh_token: refresh.map(str::to_string),
        token_endpoint: format!("{server}/token"),
        expires_at_unix: expires_at,
        scope: "axon:read axon:write".to_string(),
        server_url: server.to_string(),
    }
}

#[test]
fn save_then_load_round_trips() {
    let dir = env::temp_dir().join(format!("axon-oauth-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("oauth.json");

    let creds = sample("https://axon.example.com", Some("refresh-xyz"), 4_102_444_800);
    save(&path, &creds).unwrap();
    let loaded = load(&path).expect("credentials present after save");

    assert_eq!(loaded.client_id, "client-123");
    assert_eq!(loaded.access_token, "access-abc");
    assert_eq!(loaded.refresh_token.as_deref(), Some("refresh-xyz"));
    assert_eq!(loaded.token_endpoint, "https://axon.example.com/token");
    assert_eq!(loaded.server_url, "https://axon.example.com");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn load_missing_file_returns_none() {
    let path = env::temp_dir().join(format!("axon-oauth-missing-{}.json", uuid::Uuid::new_v4()));
    assert!(load(&path).is_none());
}

#[test]
fn clear_removes_the_file_and_is_idempotent() {
    let dir = env::temp_dir().join(format!("axon-oauth-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("oauth.json");
    save(&path, &sample("https://a", None, 0)).unwrap();
    clear(&path).unwrap();
    assert!(load(&path).is_none());
    clear(&path).unwrap(); // second clear must not error
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn expiry_accounts_for_skew() {
    let creds = sample("https://a", None, 1000);
    assert!(!creds.is_expired(900, 30)); // 900 + 30 < 1000 → valid
    assert!(creds.is_expired(980, 30)); // 980 + 30 >= 1000 → treat as expired
    assert!(creds.is_expired(1000, 0));
}

#[test]
fn matches_server_is_exact_after_trailing_slash_trim() {
    let creds = sample("https://axon.example.com", None, 0);
    assert!(creds.matches_server("https://axon.example.com"));
    assert!(creds.matches_server("https://axon.example.com/"));
    assert!(!creds.matches_server("https://other.example.com"));
}

#[test]
fn debug_redacts_token_fields() {
    let creds = sample("https://axon.example.com", Some("refresh-xyz"), 0);
    let rendered = format!("{creds:?}");
    assert!(!rendered.contains("access-abc"), "access token leaked: {rendered}");
    assert!(!rendered.contains("refresh-xyz"), "refresh token leaked: {rendered}");
    assert!(rendered.contains("client-123"), "non-secret field should remain");
}
```

- [ ] **Step 4: Run the test to verify it fails**

Run: `cargo test --manifest-path apps/palette-tauri/src-tauri/Cargo.toml oauth::store`
Expected: FAIL — `store` module/functions undefined.

- [ ] **Step 5: Implement `oauth/store.rs`**

Create `apps/palette-tauri/src-tauri/src/oauth/store.rs`:

```rust
//! Persistence for OAuth credentials, stored beside `settings.json` in the
//! app config dir as `oauth.json` (mode 0o600). Holds a sensitive refresh
//! token — the `Debug` impl is hand-written and redacted; never derive it.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

const CREDENTIALS_FILE: &str = "oauth.json";

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct StoredCredentials {
    pub client_id: String,
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    /// The token endpoint discovered at login. Refresh posts here rather than
    /// reconstructing `{server_url}/token`, which breaks behind reverse proxies.
    pub token_endpoint: String,
    pub expires_at_unix: i64,
    pub scope: String,
    pub server_url: String,
}

impl std::fmt::Debug for StoredCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoredCredentials")
            .field("client_id", &self.client_id)
            .field("access_token", &"<redacted>")
            .field("refresh_token", &self.refresh_token.as_ref().map(|_| "<redacted>"))
            .field("token_endpoint", &self.token_endpoint)
            .field("expires_at_unix", &self.expires_at_unix)
            .field("scope", &self.scope)
            .field("server_url", &self.server_url)
            .finish()
    }
}

impl StoredCredentials {
    /// True when the access token is at or past expiry once `skew_secs` of
    /// safety margin is applied.
    pub(crate) fn is_expired(&self, now_unix: i64, skew_secs: i64) -> bool {
        now_unix + skew_secs >= self.expires_at_unix
    }

    /// True when these credentials were issued for `server_url` (trailing
    /// slashes ignored on both sides).
    pub(crate) fn matches_server(&self, server_url: &str) -> bool {
        self.server_url.trim_end_matches('/') == server_url.trim_end_matches('/')
    }
}

/// Resolve the credentials file path (`<app_config_dir>/oauth.json`).
pub(crate) fn credentials_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|dir| dir.join(CREDENTIALS_FILE))
        .map_err(|err| format!("failed to resolve app config directory: {err}"))
}

/// Load credentials, returning `None` when the file is missing or unparseable
/// (a corrupt file degrades to "signed out", never a hard error). A non-missing
/// read error is logged so it is not silently indistinguishable from absence.
pub(crate) fn load(path: &Path) -> Option<StoredCredentials> {
    let contents = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return None,
        Err(err) => {
            eprintln!("palette: failed to read oauth credentials: {err}");
            return None;
        }
    };
    match serde_json::from_str(&contents) {
        Ok(creds) => Some(creds),
        Err(err) => {
            eprintln!("palette: ignoring unparseable oauth credentials: {err}");
            None
        }
    }
}

/// Persist credentials atomically with `0o600` perms.
pub(crate) fn save(path: &Path, creds: &StoredCredentials) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let json = serde_json::to_string_pretty(creds).map_err(|err| err.to_string())?;
    crate::persistence::atomic_write(path, json.as_bytes()).map_err(|err| err.to_string())
}

/// Remove the credentials file. Missing file is success (idempotent).
pub(crate) fn clear(path: &Path) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.to_string()),
    }
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
```

- [ ] **Step 6: Run the test to verify it passes**

Run: `cargo test --manifest-path apps/palette-tauri/src-tauri/Cargo.toml oauth::store`
Expected: PASS (6 tests).

- [ ] **Step 7: Commit**

```bash
git add apps/palette-tauri/src-tauri/src/persistence.rs apps/palette-tauri/src-tauri/src/oauth.rs apps/palette-tauri/src-tauri/src/oauth/store.rs apps/palette-tauri/src-tauri/src/oauth/store_tests.rs
git commit -m "feat(palette): add secure OAuth credential store with redacted Debug"
```

---

### Task 3: OAuth flow builders, validation + network helpers

**Files:**
- Create: `apps/palette-tauri/src-tauri/src/oauth/flow.rs`
- Create: `apps/palette-tauri/src-tauri/src/oauth/flow_tests.rs`
- Modify: `apps/palette-tauri/src-tauri/src/oauth.rs` (declare `flow`)

**Interfaces:**
- Produces:
  - `oauth::flow::AuthServerMetadata { issuer, authorization_endpoint, token_endpoint, registration_endpoint: Option<String> }` (deserialized from discovery; extra fields ignored).
  - `oauth::flow::TokenResponse { access_token, token_type, expires_in: u64, refresh_token: Option<String>, scope }` (redacted `Debug`).
  - Pure: `discovery_url(base_url) -> String`, `require_secure_url(raw) -> Result<url::Url, String>`, `build_authorize_url(meta, client_id, redirect_uri, scope, state, challenge) -> Result<String, String>`, `registration_body(redirect_uri) -> serde_json::Value`, `authorization_code_form(...) -> Vec<(&'static str, String)>`, `refresh_form(...) -> Vec<(&'static str, String)>`.
  - Async (each `&reqwest::Client`): `discover(client, base_url) -> Result<AuthServerMetadata, String>`, `register_client(client, registration_endpoint, redirect_uri) -> Result<String, String>`, `exchange_code(client, token_endpoint, code, client_id, redirect_uri, verifier) -> Result<TokenResponse, String>`, `refresh_access_token(client, token_endpoint, client_id, refresh_token) -> Result<TokenResponse, String>`.

- [ ] **Step 1: Declare the `flow` submodule**

In `apps/palette-tauri/src-tauri/src/oauth.rs`:

```rust
pub(crate) mod flow;
pub(crate) mod pkce;
pub(crate) mod store;
```

- [ ] **Step 2: Write the failing test**

Create `apps/palette-tauri/src-tauri/src/oauth/flow_tests.rs`:

```rust
use super::*;

fn meta() -> AuthServerMetadata {
    AuthServerMetadata {
        issuer: "https://axon.example.com".to_string(),
        authorization_endpoint: "https://axon.example.com/authorize".to_string(),
        token_endpoint: "https://axon.example.com/token".to_string(),
        registration_endpoint: Some("https://axon.example.com/register".to_string()),
    }
}

#[test]
fn discovery_url_appends_well_known_path() {
    assert_eq!(
        discovery_url("https://axon.example.com/"),
        "https://axon.example.com/.well-known/oauth-authorization-server"
    );
}

#[test]
fn metadata_deserializes_ignoring_extra_fields_and_optional_registration() {
    let json = r#"{
        "issuer": "https://axon.example.com",
        "authorization_endpoint": "https://axon.example.com/authorize",
        "token_endpoint": "https://axon.example.com/token",
        "registration_endpoint": "https://axon.example.com/register",
        "jwks_uri": "https://axon.example.com/jwks",
        "response_types_supported": ["code"]
    }"#;
    let parsed: AuthServerMetadata = serde_json::from_str(json).unwrap();
    assert_eq!(parsed.token_endpoint, "https://axon.example.com/token");
    assert_eq!(parsed.registration_endpoint.as_deref(), Some("https://axon.example.com/register"));

    // DCR-disabled server omits registration_endpoint → None, not a parse error.
    let no_dcr = r#"{
        "issuer": "https://axon.example.com",
        "authorization_endpoint": "https://axon.example.com/authorize",
        "token_endpoint": "https://axon.example.com/token"
    }"#;
    let parsed: AuthServerMetadata = serde_json::from_str(no_dcr).unwrap();
    assert!(parsed.registration_endpoint.is_none());
}

#[test]
fn token_response_deserializes_with_and_without_refresh() {
    let with = r#"{"access_token":"a","token_type":"Bearer","expires_in":3600,"refresh_token":"r","scope":"axon:read axon:write"}"#;
    let parsed: TokenResponse = serde_json::from_str(with).unwrap();
    assert_eq!(parsed.refresh_token.as_deref(), Some("r"));
    assert_eq!(parsed.expires_in, 3600);

    let without = r#"{"access_token":"a","token_type":"Bearer","expires_in":3600,"scope":"axon:read axon:write"}"#;
    let parsed: TokenResponse = serde_json::from_str(without).unwrap();
    assert!(parsed.refresh_token.is_none());
}

#[test]
fn token_response_debug_redacts_tokens() {
    let parsed: TokenResponse = serde_json::from_str(
        r#"{"access_token":"secret-a","token_type":"Bearer","expires_in":3600,"refresh_token":"secret-r","scope":"axon:read"}"#,
    )
    .unwrap();
    let rendered = format!("{parsed:?}");
    assert!(!rendered.contains("secret-a"));
    assert!(!rendered.contains("secret-r"));
}

#[test]
fn require_secure_url_allows_https_and_loopback_http_only() {
    assert!(require_secure_url("https://axon.example.com/token").is_ok());
    assert!(require_secure_url("http://127.0.0.1:8001/token").is_ok());
    assert!(require_secure_url("http://localhost:8001/token").is_ok());
    assert!(require_secure_url("http://axon.example.com/token").is_err()); // cleartext non-loopback
    assert!(require_secure_url("file:///etc/passwd").is_err());
    assert!(require_secure_url("not a url").is_err());
}

#[test]
fn authorize_url_carries_all_required_pkce_params() {
    let url = build_authorize_url(
        &meta(),
        "client-123",
        "http://127.0.0.1:7777/callback",
        "axon:read axon:write",
        "state-xyz",
        "challenge-abc",
    )
    .unwrap();
    assert!(url.starts_with("https://axon.example.com/authorize?"));
    assert!(url.contains("response_type=code"));
    assert!(url.contains("client_id=client-123"));
    assert!(url.contains("code_challenge=challenge-abc"));
    assert!(url.contains("code_challenge_method=S256"));
    assert!(url.contains("state=state-xyz"));
    assert!(url.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A7777%2Fcallback"));
    assert!(url.contains("scope=axon%3Aread+axon%3Awrite"));
}

#[test]
fn registration_body_wraps_single_redirect_uri() {
    assert_eq!(
        registration_body("http://127.0.0.1:7777/callback"),
        serde_json::json!({ "redirect_uris": ["http://127.0.0.1:7777/callback"] })
    );
}

#[test]
fn token_forms_have_required_fields() {
    let auth = authorization_code_form("code-1", "client-123", "http://127.0.0.1:7777/callback", "verifier-1");
    assert!(auth.contains(&("grant_type", "authorization_code".to_string())));
    assert!(auth.contains(&("code", "code-1".to_string())));
    assert!(auth.contains(&("client_id", "client-123".to_string())));
    assert!(auth.contains(&("redirect_uri", "http://127.0.0.1:7777/callback".to_string())));
    assert!(auth.contains(&("code_verifier", "verifier-1".to_string())));

    let refresh = refresh_form("client-123", "refresh-1");
    assert!(refresh.contains(&("grant_type", "refresh_token".to_string())));
    assert!(refresh.contains(&("refresh_token", "refresh-1".to_string())));
    assert!(refresh.contains(&("client_id", "client-123".to_string())));
}
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cargo test --manifest-path apps/palette-tauri/src-tauri/Cargo.toml oauth::flow`
Expected: FAIL — `flow` module/items undefined.

- [ ] **Step 4: Implement `oauth/flow.rs`**

Create `apps/palette-tauri/src-tauri/src/oauth/flow.rs`:

```rust
//! OAuth wire protocol: RFC 8414 discovery, RFC 7591 dynamic client
//! registration, and the PKCE authorization-code + refresh token exchanges.
//! Pure builders/validators are unit-tested; the async wrappers are thin
//! reqwest calls. Token-bearing error strings never echo response bodies.

use serde::Deserialize;

/// Subset of the RFC 8414 authorization-server metadata the client needs.
/// Extra fields in the document are ignored. `registration_endpoint` is
/// optional — a DCR-disabled server omits it.
#[derive(Clone, Debug, Deserialize)]
pub(crate) struct AuthServerMetadata {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    #[serde(default)]
    pub registration_endpoint: Option<String>,
}

/// The `/token` success response (lab-auth omits `refresh_token` when the
/// upstream IdP did not return one). `Debug` is redacted — never derive it.
#[derive(Clone, Deserialize)]
pub(crate) struct TokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u64,
    #[serde(default)]
    pub refresh_token: Option<String>,
    pub scope: String,
}

impl std::fmt::Debug for TokenResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenResponse")
            .field("access_token", &"<redacted>")
            .field("token_type", &self.token_type)
            .field("expires_in", &self.expires_in)
            .field("refresh_token", &self.refresh_token.as_ref().map(|_| "<redacted>"))
            .field("scope", &self.scope)
            .finish()
    }
}

#[derive(Deserialize)]
struct ClientRegistrationResponse {
    client_id: String,
}

pub(crate) fn discovery_url(base_url: &str) -> String {
    format!(
        "{}/.well-known/oauth-authorization-server",
        base_url.trim_end_matches('/')
    )
}

/// Reject any URL that is not `https`, or `http` on a loopback host. OAuth
/// secrets (auth code, PKCE verifier, refresh token) must never traverse
/// cleartext to a non-loopback host.
pub(crate) fn require_secure_url(raw: &str) -> Result<url::Url, String> {
    let url = url::Url::parse(raw).map_err(|err| format!("invalid OAuth URL `{raw}`: {err}"))?;
    match url.scheme() {
        "https" => Ok(url),
        "http" if matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "::1")) => Ok(url),
        _ => Err(format!(
            "refusing OAuth over an insecure URL `{raw}` — https is required for non-loopback hosts"
        )),
    }
}

pub(crate) fn build_authorize_url(
    meta: &AuthServerMetadata,
    client_id: &str,
    redirect_uri: &str,
    scope: &str,
    state: &str,
    code_challenge: &str,
) -> Result<String, String> {
    let mut url = url::Url::parse(&meta.authorization_endpoint)
        .map_err(|err| format!("invalid authorization_endpoint: {err}"))?;
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("scope", scope)
        .append_pair("state", state)
        .append_pair("code_challenge", code_challenge)
        .append_pair("code_challenge_method", "S256");
    Ok(url.to_string())
}

pub(crate) fn registration_body(redirect_uri: &str) -> serde_json::Value {
    serde_json::json!({ "redirect_uris": [redirect_uri] })
}

pub(crate) fn authorization_code_form(
    code: &str,
    client_id: &str,
    redirect_uri: &str,
    code_verifier: &str,
) -> Vec<(&'static str, String)> {
    vec![
        ("grant_type", "authorization_code".to_string()),
        ("code", code.to_string()),
        ("client_id", client_id.to_string()),
        ("redirect_uri", redirect_uri.to_string()),
        ("code_verifier", code_verifier.to_string()),
    ]
}

pub(crate) fn refresh_form(client_id: &str, refresh_token: &str) -> Vec<(&'static str, String)> {
    vec![
        ("grant_type", "refresh_token".to_string()),
        ("refresh_token", refresh_token.to_string()),
        ("client_id", client_id.to_string()),
    ]
}

pub(crate) async fn discover(
    client: &reqwest::Client,
    base_url: &str,
) -> Result<AuthServerMetadata, String> {
    let url = discovery_url(base_url);
    let response = client
        .get(&url)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|err| format!("OAuth discovery request failed: {err}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "OAuth discovery returned HTTP {} — is the server in OAuth mode (AXON_MCP_AUTH_MODE=oauth)?",
            response.status()
        ));
    }
    response
        .json()
        .await
        .map_err(|err| format!("OAuth discovery returned an invalid document: {err}"))
}

pub(crate) async fn register_client(
    client: &reqwest::Client,
    registration_endpoint: &str,
    redirect_uri: &str,
) -> Result<String, String> {
    let response = client
        .post(registration_endpoint)
        .json(&registration_body(redirect_uri))
        .send()
        .await
        .map_err(|err| format!("client registration request failed: {err}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "client registration returned HTTP {}",
            response.status()
        ));
    }
    let registered: ClientRegistrationResponse = response
        .json()
        .await
        .map_err(|err| format!("client registration returned an invalid response: {err}"))?;
    Ok(registered.client_id)
}

pub(crate) async fn exchange_code(
    client: &reqwest::Client,
    token_endpoint: &str,
    code: &str,
    client_id: &str,
    redirect_uri: &str,
    code_verifier: &str,
) -> Result<TokenResponse, String> {
    post_token_form(
        client,
        token_endpoint,
        &authorization_code_form(code, client_id, redirect_uri, code_verifier),
    )
    .await
}

pub(crate) async fn refresh_access_token(
    client: &reqwest::Client,
    token_endpoint: &str,
    client_id: &str,
    refresh_token: &str,
) -> Result<TokenResponse, String> {
    post_token_form(client, token_endpoint, &refresh_form(client_id, refresh_token)).await
}

async fn post_token_form(
    client: &reqwest::Client,
    token_endpoint: &str,
    form: &[(&'static str, String)],
) -> Result<TokenResponse, String> {
    let response = client
        .post(token_endpoint)
        .form(form)
        .send()
        .await
        .map_err(|err| format!("token request failed: {err}"))?;
    let status = response.status();
    if !status.is_success() {
        // Do NOT echo the response body — a non-standard server could reflect
        // submitted token material back in its error body.
        return Err(format!("token endpoint returned HTTP {status}"));
    }
    let text = response.text().await.map_err(|err| err.to_string())?;
    serde_json::from_str(&text)
        .map_err(|_| "token endpoint returned an invalid response".to_string())
}

#[cfg(test)]
#[path = "flow_tests.rs"]
mod tests;
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test --manifest-path apps/palette-tauri/src-tauri/Cargo.toml oauth::flow`
Expected: PASS (8 tests).

- [ ] **Step 6: Commit**

```bash
git add apps/palette-tauri/src-tauri/src/oauth.rs apps/palette-tauri/src-tauri/src/oauth/flow.rs apps/palette-tauri/src-tauri/src/oauth/flow_tests.rs
git commit -m "feat(palette): add OAuth discovery, registration, token exchange + URL validation"
```

---

### Task 4: Loopback callback server

**Files:**
- Create: `apps/palette-tauri/src-tauri/src/oauth/callback_server.rs`
- Create: `apps/palette-tauri/src-tauri/src/oauth/callback_server_tests.rs`
- Modify: `apps/palette-tauri/src-tauri/src/oauth.rs` (declare `callback_server`)
- Modify: `apps/palette-tauri/src-tauri/Cargo.toml` (extend `tokio` features)

**Interfaces:**
- Produces:
  - `oauth::callback_server::CallbackListener { pub port: u16, pub redirect_uri: String, ... }`
  - `oauth::callback_server::bind() -> Result<CallbackListener, String>`
  - `CallbackListener::await_code(&self, expected_state: &str, timeout: std::time::Duration) -> Result<String, String>`
  - Pure: `parse_request_target(request_line: &str) -> Option<&str>`, `parse_callback_params(target: &str) -> CallbackParams { code, state, error }`.

- [ ] **Step 1: Extend tokio features**

In `apps/palette-tauri/src-tauri/Cargo.toml`, replace:

```toml
tokio = { version = "1", features = ["sync"] }
```

with:

```toml
tokio = { version = "1", features = ["sync", "net", "io-util", "time"] }
```

- [ ] **Step 2: Declare the `callback_server` submodule**

In `apps/palette-tauri/src-tauri/src/oauth.rs`:

```rust
pub(crate) mod callback_server;
pub(crate) mod flow;
pub(crate) mod pkce;
pub(crate) mod store;
```

- [ ] **Step 3: Write the failing test**

Create `apps/palette-tauri/src-tauri/src/oauth/callback_server_tests.rs`:

```rust
use super::*;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[test]
fn parse_request_target_extracts_path_and_query() {
    assert_eq!(
        parse_request_target("GET /callback?code=abc&state=xyz HTTP/1.1"),
        Some("/callback?code=abc&state=xyz")
    );
    assert_eq!(parse_request_target("GET / HTTP/1.1"), Some("/"));
    assert_eq!(parse_request_target("garbage"), None);
    assert_eq!(parse_request_target("GET notapath HTTP/1.1"), None);
}

#[test]
fn parse_callback_params_reads_code_state_and_error() {
    let ok = parse_callback_params("/callback?code=abc&state=xyz");
    assert_eq!(ok.code.as_deref(), Some("abc"));
    assert_eq!(ok.state.as_deref(), Some("xyz"));
    assert!(ok.error.is_none());

    let denied = parse_callback_params("/callback?error=access_denied&state=xyz");
    assert_eq!(denied.error.as_deref(), Some("access_denied"));
    assert!(denied.code.is_none());

    let empty = parse_callback_params("/callback");
    assert!(empty.code.is_none() && empty.state.is_none() && empty.error.is_none());
}

async fn send_request(port: u16, line: &str) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    stream
        .write_all(format!("{line}\r\nHost: localhost\r\n\r\n").as_bytes())
        .await
        .unwrap();
    let mut buf = Vec::new();
    let _ = stream.read_to_end(&mut buf).await; // drain so the server write completes
}

#[tokio::test]
async fn await_code_returns_code_for_matching_state() {
    let listener = bind().await.unwrap();
    let port = listener.port;
    assert_eq!(listener.redirect_uri, format!("http://127.0.0.1:{port}/callback"));

    let client = tokio::spawn(async move {
        send_request(port, "GET /callback?code=the-code&state=expected HTTP/1.1").await;
    });
    let code = listener
        .await_code("expected", Duration::from_secs(5))
        .await
        .unwrap();
    assert_eq!(code, "the-code");
    client.await.unwrap();
}

#[tokio::test]
async fn await_code_ignores_state_mismatch_and_resolves_on_later_match() {
    // A racing/wrong-state request must NOT abort the flow; the real callback
    // (correct state) arriving afterward still resolves the login.
    let listener = bind().await.unwrap();
    let port = listener.port;
    tokio::spawn(async move {
        send_request(port, "GET /callback?code=evil&state=wrong HTTP/1.1").await;
        send_request(port, "GET /favicon.ico HTTP/1.1").await;
        send_request(port, "GET /callback?code=real-code&state=expected HTTP/1.1").await;
    });
    let code = listener
        .await_code("expected", Duration::from_secs(5))
        .await
        .unwrap();
    assert_eq!(code, "real-code");
}

#[tokio::test]
async fn await_code_returns_error_for_matching_state_denial() {
    let listener = bind().await.unwrap();
    let port = listener.port;
    tokio::spawn(async move {
        send_request(port, "GET /callback?error=access_denied&state=expected HTTP/1.1").await;
    });
    let err = listener
        .await_code("expected", Duration::from_secs(5))
        .await
        .unwrap_err();
    assert!(err.contains("denied"), "unexpected error: {err}");
}
```

- [ ] **Step 4: Run the test to verify it fails**

Run: `cargo test --manifest-path apps/palette-tauri/src-tauri/Cargo.toml oauth::callback_server`
Expected: FAIL — module/items undefined.

- [ ] **Step 5: Implement `oauth/callback_server.rs`**

Create `apps/palette-tauri/src-tauri/src/oauth/callback_server.rs`:

```rust
//! Loopback HTTP listener that captures the OAuth `?code&state` redirect.
//! RFC 8252 §7.3 native-app pattern: bind 127.0.0.1:0, register that exact
//! `redirect_uri`, then accept browser requests until one carries the matching
//! state. A non-matching request (favicon, a racing local process with a wrong
//! state) is answered and ignored — only a state-matching code/error ends the
//! loop — so a hostile local request cannot abort a legitimate login.

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

const MAX_REQUEST_BYTES: usize = 8192;

const SUCCESS_PAGE: &str =
    "<!doctype html><html><body style=\"font-family:sans-serif;background:#07131c;color:#e6f4fb;\
     text-align:center;padding-top:4rem\"><h2>Signed in to Axon</h2>\
     <p>You can close this tab and return to the palette.</p></body></html>";

pub(crate) struct CallbackListener {
    listener: TcpListener,
    pub port: u16,
    pub redirect_uri: String,
}

pub(crate) struct CallbackParams {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
}

/// Bind a loopback listener on an ephemeral port. The `redirect_uri` string is
/// fixed here and must be reused verbatim for `/register`, `/authorize`, and
/// `/token`.
pub(crate) async fn bind() -> Result<CallbackListener, String> {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .map_err(|err| format!("failed to bind loopback callback listener: {err}"))?;
    let port = listener
        .local_addr()
        .map_err(|err| err.to_string())?
        .port();
    let redirect_uri = format!("http://127.0.0.1:{port}/callback");
    Ok(CallbackListener {
        listener,
        port,
        redirect_uri,
    })
}

impl CallbackListener {
    /// Accept connections until one carries the OAuth redirect with the matching
    /// `state`, returning the authorization `code`. Times out after `timeout`.
    pub(crate) async fn await_code(
        &self,
        expected_state: &str,
        timeout: Duration,
    ) -> Result<String, String> {
        tokio::time::timeout(timeout, self.accept_loop(expected_state))
            .await
            .map_err(|_| "timed out waiting for the OAuth redirect".to_string())?
    }

    async fn accept_loop(&self, expected_state: &str) -> Result<String, String> {
        loop {
            let (mut socket, _) = self
                .listener
                .accept()
                .await
                .map_err(|err| err.to_string())?;
            let Some(target) = read_request_target(&mut socket).await else {
                respond(&mut socket, "400 Bad Request", "Bad Request").await;
                continue;
            };
            let params = parse_callback_params(&target);
            // Only a request bearing OUR state is the real callback. Anything
            // else (favicon, a racing process with a wrong/absent state) is
            // answered and ignored so it cannot abort the flow.
            if params.state.as_deref() != Some(expected_state) {
                respond(&mut socket, "404 Not Found", "Not Found").await;
                continue;
            }
            if let Some(error) = params.error {
                respond(&mut socket, "400 Bad Request", SUCCESS_PAGE).await;
                return Err(format!("authorization was denied ({error})"));
            }
            if let Some(code) = params.code {
                respond(&mut socket, "200 OK", SUCCESS_PAGE).await;
                return Ok(code);
            }
            respond(&mut socket, "400 Bad Request", "Missing code").await;
        }
    }
}

async fn read_request_target(socket: &mut TcpStream) -> Option<String> {
    let mut buf = vec![0u8; MAX_REQUEST_BYTES];
    let n = socket.read(&mut buf).await.ok()?;
    let head = String::from_utf8_lossy(&buf[..n]);
    let request_line = head.lines().next()?;
    parse_request_target(request_line).map(str::to_string)
}

async fn respond(socket: &mut TcpStream, status: &str, body: &str) {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\nReferrer-Policy: no-referrer\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = socket.write_all(response.as_bytes()).await;
    let _ = socket.flush().await;
    let _ = socket.shutdown().await;
}

/// Extract the request target (path + query) from an HTTP request line.
pub(crate) fn parse_request_target(request_line: &str) -> Option<&str> {
    let mut parts = request_line.split_whitespace();
    let _method = parts.next()?;
    let target = parts.next()?;
    target.starts_with('/').then_some(target)
}

/// Parse `code`/`state`/`error` from a `/callback?...` target.
pub(crate) fn parse_callback_params(target: &str) -> CallbackParams {
    let mut params = CallbackParams {
        code: None,
        state: None,
        error: None,
    };
    if let Ok(url) = url::Url::parse(&format!("http://127.0.0.1{target}")) {
        for (key, value) in url.query_pairs() {
            match key.as_ref() {
                "code" => params.code = Some(value.into_owned()),
                "state" => params.state = Some(value.into_owned()),
                "error" => params.error = Some(value.into_owned()),
                _ => {}
            }
        }
    }
    params
}

#[cfg(test)]
#[path = "callback_server_tests.rs"]
mod tests;
```

- [ ] **Step 6: Run the test to verify it passes**

Run: `cargo test --manifest-path apps/palette-tauri/src-tauri/Cargo.toml oauth::callback_server`
Expected: PASS (5 tests).

- [ ] **Step 7: Commit**

```bash
git add apps/palette-tauri/src-tauri/Cargo.toml apps/palette-tauri/src-tauri/Cargo.lock apps/palette-tauri/src-tauri/src/oauth.rs apps/palette-tauri/src-tauri/src/oauth/callback_server.rs apps/palette-tauri/src-tauri/src/oauth/callback_server_tests.rs
git commit -m "feat(palette): add loopback OAuth redirect listener (state-gated, no-referrer)"
```

---

### Task 5: Commands, managed state + single-flight token resolution

**Files:**
- Modify: `apps/palette-tauri/src-tauri/src/oauth.rs` (commands, `OauthStatus`, `OauthState`, orchestration, `effective_access_token`, `resolve_auth_token`, `pick_token`, `status_for`)
- Create: `apps/palette-tauri/src-tauri/src/oauth_tests.rs`
- Modify: `apps/palette-tauri/src-tauri/Cargo.toml` (add `open`)
- Modify: `apps/palette-tauri/src-tauri/src/lib.rs` (register commands + `.manage(OauthState::new())`)

**Interfaces:**
- Consumes: Tasks 1–4; `crate::merged_settings`, `crate::validate_saved_server_url`, `crate::axon_bridge::BridgeClient`, `crate::PaletteSettings`.
- Produces:
  - `oauth::OauthStatus { signed_in: bool, scope: Option<String>, expires_at_unix: Option<i64>, server_url: Option<String> }` (serde camelCase, `Serialize`).
  - `oauth::OauthState` (Tauri-managed: cached creds + login guard) with `OauthState::new()`.
  - `#[tauri::command] async fn oauth::axon_oauth_login(app, bridge: State<BridgeClient>, oauth_state: State<OauthState>) -> Result<OauthStatus, String>`
  - `#[tauri::command] async fn oauth::axon_oauth_logout(app, oauth_state: State<OauthState>) -> Result<OauthStatus, String>`
  - `#[tauri::command] async fn oauth::axon_oauth_status(app, oauth_state: State<OauthState>) -> Result<OauthStatus, String>`
  - `pub(crate) async fn oauth::resolve_auth_token(app: &AppHandle, client: &reqwest::Client, server_url: &str, static_token: Option<&str>, state: &OauthState) -> Option<String>` — consumed by Task 6.
  - Pure `pick_token(oauth: Option<String>, static_token: Option<String>) -> Option<String>`; pure `status_for(creds: Option<&StoredCredentials>, current_server: &str) -> OauthStatus`.

- [ ] **Step 1: Add the `open` dependency**

In `apps/palette-tauri/src-tauri/Cargo.toml`, under `[dependencies]`, add:

```toml
open = "5"
```

- [ ] **Step 2: Write the failing test**

Create `apps/palette-tauri/src-tauri/src/oauth_tests.rs`:

```rust
use super::*;
use crate::oauth::store::StoredCredentials;

fn creds(server: &str) -> StoredCredentials {
    StoredCredentials {
        client_id: "c".to_string(),
        access_token: "a".to_string(),
        refresh_token: None,
        token_endpoint: format!("{server}/token"),
        expires_at_unix: 4_102_444_800,
        scope: "axon:read axon:write".to_string(),
        server_url: server.to_string(),
    }
}

#[test]
fn pick_token_prefers_oauth_then_static() {
    assert_eq!(
        pick_token(Some("oauth".to_string()), Some("static".to_string())),
        Some("oauth".to_string())
    );
    assert_eq!(pick_token(None, Some("static".to_string())), Some("static".to_string()));
    assert_eq!(pick_token(None, None), None);
}

#[test]
fn status_for_reports_signed_in_only_when_server_matches() {
    let c = creds("https://axon.example.com");

    let matched = status_for(Some(&c), "https://axon.example.com");
    assert!(matched.signed_in);
    assert_eq!(matched.scope.as_deref(), Some("axon:read axon:write"));

    // Credentials for a different server → not signed in here, but the stored
    // server_url is surfaced so the UI can explain the mismatch.
    let mismatched = status_for(Some(&c), "https://other.example.com");
    assert!(!mismatched.signed_in);
    assert_eq!(mismatched.server_url.as_deref(), Some("https://axon.example.com"));

    let none = status_for(None, "https://axon.example.com");
    assert!(!none.signed_in);
    assert!(none.server_url.is_none());
}
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cargo test --manifest-path apps/palette-tauri/src-tauri/Cargo.toml oauth_tests`
Expected: FAIL — items undefined.

- [ ] **Step 4: Implement the command surface in `oauth.rs`**

Set the full contents of `apps/palette-tauri/src-tauri/src/oauth.rs` to (module declarations on top, then the body):

```rust
//! OAuth 2.0 (Authorization Code + PKCE) login client for the Axon server.
//!
//! The full flow runs in the Rust shell because the webview CSP forbids
//! outbound HTTP and there is no shell/deep-link capability. Credentials are
//! cached in `OauthState` (Tauri-managed); refresh is single-flight under the
//! cache lock.

pub(crate) mod callback_server;
pub(crate) mod flow;
pub(crate) mod pkce;
pub(crate) mod store;

use std::time::Duration;

use serde::Serialize;
use tauri::AppHandle;

use crate::axon_bridge::BridgeClient;
use crate::oauth::store::StoredCredentials;
use crate::{merged_settings, validate_saved_server_url};

/// Client login timeout, kept below the server's 300s auth-request TTL so the
/// client times out first with a clear message.
const LOGIN_TIMEOUT: Duration = Duration::from_secs(240);
/// Refresh the access token this many seconds before its stated expiry.
const EXPIRY_SKEW_SECS: i64 = 60;
const SCOPE: &str = "axon:read axon:write";

/// Cached credentials for the current process. `Unloaded` until first access,
/// then `Loaded(Some|None)`.
enum CredCache {
    Unloaded,
    Loaded(Option<StoredCredentials>),
}

/// Tauri-managed OAuth state: the credential cache (whose lock also serializes
/// refresh — single-flight) and a guard that serializes interactive logins.
pub(crate) struct OauthState {
    creds: tokio::sync::Mutex<CredCache>,
    login: tokio::sync::Mutex<()>,
}

impl OauthState {
    pub(crate) fn new() -> Self {
        OauthState {
            creds: tokio::sync::Mutex::new(CredCache::Unloaded),
            login: tokio::sync::Mutex::new(()),
        }
    }
}

impl Default for OauthState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OauthStatus {
    pub signed_in: bool,
    pub scope: Option<String>,
    pub expires_at_unix: Option<i64>,
    pub server_url: Option<String>,
}

/// Build a status for the UI: signed in only when the stored credentials match
/// the currently-configured server. On a server mismatch, `signed_in` is false
/// but `server_url` carries the credential's server so the UI can explain it.
pub(crate) fn status_for(creds: Option<&StoredCredentials>, current_server: &str) -> OauthStatus {
    match creds {
        Some(creds) if creds.matches_server(current_server) => OauthStatus {
            signed_in: true,
            scope: Some(creds.scope.clone()),
            expires_at_unix: Some(creds.expires_at_unix),
            server_url: Some(creds.server_url.clone()),
        },
        Some(creds) => OauthStatus {
            signed_in: false,
            scope: None,
            expires_at_unix: None,
            server_url: Some(creds.server_url.clone()),
        },
        None => OauthStatus {
            signed_in: false,
            scope: None,
            expires_at_unix: None,
            server_url: None,
        },
    }
}

#[tauri::command]
pub(crate) async fn axon_oauth_login(
    app: AppHandle,
    bridge: tauri::State<'_, BridgeClient>,
    oauth_state: tauri::State<'_, OauthState>,
) -> Result<OauthStatus, String> {
    // Serialize interactive logins — a second concurrent click is rejected.
    let _login_guard = oauth_state
        .login
        .try_lock()
        .map_err(|_| "a sign-in is already in progress".to_string())?;

    let settings = merged_settings(&app)?;
    let server_url = validate_saved_server_url(&settings.server_url)?;
    let client = bridge.client().clone();

    let creds = run_login(&client, &server_url).await?;
    let path = store::credentials_path(&app)?;
    store::save(&path, &creds)?;
    *oauth_state.creds.lock().await = CredCache::Loaded(Some(creds.clone()));
    Ok(status_for(Some(&creds), &server_url))
}

#[tauri::command]
pub(crate) async fn axon_oauth_logout(
    app: AppHandle,
    oauth_state: tauri::State<'_, OauthState>,
) -> Result<OauthStatus, String> {
    let path = store::credentials_path(&app)?;
    store::clear(&path)?;
    *oauth_state.creds.lock().await = CredCache::Loaded(None);
    Ok(OauthStatus {
        signed_in: false,
        scope: None,
        expires_at_unix: None,
        server_url: None,
    })
}

#[tauri::command]
pub(crate) async fn axon_oauth_status(
    app: AppHandle,
    oauth_state: tauri::State<'_, OauthState>,
) -> Result<OauthStatus, String> {
    let settings = merged_settings(&app)?;
    let server_url = validate_saved_server_url(&settings.server_url)?;
    let mut cache = oauth_state.creds.lock().await;
    ensure_loaded(&app, &mut cache);
    let CredCache::Loaded(slot) = &*cache else {
        unreachable!("ensure_loaded sets Loaded")
    };
    Ok(status_for(slot.as_ref(), &server_url))
}

/// Resolve the token to attach to a bridge request: a valid OAuth access token
/// for `server_url` (refreshing single-flight on expiry), else the static token.
pub(crate) async fn resolve_auth_token(
    app: &AppHandle,
    client: &reqwest::Client,
    server_url: &str,
    static_token: Option<&str>,
    state: &OauthState,
) -> Option<String> {
    let oauth = effective_access_token(app, client, server_url, state).await;
    pick_token(oauth, static_token.map(str::to_string))
}

/// The cached OAuth access token for `server_url`, refreshed if expired. Holds
/// the cache lock across any refresh so concurrent callers single-flight.
async fn effective_access_token(
    app: &AppHandle,
    client: &reqwest::Client,
    server_url: &str,
    state: &OauthState,
) -> Option<String> {
    let mut cache = state.creds.lock().await;
    ensure_loaded(app, &mut cache);
    let CredCache::Loaded(slot) = &mut *cache else {
        unreachable!("ensure_loaded sets Loaded")
    };

    // Snapshot the fields needed before any await (ends the borrow on `slot`).
    let (client_id, token_endpoint, refresh_token, access_token, valid) = {
        let creds = slot.as_ref()?;
        if !creds.matches_server(server_url) {
            return None;
        }
        (
            creds.client_id.clone(),
            creds.token_endpoint.clone(),
            creds.refresh_token.clone(),
            creds.access_token.clone(),
            !creds.is_expired(now_unix(), EXPIRY_SKEW_SECS),
        )
    };
    if valid {
        return Some(access_token);
    }

    // Expired. Re-validate the stored endpoint, then single-flight a refresh.
    let refresh_token = refresh_token?;
    let token_endpoint = flow::require_secure_url(&token_endpoint).ok()?.to_string();
    match flow::refresh_access_token(client, &token_endpoint, &client_id, &refresh_token).await {
        Ok(token) => {
            let refreshed =
                credentials_from_token(client_id, server_url, token_endpoint, token);
            let access = refreshed.access_token.clone();
            if let Ok(path) = store::credentials_path(app) {
                let _ = store::save(&path, &refreshed);
            }
            *slot = Some(refreshed);
            Some(access)
        }
        Err(err) => {
            eprintln!("palette: OAuth token refresh failed, falling back: {err}");
            None
        }
    }
}

/// Run the browser-based authorization-code flow and return fresh credentials.
async fn run_login(
    client: &reqwest::Client,
    server_url: &str,
) -> Result<StoredCredentials, String> {
    flow::require_secure_url(server_url)?;
    let meta = flow::discover(client, server_url).await?;
    let registration_endpoint = meta.registration_endpoint.clone().ok_or_else(|| {
        "this server does not support OAuth login (dynamic client registration is disabled) — \
         use a static bearer token instead"
            .to_string()
    })?;
    // Validate every server-supplied endpoint before using it.
    flow::require_secure_url(&meta.authorization_endpoint)?;
    flow::require_secure_url(&meta.token_endpoint)?;
    flow::require_secure_url(&registration_endpoint)?;

    let listener = callback_server::bind().await?;
    let client_id =
        flow::register_client(client, &registration_endpoint, &listener.redirect_uri).await?;

    let verifier = pkce::generate_code_verifier();
    let challenge = pkce::code_challenge_s256(&verifier);
    let state = pkce::generate_state();
    let authorize_url = flow::build_authorize_url(
        &meta,
        &client_id,
        &listener.redirect_uri,
        SCOPE,
        &state,
        &challenge,
    )?;

    if let Err(err) = open::that(&authorize_url) {
        return Err(format!(
            "failed to open the system browser — open this URL manually to sign in:\n{authorize_url}\n({err})"
        ));
    }

    let code = listener
        .await_code(&state, LOGIN_TIMEOUT)
        .await
        .map_err(|err| format!("{err}. If the browser did not open, sign in here:\n{authorize_url}"))?;

    let token = flow::exchange_code(
        client,
        &meta.token_endpoint,
        &code,
        &client_id,
        &listener.redirect_uri,
        &verifier,
    )
    .await?;

    Ok(credentials_from_token(
        client_id,
        server_url,
        meta.token_endpoint,
        token,
    ))
}

fn credentials_from_token(
    client_id: String,
    server_url: &str,
    token_endpoint: String,
    token: flow::TokenResponse,
) -> StoredCredentials {
    StoredCredentials {
        client_id,
        access_token: token.access_token,
        refresh_token: token.refresh_token,
        token_endpoint,
        expires_at_unix: now_unix() + token.expires_in as i64,
        scope: token.scope,
        server_url: server_url.trim_end_matches('/').to_string(),
    }
}

/// Populate the cache from disk on first use. Caller holds the cache lock.
fn ensure_loaded(app: &AppHandle, cache: &mut CredCache) {
    if let CredCache::Unloaded = cache {
        let loaded = store::credentials_path(app)
            .ok()
            .and_then(|path| store::load(&path));
        *cache = CredCache::Loaded(loaded);
    }
}

/// Prefer an OAuth token over the static bearer token.
pub(crate) fn pick_token(oauth: Option<String>, static_token: Option<String>) -> Option<String> {
    oauth.or(static_token)
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
#[path = "oauth_tests.rs"]
mod tests;
```

If `oauth.rs` exceeds 500 lines or any function exceeds 120, split the command bodies into an `oauth/commands.rs` submodule (pre-authorized by the monolith policy). As written it is ~260 lines with the largest function (`run_login`) ~55 lines.

- [ ] **Step 5: Register the commands and managed state in `lib.rs`**

In `apps/palette-tauri/src-tauri/src/lib.rs`, add the three OAuth commands to the `invoke_handler!` list (after `axon_http_stream_request`):

```rust
            axon_http_stream_request,
            oauth::axon_oauth_login,
            oauth::axon_oauth_logout,
            oauth::axon_oauth_status
        ])
```

and register the managed state alongside the existing `.manage(...)` calls (after `.manage(stream_client)`):

```rust
        .manage(bridge_client)
        .manage(stream_client)
        .manage(oauth::OauthState::new())
```

- [ ] **Step 6: Run the unit tests + full build**

Run: `cargo test --manifest-path apps/palette-tauri/src-tauri/Cargo.toml oauth_tests`
Expected: PASS (2 tests).

Run: `cargo build --manifest-path apps/palette-tauri/src-tauri/Cargo.toml`
Expected: builds cleanly (commands + managed state compile and register).

- [ ] **Step 7: Commit**

```bash
git add apps/palette-tauri/src-tauri/Cargo.toml apps/palette-tauri/src-tauri/Cargo.lock apps/palette-tauri/src-tauri/src/oauth.rs apps/palette-tauri/src-tauri/src/oauth_tests.rs apps/palette-tauri/src-tauri/src/lib.rs
git commit -m "feat(palette): add OAuth commands, managed state, and single-flight token resolution"
```

---

### Task 6: Bridge dual-mode token injection

**Files:**
- Modify: `apps/palette-tauri/src-tauri/src/axon_bridge.rs` (`axon_http_request`, `axon_artifact_request`)
- Modify: `apps/palette-tauri/src-tauri/src/stream.rs` (`axon_http_stream_request`)

**Interfaces:**
- Consumes: `oauth::resolve_auth_token(app, client, server_url, static_token, oauth_state)`, `oauth::OauthState`.

Each of the three commands gains an `oauth_state: tauri::State<'_, crate::oauth::OauthState>` parameter and replaces its direct `settings.token` attachment with a `resolve_auth_token(...)` call.

- [ ] **Step 1: Update `axon_http_request` in `axon_bridge.rs`**

Add the managed-state parameter to the command signature:

```rust
pub(crate) async fn axon_http_request(
    app: AppHandle,
    bridge: tauri::State<'_, BridgeClient>,
    oauth_state: tauri::State<'_, crate::oauth::OauthState>,
    request: AxonHttpRequest,
) -> Result<AxonHttpResult, String> {
```

Then replace the token block (leave the `if let Some(body)` block untouched):

```rust
    if let Some(token) = settings
        .token
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
    {
        builder = builder.bearer_auth(token).header("x-api-key", token);
    }
```

with:

```rust
    let static_token = settings
        .token
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty());
    if let Some(token) =
        crate::oauth::resolve_auth_token(&app, client, &base_url, static_token, &oauth_state).await
    {
        builder = builder.bearer_auth(&token).header("x-api-key", &token);
    }
```

(`client` is already `(*bridge).client()` and `base_url` the validated server URL.)

- [ ] **Step 2: Update `axon_artifact_request` in `axon_bridge.rs`**

Add the parameter:

```rust
pub(crate) async fn axon_artifact_request(
    app: AppHandle,
    bridge: tauri::State<'_, BridgeClient>,
    oauth_state: tauri::State<'_, crate::oauth::OauthState>,
    relative_path: String,
) -> Result<AxonArtifactResult, String> {
```

Replace:

```rust
    if let Some(token) = settings
        .token
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
    {
        request = request.bearer_auth(token).header("x-api-key", token);
    }
```

with:

```rust
    let static_token = settings
        .token
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty());
    if let Some(token) =
        crate::oauth::resolve_auth_token(&app, client, &base_url, static_token, &oauth_state).await
    {
        request = request.bearer_auth(&token).header("x-api-key", &token);
    }
```

- [ ] **Step 3: Update `axon_http_stream_request` in `stream.rs`**

Add the parameter to the command signature:

```rust
pub(crate) async fn axon_http_stream_request(
    app: AppHandle,
    window: tauri::Window,
    stream_client: tauri::State<'_, StreamClient>,
    oauth_state: tauri::State<'_, crate::oauth::OauthState>,
    request: PaletteStreamRequest,
) -> Result<(), String> {
```

Replace:

```rust
    let mut builder = (*stream_client)
        .client()
        .post(url)
        .header(reqwest::header::ACCEPT, "text/event-stream")
        .json(&request.body);
    if let Some(token) = settings
        .token
        .as_deref()
        .map(str::trim)
        .filter(|token| !token.is_empty())
    {
        builder = builder.bearer_auth(token).header("x-api-key", token);
    }
```

with:

```rust
    let client = (*stream_client).client();
    let mut builder = client
        .post(url)
        .header(reqwest::header::ACCEPT, "text/event-stream")
        .json(&request.body);
    let static_token = settings
        .token
        .as_deref()
        .map(str::trim)
        .filter(|token| !token.is_empty());
    if let Some(token) =
        crate::oauth::resolve_auth_token(&app, client, &base_url, static_token, &oauth_state).await
    {
        builder = builder.bearer_auth(&token).header("x-api-key", &token);
    }
```

- [ ] **Step 4: Run the full Rust gate**

Run: `cargo test --manifest-path apps/palette-tauri/src-tauri/Cargo.toml`
Expected: PASS (all existing + new tests).

Run: `cargo clippy --manifest-path apps/palette-tauri/src-tauri/Cargo.toml --all-targets -- -D warnings`
Expected: no warnings.

Run: `cargo fmt --manifest-path apps/palette-tauri/src-tauri/Cargo.toml` then `cargo fmt --manifest-path apps/palette-tauri/src-tauri/Cargo.toml -- --check`
Expected: formats; check is clean.

- [ ] **Step 5: Commit**

```bash
git add apps/palette-tauri/src-tauri/src/axon_bridge.rs apps/palette-tauri/src-tauri/src/stream.rs
git commit -m "feat(palette): resolve OAuth token (single-flight) per bridge request, fall back to static"
```

---

### Task 7: Frontend OAuth client + browser-dev stubs

**Files:**
- Create: `apps/palette-tauri/src/lib/oauthClient.ts`
- Create: `apps/palette-tauri/src/lib/oauthClient.test.ts`
- Modify: `apps/palette-tauri/src/lib/invoke.ts` (browser-dev stubs)

**Interfaces:**
- Produces:
  - `OauthStatus { signedIn: boolean; scope: string | null; expiresAtUnix: number | null; serverUrl: string | null }`
  - `oauthStatus(): Promise<OauthStatus>`, `oauthLogin(): Promise<OauthStatus>`, `oauthLogout(): Promise<OauthStatus>`
  - Pure `describeOauthStatus(status, nowUnix?): { label: string; detail: string; tone: "neutral" | "success" | "error" }`

- [ ] **Step 1: Write the failing test**

Create `apps/palette-tauri/src/lib/oauthClient.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { describeOauthStatus, type OauthStatus } from "./oauthClient";

const signedOut: OauthStatus = { signedIn: false, scope: null, expiresAtUnix: null, serverUrl: null };

describe("describeOauthStatus", () => {
  it("reports signed-out state", () => {
    const result = describeOauthStatus(signedOut);
    expect(result.tone).toBe("neutral");
    expect(result.label).toBe("Not signed in");
  });

  it("reports an active session", () => {
    const status: OauthStatus = {
      signedIn: true,
      scope: "axon:read axon:write",
      expiresAtUnix: 4_102_444_800,
      serverUrl: "https://axon.example.com",
    };
    const result = describeOauthStatus(status, 1_700_000_000);
    expect(result.tone).toBe("success");
    expect(result.label).toBe("Signed in");
    expect(result.detail).toContain("axon.example.com");
  });

  it("flags an expired session", () => {
    const status: OauthStatus = {
      signedIn: true,
      scope: "axon:read",
      expiresAtUnix: 1_000,
      serverUrl: "https://axon.example.com",
    };
    const result = describeOauthStatus(status, 2_000);
    expect(result.tone).toBe("error");
    expect(result.label).toBe("Session expired");
  });

  it("flags credentials issued for a different server", () => {
    const status: OauthStatus = {
      signedIn: false,
      scope: null,
      expiresAtUnix: null,
      serverUrl: "https://other.example.com",
    };
    const result = describeOauthStatus(status);
    expect(result.tone).toBe("error");
    expect(result.label).toBe("Different server");
    expect(result.detail).toContain("other.example.com");
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run (from `apps/palette-tauri/`): `pnpm test oauthClient`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement `oauthClient.ts`**

Create `apps/palette-tauri/src/lib/oauthClient.ts`:

```ts
// OAuth login client. Wraps the Rust Tauri commands through the shared invoke
// seam so the browser-dev path keeps working (never import @tauri-apps/* here).
import { invoke } from "./invoke";

export interface OauthStatus {
  signedIn: boolean;
  scope: string | null;
  expiresAtUnix: number | null;
  serverUrl: string | null;
}

export function oauthStatus(): Promise<OauthStatus> {
  return invoke<OauthStatus>("axon_oauth_status");
}

export function oauthLogin(): Promise<OauthStatus> {
  return invoke<OauthStatus>("axon_oauth_login");
}

export function oauthLogout(): Promise<OauthStatus> {
  return invoke<OauthStatus>("axon_oauth_logout");
}

type Tone = "neutral" | "success" | "error";

export function describeOauthStatus(
  status: OauthStatus,
  nowUnix: number = Math.floor(Date.now() / 1000),
): { label: string; detail: string; tone: Tone } {
  if (status.signedIn) {
    const host = hostOf(status.serverUrl);
    if (status.expiresAtUnix != null && status.expiresAtUnix <= nowUnix) {
      return { tone: "error", label: "Session expired", detail: `Your ${host} session expired — sign in again.` };
    }
    return {
      tone: "success",
      label: "Signed in",
      detail: `Authorized to ${host}${status.scope ? ` (${status.scope})` : ""}.`,
    };
  }
  // Not signed in. If a credential exists for another server, explain it.
  if (status.serverUrl) {
    return {
      tone: "error",
      label: "Different server",
      detail: `Signed in to ${hostOf(status.serverUrl)}, not the current server — sign in again.`,
    };
  }
  return {
    tone: "neutral",
    label: "Not signed in",
    detail: "Sign in with Google to authorize this server via OAuth.",
  };
}

function hostOf(serverUrl: string | null): string {
  if (!serverUrl) return "the server";
  try {
    return new URL(serverUrl).host;
  } catch {
    return serverUrl;
  }
}
```

- [ ] **Step 4: Add browser-dev stubs in `invoke.ts`**

In `apps/palette-tauri/src/lib/invoke.ts`, inside the `switch (command)` block, add cases before `default:`:

```ts
    case "axon_oauth_status":
    case "axon_oauth_logout":
      return { signedIn: false, scope: null, expiresAtUnix: null, serverUrl: null } as T;
    case "axon_oauth_login":
      throw new Error("OAuth login is only available in the desktop app");
```

- [ ] **Step 5: Run the test to verify it passes**

Run (from `apps/palette-tauri/`): `pnpm test oauthClient`
Expected: PASS (4 tests).

- [ ] **Step 6: Commit**

```bash
git add apps/palette-tauri/src/lib/oauthClient.ts apps/palette-tauri/src/lib/oauthClient.test.ts apps/palette-tauri/src/lib/invoke.ts
git commit -m "feat(palette): add frontend OAuth client and browser-dev stubs"
```

---

### Task 8: "Authentication" UI in the Connection tab

**Files:**
- Modify: `apps/palette-tauri/src/components/palette/SettingsPanel.tsx` (add an Authentication block to `ConnectionPanel`)
- Create: `apps/palette-tauri/src/components/palette/SettingsPanel.test.tsx`
- Modify: `apps/palette-tauri/src/styles.css` (status styles)

**Interfaces:**
- Consumes: `oauthStatus`, `oauthLogin`, `oauthLogout`, `describeOauthStatus`, `OauthStatus` from `@/lib/oauthClient`; the existing `Button` aurora primitive.

- [ ] **Step 1: Write the failing render test**

Create `apps/palette-tauri/src/components/palette/SettingsPanel.test.tsx`:

```tsx
import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const oauthState = { value: { signedIn: false, scope: null, expiresAtUnix: null, serverUrl: null } };

vi.mock("@/lib/oauthClient", async () => {
  const actual = await vi.importActual<typeof import("@/lib/oauthClient")>("@/lib/oauthClient");
  return {
    ...actual,
    oauthStatus: vi.fn(() => Promise.resolve(oauthState.value)),
    oauthLogin: vi.fn(() => Promise.resolve(oauthState.value)),
    oauthLogout: vi.fn(() => Promise.resolve(oauthState.value)),
  };
});

import { SettingsPanel } from "./SettingsPanel";
import type { PaletteConfig } from "@/lib/axonClient";

const config: PaletteConfig = {
  serverUrl: "https://axon.example.com",
  token: null,
  shortcut: "Ctrl+Shift+Space",
  collection: "axon",
  resultLimit: 10,
  theme: "dark",
  hideOnBlur: false,
  openResultsInline: true,
  envValues: {},
  configValues: {},
};

describe("SettingsPanel authentication block", () => {
  beforeEach(() => {
    oauthState.value = { signedIn: false, scope: null, expiresAtUnix: null, serverUrl: null };
  });

  it("shows a Sign in button when signed out", async () => {
    render(
      <SettingsPanel
        configError={null}
        draftConfig={config}
        shortcutOptions={["Ctrl+Shift+Space"]}
        onChange={() => {}}
        onClose={() => {}}
        onSave={() => {}}
      />,
    );
    await waitFor(() =>
      expect(screen.getByRole("button", { name: /sign in with google/i })).toBeInTheDocument(),
    );
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run (from `apps/palette-tauri/`): `pnpm test SettingsPanel`
Expected: FAIL — no "Sign in with Google" button.

- [ ] **Step 3: Add the Authentication block to `ConnectionPanel`**

In `apps/palette-tauri/src/components/palette/SettingsPanel.tsx`:

Replace `import { useRef, useState } from "react";` with:

```tsx
import { useEffect, useRef, useState } from "react";
```

Add after the other `@/lib` imports:

```tsx
import { describeOauthStatus, oauthLogin, oauthLogout, oauthStatus, type OauthStatus } from "@/lib/oauthClient";
```

Add a new component above `ConnectionPanel`:

```tsx
function AuthBlock() {
  const [status, setStatus] = useState<OauthStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    oauthStatus()
      .then((next) => active && setStatus(next))
      .catch(() => active && setStatus(null));
    return () => {
      active = false;
    };
  }, []);

  const view = status
    ? describeOauthStatus(status)
    : { label: "Checking…", detail: "Reading saved credentials…", tone: "neutral" as const };

  const run = async (action: () => Promise<OauthStatus>) => {
    setBusy(true);
    setError(null);
    try {
      setStatus(await action());
    } catch (err) {
      setError(err instanceof Error ? err.message : "OAuth request failed.");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="settings-stack">
      <span className="settings-section-label">Authentication</span>
      <div className="settings-auth-status" data-tone={view.tone} aria-live="polite">
        <strong>{view.label}</strong>
        <span>{view.detail}</span>
        {error && <span className="settings-error">{error}</span>}
      </div>
      {status?.signedIn ? (
        <Button size="sm" variant="neutral" disabled={busy} onClick={() => void run(oauthLogout)}>
          <KeyRound size={14} />
          {busy ? "Working…" : "Sign out"}
        </Button>
      ) : (
        <Button size="sm" variant="aurora" disabled={busy} onClick={() => void run(oauthLogin)}>
          <KeyRound size={14} />
          {busy ? "Opening browser…" : "Sign in with Google"}
        </Button>
      )}
    </div>
  );
}
```

Render `<AuthBlock />` inside `ConnectionPanel`'s returned JSX, after the first `settings-stack` (Server/Bearer token/Collection) and before the "Client" stack:

```tsx
  return (
    <div className="settings-connection-grid">
      <div className="settings-stack">
        <span className="settings-section-label">Connection</span>
        {/* Server / Bearer token / Collection fields unchanged */}
        ...
      </div>
      <AuthBlock />
      <div className="settings-stack">
        <span className="settings-section-label">Client</span>
        {/* Global shortcut / Max results / toggles unchanged */}
        ...
      </div>
    </div>
  );
```

Keep the existing "Bearer token" `SecretInput` field exactly as-is (dual-mode).

- [ ] **Step 4: Add minimal styles**

In `apps/palette-tauri/src/styles.css`, add:

```css
.settings-auth-status {
  display: flex;
  flex-direction: column;
  gap: 2px;
  font-size: 12px;
  color: var(--aurora-text-muted);
}
.settings-auth-status strong {
  color: var(--aurora-text-primary);
}
.settings-auth-status[data-tone="success"] strong {
  color: var(--aurora-status-success);
}
.settings-auth-status[data-tone="error"] strong {
  color: var(--aurora-status-error);
}
```

If any of these `--aurora-*` token names are not defined in `src/styles.css`/`src/components/aurora.css`, substitute the nearest existing token (grep `--aurora-` for the success/error/muted/primary names actually in use) — do not invent new tokens or hardcode hex.

- [ ] **Step 5: Run the test to verify it passes**

Run (from `apps/palette-tauri/`): `pnpm test SettingsPanel`
Expected: PASS.

- [ ] **Step 6: Run the frontend gate**

Run (from `apps/palette-tauri/`): `pnpm test && pnpm typecheck && pnpm lint`
Expected: all pass.

- [ ] **Step 7: Commit**

```bash
git add apps/palette-tauri/src/components/palette/SettingsPanel.tsx apps/palette-tauri/src/components/palette/SettingsPanel.test.tsx apps/palette-tauri/src/styles.css
git commit -m "feat(palette): add Sign in with Google to the Connection settings tab"
```

---

### Task 9: Version bump + docs

**Files:**
- Modify: `apps/palette-tauri/src-tauri/tauri.conf.json` (`5.10.4` → `5.11.0`)
- Modify: `apps/palette-tauri/package.json` (`5.10.4` → `5.11.0`)
- Modify: `apps/palette-tauri/src-tauri/Cargo.toml` (`5.10.4` → `5.11.0`)
- Modify: `apps/palette-tauri/README.md` (document OAuth login)

- [ ] **Step 1: Bump all three palette version files**

Set `"version": "5.11.0"` in `apps/palette-tauri/src-tauri/tauri.conf.json` (line 4), `"version": "5.11.0"` in `apps/palette-tauri/package.json` (line 3), and `version = "5.11.0"` in `apps/palette-tauri/src-tauri/Cargo.toml` (line 3). Do NOT touch the root `Cargo.toml` or any CLI version files.

- [ ] **Step 2: Refresh the palette Cargo.lock version entry**

Run: `cargo build --manifest-path apps/palette-tauri/src-tauri/Cargo.toml`
Expected: builds; `apps/palette-tauri/src-tauri/Cargo.lock` updates the `axon-palette-tauri` package version to `5.11.0`.

- [ ] **Step 3: Document the feature in the palette README**

In `apps/palette-tauri/README.md`, add an "Authentication" subsection (near the security/runtime model): the palette supports both a static bearer token and OAuth "Sign in with Google"; OAuth runs an Authorization Code + PKCE loopback flow entirely in the Rust shell (no webview HTTP, no new capabilities); credentials are stored at `<app config dir>/oauth.json` (mode `0o600`) and cached in-process with single-flight refresh; when signed in, the OAuth token takes precedence over the static token; OAuth requires `https` (or loopback `http`) and the target server running with `AXON_MCP_AUTH_MODE=oauth` and dynamic client registration enabled. Note the known tradeoff: each sign-in dynamically registers a fresh client on the server (loopback redirects use an ephemeral port, so client IDs cannot be reused); server-side this is rate-limited and bounded by operator policy.

- [ ] **Step 4: Validate the release plan picks up only the palette**

Run: `cargo xtask check-release-versions --base origin/main --head HEAD --mode pr`
Expected: PASS — palette version bump detected; no other component flagged.

- [ ] **Step 5: Commit**

```bash
git add apps/palette-tauri/src-tauri/tauri.conf.json apps/palette-tauri/package.json apps/palette-tauri/src-tauri/Cargo.toml apps/palette-tauri/src-tauri/Cargo.lock apps/palette-tauri/README.md
git commit -m "chore(palette): bump to 5.11.0 and document OAuth login"
```

---

## Final Verification

- [ ] `cargo test --manifest-path apps/palette-tauri/src-tauri/Cargo.toml` — all pass
- [ ] `cargo clippy --manifest-path apps/palette-tauri/src-tauri/Cargo.toml --all-targets -- -D warnings` — clean
- [ ] `cargo fmt --manifest-path apps/palette-tauri/src-tauri/Cargo.toml -- --check` — clean
- [ ] From `apps/palette-tauri/`: `pnpm test && pnpm typecheck && pnpm lint` — all pass
- [ ] Monolith check: every new `.rs` file ≤ 500 lines; no function > 120 lines (`oauth.rs` is the largest — confirm < 500 and `run_login`/`effective_access_token` < 120; split into `oauth/commands.rs` if it grows).
- [ ] Manual smoke (optional, needs a live `AXON_MCP_AUTH_MODE=oauth` axon server): set the palette server URL to that server, click "Sign in with Google", complete the browser flow, confirm the status flips to "Signed in", then run an action (e.g. Test connection / doctor) and confirm it succeeds without a static token set. Change the server URL and confirm the status shows "Different server".

## Notes for the Implementer

- **Why DCR is mandatory:** lab-auth's `/authorize` rejects unknown `client_id`s and validates `redirect_uri` against the *registered* client (`vendor/lab-auth/src/authorize.rs:185-211`). Bind the listener *before* registering so the exact `redirect_uri` string is known. Reuse `listener.redirect_uri` for register/authorize/token — never reconstruct it.
- **Single-flight refresh:** `effective_access_token` holds the `OauthState.creds` lock across the refresh network call. Concurrent requests at expiry block briefly and reuse the one refreshed token; exactly one `/token` call and one file write occur. The fast path (valid token) is a lock + clone — cheap at desktop scale.
- **Token endpoint is persisted, not reconstructed:** refresh posts to the stored `token_endpoint` from discovery, so reverse-proxy deployments (where the server's `public_url` ≠ the dialed URL) refresh correctly.
- **No new Tauri capability/CSP changes:** the browser is launched with the `open` crate and the redirect is captured by a Rust `TcpListener` — both pure Rust, outside the webview sandbox. Do not add `shell`/deep-link capabilities or relax the CSP.
- **Refresh-token absence is normal:** if the server returns no `refresh_token`, `effective_access_token` returns `None` once the access token expires and the bridge falls back to the static token (or the user re-signs-in). Expected, not a bug.
- **`resource` param omitted intentionally:** optional and, when present, must equal the canonical `<public_url>/mcp` (`vendor/lab-auth/src/token.rs:74`, `authorize.rs:479-499`). Omitting it avoids a brittle dependency on the server's public-URL canonicalization.
- **Deferred follow-up (file a bead):** *reactive 401 refresh.* Today a clock-skew-induced 401 on a bridge call is surfaced to the UI rather than triggering a refresh+retry. Proactive expiry (60s skew) + single-flight covers the common path; a one-shot "on 401 with OAuth creds, force-refresh and retry once" across the three bridge commands is a worthwhile follow-up but out of scope for this slice.
- **Known tradeoff (documented, not fixed):** each login dynamically registers a fresh client on the server (ephemeral-port loopback redirects preclude client-ID reuse). Server-side this is rate-limited (`vendor/lab-auth` register rate limit) and bounded by operator policy; the client does not GC registrations.
