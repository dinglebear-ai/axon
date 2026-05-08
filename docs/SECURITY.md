# Security Model
Last Modified: 2026-05-06

## Table of Contents

1. Scope and Threat Model
2. SSRF and URL Validation
3. HTTP Client Safety
4. MCP HTTP Authentication
5. Host and CORS Allowlists
6. Web Admin Panel
7. Secrets Handling
8. Network Exposure
9. Operational Checklist
10. Source Map

---

## 1. Scope and Threat Model

This document captures the security controls present in the Axon code base today. Axon is a single-binary Rust application (`axon`) running in **lite mode only** — SQLite-backed jobs and in-process workers. The legacy Postgres / Redis / AMQP / OAuth surfaces described in older revisions of this document have been removed; if you find references to them in other files they are stale.

**In scope:**

- SSRF via user-supplied URLs (CLI args, MCP tool calls, sitemap/discovered URLs)
- DNS rebinding against the in-process HTTP client
- Secret leakage through commits, logs, and `Debug` impls
- MCP HTTP transport authentication and origin/host validation
- Local admin web panel access control
- Heap exposure from the optional ask full-document cache in long-lived
  `serve`/`mcp` processes

**Out of scope:**

- Host kernel compromise
- Multi-tenant isolation — Axon is designed for trusted self-hosted operation
- Hardening of the upstream services Axon talks to (Qdrant, TEI, Gemini headless LLM)
- Supply-chain integrity beyond pinned crate versions

---

## 2. SSRF and URL Validation

### 2.1 `validate_url()`

Source: `crates/core/http/ssrf.rs:64`.

`validate_url(&str) -> Result<(), HttpError>` is the parse-time SSRF guard. It rejects:

| Category | Examples |
|----------|----------|
| Non-HTTP schemes | `file://`, `gopher://`, `ftp://`, `javascript:` |
| Loopback hosts | `localhost`, `*.localhost` |
| Reserved TLDs | `*.internal`, `*.local` |
| Loopback IPs | `127.0.0.0/8`, `::1`, `0.0.0.0/8` |
| Link-local | `169.254.0.0/16`, `fe80::/10` |
| RFC 1918 private | `10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16` |
| IPv6 unique-local | `fc00::/7` |
| IPv4-mapped IPv6 | `::ffff:127.0.0.1`, `::ffff:10.x.x.x` (recursed into the v4 checks) |

Implementation note: hosts are parsed with `host_str().parse::<IpAddr>()`, **not** `spider::url::Host::Ipv4/Ipv6` — the spider variants silently miss IPv6 (confirmed production bug, see `crates/core/CLAUDE.md`).

### 2.2 Call sites

`validate_url()` is invoked at every external entry point that accepts a URL. As of this writing, callers include (`crates/core/http/client.rs:46,70` and):

- `crates/cli/commands/scrape.rs`, `crawl.rs`, `screenshot.rs`
- `crates/services/scrape.rs`, `map.rs`, `screenshot.rs`
- `crates/crawl/engine/map.rs`, `engine/sitemap.rs`, `scrape.rs`, `screenshot.rs`
- `crates/jobs/lite/workers/runners/crawl.rs`, `crates/jobs/crawl.rs`
- `crates/ingest/youtube.rs`, `ingest/github/files/clone.rs`, `ingest/github/wiki.rs`
- `crates/mcp/server/common.rs`
- `crates/core/content/engine.rs`

The reqwest redirect policy also re-validates every redirect target (`crates/core/http/client.rs:44-53`). A 30x to a blocked URL becomes `PermissionDenied` instead of a follow.

### 2.3 DNS rebinding (TOCTOU) — mitigated

`validate_url()` only checks literal hostnames and IPs. The connect-time TOCTOU window is closed by `SsrfBlockingResolver` (`crates/core/http/ssrf.rs:174-205`), wired into the reqwest client via `ClientBuilder::dns_resolver()` in production builds. The resolver runs `check_ip()` on every IP returned by the OS resolver at the moment reqwest dials. A TTL-0 record that flips to `127.0.0.1` after `validate_url()` returns is rejected before the connection is made.

Test builds (`#[cfg(test)]`) skip the custom resolver so `httpmock` servers on `127.0.0.1` remain reachable; `validate_url()` itself still blocks loopback unless a thread-local `ALLOW_LOOPBACK` flag is set inside the test.

### 2.4 Defence-in-depth blacklist

`ssrf_blacklist_patterns()` (`crates/core/http/ssrf.rs:144`) returns 12 regex patterns covering loopback, link-local, RFC-1918, and IPv6 private ranges. These are passed to `spider`'s `with_blacklist_url()` so URLs **discovered during crawl** (sitemaps, link extraction) are dropped before fetch — even if the seed URL was public, a crawler cannot follow a same-page link to `http://127.0.0.1/admin`.

---

## 3. HTTP Client Safety

Source: `crates/core/http/client.rs`.

- Production builds use a single shared `LazyLock<reqwest::Client>` (`HTTP_CLIENT`), constructed once with a 30-second timeout and the SSRF-blocking DNS resolver. **Never construct `reqwest::Client::new()` per call** — that bypasses the resolver and exhausts sockets under load.
- The redirect policy re-validates every hop with `validate_url()` (`client.rs:44`).
- `fetch_html()` validates the final URL before issuing the request (`client.rs:70`).
- Test builds get a fresh leaked `reqwest::Client` per call to avoid cross-runtime "dispatch task is gone" failures and to keep `httpmock` working.

The shared user-agent honours `AXON_CHROME_USER_AGENT` if set.

---

## 4. MCP HTTP Authentication

The MCP server (`axon mcp`) supports `stdio`, `http`, and `both` transports. **Stdio is unauthenticated** and relies on OS process boundaries — the MCP client owns the process lifecycle. HTTP is gated by a single static bearer token.

Sources: `crates/mcp/auth.rs`, `crates/mcp/server/http.rs`.

### 4.1 Token enforcement

- Configured via `AXON_MCP_HTTP_TOKEN` (single shared secret).
- Accepted on either header:
  - `Authorization: Bearer <token>`
  - `x-api-key: <token>`
- Compared in constant time via `subtle::ConstantTimeEq` (`auth.rs:21-23`) to defeat timing oracles.
- Empty / whitespace-only tokens are treated as unset (`auth.rs:47-52`).

### 4.2 Startup policy

`enforce_mcp_http_startup_policy` (`server/http.rs:154-171`) runs before the listener binds:

| Bind host | Token configured | Behaviour |
|-----------|------------------|-----------|
| Loopback (`127.0.0.1`, `::1`, `localhost`) | yes | start, auth required |
| Loopback | no | start, log a warning, requests pass through |
| Non-loopback (`0.0.0.0`, public DNS) | yes | start, auth required |
| Non-loopback | no | **refuse to start** with a clear error |

This means a forgotten token on a public bind fails closed instead of running unauthenticated.

### 4.3 What is not implemented

There is **no OAuth broker, no Google sign-in, no DCR, no `atk_` token issuance, and no Redis-backed token cache**. Earlier docs referenced these surfaces; they were never shipped. See `docs/auth/MCP-AUTH.md` for the canonical, code-aligned auth flow.

---

## 5. Host and CORS Allowlists

The MCP HTTP server stacks two additional middlewares around `mcp_auth_middleware`:

### 5.1 Host validation

Source: `crates/web/security.rs` (used by `crates/mcp/server/http.rs:23-26`).

`HostAllowlist` accepts:

- `127.0.0.1:<port>`, `localhost:<port>`, `[::1]:<port>`
- The configured bind host on its port
- Every entry in `AXON_MCP_ALLOWED_ORIGINS` (origin → authority via `Uri::authority()`)

Requests with a `Host` header outside that set return `403 forbidden: host not allowed`. Missing `Host` returns `400`.

### 5.2 CORS

Source: `crates/mcp/cors.rs` (mounted by `server/http.rs:148-151`).

- Allowlist driven by `AXON_MCP_ALLOWED_ORIGINS` (comma-separated). Unset = strict default (only same-origin / loopback). Non-browser tools (curl, MCP SDKs) are unaffected because they do not send `Origin`.
- Preflight `OPTIONS` requests with a disallowed origin return `403`.
- `Access-Control-Allow-Headers` is the **static** list `authorization, content-type, x-api-key`. The middleware never reflects the client-supplied `Access-Control-Request-Headers` value, which would grant an effective wildcard (CWE-942).

---

## 6. Web Admin Panel

Source: `crates/web/auth.rs`, `crates/web/server.rs`.

`apps/web` (`@axon/admin-panel`) is an admin-only setup/config UI mounted by `axon serve`. It is **not** a public-facing application.

- On first start, `init_panel_password()` (`auth.rs:33`) generates a 32-byte URL-safe password, writes it to `~/.axon/panel-password` with mode `0600` and `O_NOFOLLOW`, and prints it once to stderr. Existing files are reused.
- `/api/panel/login` accepts the password and returns it back to the caller as a session token. `/api/panel/state` is unauthenticated (returns only `setup_required` + the config path).
- All other `/api/panel/*` routes require `Authorization: Bearer <token>` or `x-axon-panel-token: <token>`, verified in constant time via `PanelPassword::verify` (`auth.rs:21-26`).
- Routes exposed: `state` (GET), `login` (POST), `config` (GET/PUT), `ops` (GET), `setup/targets` (GET), `setup/deploy` (POST). There is no shell endpoint, no WebSocket, no download route, no `/output/*` route in the current code.

Recommendations:

- Bind the unified `axon serve` to `127.0.0.1` unless you intend to expose the panel externally.
- If exposing externally, terminate TLS and add a reverse-proxy auth layer in front — the panel password is meant for local administration.

---

## 7. Secrets Handling

### 7.1 `.env` is the only secret store

- Service URLs and credentials live in `.env` (gitignored). `.env.example` is the tracked template.
- `~/.axon/config.toml` is for **non-secret** tuning knobs only (search params, worker limits). The loader treats unknown fields as fatal so accidentally pasting a secret there fails fast (`crates/core/config/parse/toml_config.rs`).
- The MCP HTTP token is `AXON_MCP_HTTP_TOKEN`. No other static auth tokens exist in the current binary.

### 7.2 `Debug` redaction

`Config`'s `fmt::Debug` impl (`crates/core/config/types/config_impls.rs:203-369`) redacts:

- `github_token`
- `reddit_client_id`, `reddit_client_secret`
- `openai_api_key`
- `tavily_api_key`
- `custom_headers` — values redacted, header names preserved as `"Name: [REDACTED]"`; malformed entries become `"[MALFORMED]"`

Do **not** add new secret fields without extending this impl. The compiler will not warn you.

### 7.3 Logging hygiene

- Library code uses `log_info` / `log_warn` / `log_done` from `crates/core/logging.rs`. Never `println!` from a library — it bypasses log targets and rotation.
- `redact_url()` in `crates/core/content.rs` strips `username:password@` from URLs before logging.
- The MCP server returns deterministic error messages and never echoes secret env values back to callers.

### 7.4 Ask full-document cache

The optional `[ask.cache]` cache stores full-document Qdrant chunks in the
process heap for the ask retrieval path. Cached values include `chunk_text`;
logs deliberately omit that text and only use source identifiers and counters.

The cache is disabled by default and is useful only in long-lived `axon serve`
or `axon mcp` processes. When enabled for those modes, startup enforces
`RLIMIT_CORE=0` before initializing the daemon so a crash does not write cached
source text to a core file. This guard does not encrypt heap memory and does
not protect against a compromised process; it only removes the core-dump leak
path.

---

## 8. Network Exposure

| Service | Default bind | Notes |
|---------|--------------|-------|
| `axon mcp` (HTTP) | `127.0.0.1:8001` | Non-loopback bind requires `AXON_MCP_HTTP_TOKEN` (startup policy). |
| `axon serve` (unified web + MCP) | `127.0.0.1:49000` | Same MCP token policy applies. |
| `axon-qdrant` (compose) | `127.0.0.1:53333`, `:53334` | Loopback in `config/docker-compose.services.yaml`. |
| `axon-tei` (compose) | `127.0.0.1:52000` | Loopback. |
| `axon-chrome` (compose) | `127.0.0.1:6000`, `:9222`, `:9223` | Loopback. Ports: 6000 = `headless_browser` management API, 9222 = CDP proxy, 9223 = raw Chrome DevTools. **All three are unauthenticated control planes** and rely on the loopback bind for access control. |

Hardening guidance:

- Keep infra services loopback-bound. The compose file already does this; the `127.0.0.1:` prefix on every Chrome port mapping is intentional security posture, not a bug.
- For the MCP server on a non-loopback host, set a long random `AXON_MCP_HTTP_TOKEN` (`openssl rand -hex 32`).
- Never expose Qdrant or Chrome's CDP / management ports to a network. The upstream `headless_browser` and Chrome DevTools Protocol have **no built-in authentication** — anyone who can reach 6000/9222/9223 can run arbitrary JS, navigate to internal URLs, exfiltrate cookies from any origin Chrome has visited, and (via `Page.navigate` on `file://` URLs) read local files inside the container.

Cross-host deployments (`crates/services/setup/deploy.rs` / `axon serve` setup wizard):

- The setup wizard can write a non-loopback `chrome_remote_url` into `~/.axon/config.toml` for clients running on a different machine than `axon-chrome`. **If you do this, you own the auth boundary** — front the Chrome ports with an authenticated reverse proxy, an SSH tunnel, a WireGuard mesh, or equivalent. Axon does not add a token to the CDP/management endpoints because those endpoints are owned by upstream crates we do not control.
- The defense-in-depth `validate_url()` SSRF guard still runs on every URL handed to Chrome via spider (`screenshot`, `extract`, `crawl`, `map`, `scrape`), so an attacker who tricks axon into asking Chrome to fetch `http://127.0.0.1:54321/admin` is blocked at the axon layer regardless of where Chrome is hosted.

---

## 9. Operational Checklist

Before deploy:

1. `.env` exists, is not committed, and contains every required secret.
2. `git diff -- . ':!*.lock'` shows no secret material in the changeset.
3. For history scans, run a dedicated tool (`gitleaks detect --source=. --log-opts="HEAD~50..HEAD"` or similar). `git diff` only sees uncommitted changes.
4. `AXON_MCP_HTTP_TOKEN` is set if `AXON_MCP_HTTP_HOST` is anything other than `127.0.0.1` / `localhost` / `::1`.
5. `~/.axon/panel-password` exists and is mode `0600` if `axon serve` will run.
6. `./scripts/axon doctor` reports Qdrant and TEI healthy.

After deploy:

1. Containers report healthy.
2. `curl http://<host>:8001/mcp` (no auth) returns `401` when the token is configured.
3. `curl -H "Authorization: Bearer <wrong>" http://<host>:8001/mcp` returns `401`.
4. Logs do not show repeated `web: rejected request with disallowed Host header` (indicates a misconfigured allowlist) or token-auth failures from your own clients.

---

## 10. Source Map

- `crates/core/http/ssrf.rs` — `validate_url()`, `check_ip()`, `ssrf_blacklist_patterns()`, `SsrfBlockingResolver`
- `crates/core/http/client.rs` — `HTTP_CLIENT` singleton, redirect-time SSRF re-validation, `fetch_html()`
- `crates/core/http/normalize.rs` — `normalize_url()` (scheme prepend)
- `crates/core/config/types/config_impls.rs` — `Config::Debug` redaction
- `crates/mcp/auth.rs` — `mcp_auth_middleware`, `mcp_http_token_is_configured`
- `crates/mcp/server/http.rs` — startup policy, host allowlist + CORS wiring, unified router
- `crates/mcp/cors.rs` — CORS middleware (static `Allow-Headers`, no reflection)
- `crates/web/security.rs` — `HostAllowlist`, `host_validation_middleware`
- `crates/web/auth.rs` — admin panel password generation and constant-time verify
- `crates/web/server.rs` — admin panel routes and authorization helper
- `docs/auth/MCP-AUTH.md` — canonical MCP HTTP auth reference

> The legacy `docs/auth/API-TOKEN.md` describes a `/ws`, `/output/*`, `/download/*`, and `AXON_WEB_API_TOKEN`-gated surface that does not exist in the current binary. Treat that file as historical until it is rewritten or removed.
