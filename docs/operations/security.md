# Security Model
Last Modified: 2026-06-01

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

This document captures the security controls present in the Axon code base today. Axon is a single-binary Rust application (`axon`) with SQLite-backed jobs and in-process workers. The legacy Postgres / Redis / AMQP runtime has been removed. MCP HTTP auth supports static bearer mode and Google OAuth/JWT mode through lab-auth.

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

Source: `src/core/http/ssrf.rs:64`.

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

Implementation note: hosts are parsed with `host_str().parse::<IpAddr>()`, **not** `spider::url::Host::Ipv4/Ipv6` — the spider variants silently miss IPv6 (confirmed production bug, see `src/core/CLAUDE.md`).

### 2.2 Call sites

`validate_url()` is invoked at every external entry point that accepts a URL. As of this writing, callers include (`src/core/http/client.rs:46,70` and):

- `src/cli/commands/scrape.rs`, `crawl.rs`, `screenshot.rs`
- `src/services/scrape.rs`, `map.rs`, `screenshot.rs`
- `src/crawl/engine/map.rs`, `engine/sitemap.rs`, `scrape.rs`, `screenshot.rs`
- `src/jobs/workers/runners/crawl.rs`, `src/jobs/crawl.rs`
- `src/ingest/youtube.rs`, `ingest/github/files/clone.rs`, `ingest/github/wiki.rs`
- `src/mcp/server/common.rs`
- `src/core/content/engine.rs`

The reqwest redirect policy also re-validates every redirect target (`src/core/http/client.rs:44-53`). A 30x to a blocked URL becomes `PermissionDenied` instead of a follow. The scrape direct-fetch fallback uses the same SSRF-guarded client builder, then layers its per-request timeout, TLS, proxy, user-agent, and header options on top without process-global reuse.

### 2.3 DNS rebinding (TOCTOU) — mitigated

`validate_url()` only checks literal hostnames and IPs. The connect-time TOCTOU window is closed by `SsrfBlockingResolver` (`src/core/http/ssrf.rs:174-205`), wired into the reqwest client via `ClientBuilder::dns_resolver()` in production builds. The resolver runs `check_ip()` on every IP returned by the OS resolver at the moment reqwest dials. A TTL-0 record that flips to `127.0.0.1` after `validate_url()` returns is rejected before the connection is made.

Test builds (`#[cfg(test)]`) skip the custom resolver so `httpmock` servers on `127.0.0.1` remain reachable; `validate_url()` itself still blocks loopback unless a thread-local `ALLOW_LOOPBACK` flag is set inside the test.

### 2.4 Defence-in-depth blacklist

`ssrf_blacklist_patterns()` (`src/core/http/ssrf.rs:144`) returns 12 regex patterns covering loopback, link-local, RFC-1918, and IPv6 private ranges. These are passed to `spider`'s `with_blacklist_url()` so URLs **discovered during crawl** (sitemaps, link extraction) are dropped before fetch — even if the seed URL was public, a crawler cannot follow a same-page link to `http://127.0.0.1/admin`.

---

## 3. HTTP Client Safety

Source: `src/core/http/client.rs`.

- Production builds use a single shared `LazyLock<reqwest::Client>` (`HTTP_CLIENT`), constructed once with a 30-second timeout and the SSRF-blocking DNS resolver. Request paths that need custom client settings must use the shared builder helper that installs the same resolver before layering additional options. **Never construct `reqwest::Client::new()` per call** — that bypasses the resolver and exhausts sockets under load.
- The redirect policy re-validates every hop with `validate_url()` (`client.rs:44`).
- `fetch_html()` validates the final URL before issuing the request (`client.rs:70`).
- Test builds get a fresh leaked `reqwest::Client` per call to avoid cross-runtime "dispatch task is gone" failures and to keep `httpmock` working. The scrape direct-fetch fallback also builds a fresh client per call so headers, proxy, and TLS settings from one config cannot leak into another scrape.

The shared User-Agent resolves in priority order: `AXON_USER_AGENT` → `AXON_CHROME_USER_AGENT` → built-in Firefox browser UA (`DEFAULT_UA` in `src/core/http/ua.rs`). All HTTP paths — the `http_client()` singleton, Spider crawl/scrape/screenshot paths, and vertical extractors — use this resolved value consistently. Reddit ingestion uses its own OAuth-format UA regardless of these settings.

---

## 4. MCP HTTP Authentication

The MCP server (`axon mcp`) supports `stdio`, `http`, and `both` transports. **Stdio is unauthenticated** and relies on OS process boundaries — the MCP client owns the process lifecycle. HTTP is gated by static bearer auth, OAuth, or loopback-only dev mode.

Sources: `src/mcp/auth.rs`, `src/mcp/server/http.rs`.

### 4.1 Auth policies

| Policy | How selected | Behavior |
|--------|--------------|----------|
| OAuth/JWT | `AXON_MCP_AUTH_MODE=oauth` | Builds `lab_auth::state::AuthState`, mounts OAuth routes, validates JWT bearer tokens, and enforces `axon:read` / `axon:write` scopes. |
| Bearer-only | default mode with `AXON_MCP_HTTP_TOKEN` set | Validates a static token with `lab_auth::AuthLayer::with_static_token`; static tokens receive both read and write scopes. |
| Loopback dev | default mode, no token, loopback bind | No auth layer; loopback bind is the trust boundary. |

Static bearer tokens are accepted on either header:
- `Authorization: Bearer <token>`
- `x-api-key: <token>` (normalized to bearer before lab-auth sees the request)

Empty or whitespace-only `AXON_MCP_HTTP_TOKEN` values are treated as unset.

### 4.2 OAuth mode

When `AXON_MCP_AUTH_MODE=oauth`, `src/mcp/auth.rs` initializes lab-auth with
Google OAuth, JWKS/JWT validation, dynamic client registration, and OAuth
metadata routes. `AXON_MCP_PUBLIC_URL`, Google client credentials, and admin
email are required. `AXON_MCP_HTTP_TOKEN` remains valid in dual mode when set.

OAuth mode reads `AXON_MCP_PUBLIC_URL`, `AXON_MCP_GOOGLE_CLIENT_ID`,
`AXON_MCP_GOOGLE_CLIENT_SECRET`, `AXON_MCP_AUTH_ADMIN_EMAIL`, and
`AXON_MCP_AUTH_ALLOWED_REDIRECT_URIS`. The OAuth router exposes
`/.well-known/oauth-authorization-server`, `/.well-known/oauth-protected-resource`,
`/jwks`, `/authorize`, `/auth/google/callback`, `/token`, and `/register`.
The account configured as `AXON_MCP_AUTH_ADMIN_EMAIL` is always granted the full
configured Axon OAuth scope set (`axon:read axon:write`) after Google email
verification. Other allowlisted users receive only the scope approved for their
authorization request.

### 4.3 Startup policy

`build_auth_policy` (`src/mcp/auth.rs`) runs before the listener binds:

| Bind host | Auth configured | Behaviour |
|-----------|-----------------|-----------|
| Stdio transport | any | loopback/process-boundary policy; OAuth config is ignored with a warning |
| Loopback (`127.0.0.1`, `::1`, `localhost`) | OAuth or static token | start, auth required |
| Loopback | none | start, log a warning, requests pass through |
| Non-loopback (`0.0.0.0`, public DNS) | OAuth or static token | start, auth required |
| Non-loopback | none | **refuse to start** with a clear error |

This means a forgotten token on a public bind fails closed instead of running unauthenticated.

### 4.4 Scope enforcement

Mounted auth inserts `lab_auth::AuthContext` into request extensions. `src/mcp/server.rs` maps each tool action to a minimum scope:

- `axon:write`: mutating operations such as `crawl`, `extract`, `embed`, `ingest`, `scrape`, and artifact deletion/cleanup.
- `axon:read`: query, search, retrieval, status, ask/research/evaluate, screenshots, and read-only artifact operations.
- `axon:write` satisfies `axon:read`; unknown actions fail closed.

See `docs/operations/auth/mcp-auth.md` for the canonical, code-aligned auth flow.

---

## 5. Host and CORS Allowlists

The MCP HTTP server stacks host and CORS middleware around the MCP router. When `AuthPolicy` is mounted, `lab_auth::AuthLayer` protects `/mcp`; OAuth routes are mounted beside it and remain unauthenticated so the OAuth flow can start.

### 5.1 Host validation

Source: `src/web/security.rs` (used by `src/mcp/server/http.rs:23-26`).

`HostAllowlist` accepts:

- `127.0.0.1:<port>`, `localhost:<port>`, `[::1]:<port>`
- The configured bind host on its port
- Every entry in `AXON_MCP_ALLOWED_ORIGINS` (origin → authority via `Uri::authority()`)

Requests with a `Host` header outside that set return `403 forbidden: host not allowed`. Missing `Host` returns `400`.

### 5.2 CORS

Source: `src/mcp/cors.rs` (mounted by `server/http.rs:148-151`).

- Allowlist driven by `AXON_MCP_ALLOWED_ORIGINS` (comma-separated). Unset = strict default (only same-origin / loopback). Non-browser tools (curl, MCP SDKs) are unaffected because they do not send `Origin`.
- Preflight `OPTIONS` requests with a disallowed origin return `403`.
- `Access-Control-Allow-Headers` is the **static** list `authorization, content-type, x-api-key`. The middleware never reflects the client-supplied `Access-Control-Request-Headers` value, which would grant an effective wildcard (CWE-942).

---

## 6. Web Admin Panel

Source: `src/web/auth.rs`, `src/web/server.rs`.

`apps/web` (`@axon/admin-panel`) is an admin-only setup/config UI mounted by `axon serve`. It is **not** a public-facing application.

- On first start, `init_panel_password()` (`auth.rs:33`) generates a 32-byte URL-safe password, writes it to `~/.axon/panel-password` with mode `0600` and `O_NOFOLLOW`, and prints it once to stderr. Existing files are reused.
- `/api/panel/login` accepts the password and returns it back to the caller as a session token. `/api/panel/state` is unauthenticated (returns only `setup_required` + the config path).
- All other `/api/panel/*` routes require `Authorization: Bearer <token>` or `x-axon-panel-token: <token>`, verified in constant time via `PanelPassword::verify` (`auth.rs:21-26`).
- Routes exposed (see `src/web/server/routing.rs` and `src/web/CLAUDE.md`): `state` (GET), `login` (POST), `config` (GET/PUT), `env` (GET/PUT), `status` (GET), `doctor` (GET), `command` (POST), `ops` (GET), `stack` (GET), `first-run/crawl` (POST), `first-run/ask` (POST), `setup/targets` (GET).
- `/api/panel/command` is a **command runner**, not a raw shell: `panel_command` (`handlers/config.rs:179`) parses only a fixed `ask`-or-action grammar (`parse_panel_command`) and dispatches through the same `services`/action layer the CLI uses. It is authenticated like every other write route. There is no arbitrary-shell endpoint, no WebSocket, no download route, and no `/output/*` route in the current code.

Recommendations:

- Bind the unified `axon serve` to `127.0.0.1` unless you intend to expose the panel externally.
- If exposing externally, terminate TLS and add a reverse-proxy auth layer in front — the panel password is meant for local administration.

---

## 7. Secrets Handling

### 7.1 `~/.axon/.env` is the only secret store

- Service URLs and credentials live in `~/.axon/.env`. Repo-local `.env` is a gitignored development fallback only. `.env.example` is the tracked template.
- `~/.axon/config.toml` is for **non-secret** tuning knobs only (search params, worker limits). The loader treats unknown fields as fatal so accidentally pasting a secret there fails fast (`src/core/config/parse/toml_config.rs`).
- The MCP HTTP static token is `AXON_MCP_HTTP_TOKEN`; OAuth/JWT mode is configured with the `AXON_MCP_*` auth variables documented above.

### 7.2 `Debug` redaction

`Config`'s `fmt::Debug` impl (`src/core/config/types/config_impls.rs:203-369`) redacts:

- `github_token`
- `reddit_client_id`, `reddit_client_secret`
- `tavily_api_key`
- `custom_headers` — values redacted, header names preserved as `"Name: [REDACTED]"`; malformed entries become `"[MALFORMED]"`

Do **not** add new secret fields without extending this impl. The compiler will not warn you.

### 7.3 Logging hygiene

- Library code uses `log_info` / `log_warn` / `log_done` from `src/core/logging.rs`. Never `println!` from a library — it bypasses log targets and rotation.
- `redact_url()` in `src/core/content.rs` strips `username:password@` from URLs before logging.
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

| Service | Compose publish (`docker-compose.prod.yaml`) | Notes |
|---------|----------------------------------------------|-------|
| `axon mcp` / `axon serve` / Compose `axon` (HTTP) | `${AXON_MCP_HTTP_PUBLISH:-8001}:8001` | Host mapping publishes on all interfaces by default, but the in-container axon process binds `AXON_MCP_HTTP_HOST` (default `127.0.0.1`), so the port is not actually reachable until you set `AXON_MCP_HTTP_HOST=0.0.0.0`. A non-loopback process bind requires bearer or OAuth auth — the startup policy refuses to start a tokenless non-loopback bind. |
| `axon-qdrant` (compose) | `53333:6333`, `53334:6334` | **Published on all interfaces.** No `127.0.0.1:` prefix. |
| `axon-tei` (compose) | `${TEI_HTTP_PORT:-52000}:80` | **Published on all interfaces.** |
| `axon-chrome` (compose) | `6000:6000`, `9222:9222`, `9223:9223` | **Published on all interfaces.** Ports: 6000 = `headless_browser` management API, 9222 = CDP proxy, 9223 = raw Chrome DevTools. **All three are unauthenticated control planes** with no built-in access control. |

> **Caution: the compose files publish these ports on all interfaces.** Every
> port mapping in `docker-compose.prod.yaml` (and the `docker-compose.yaml` dev
> stack, which `extends` it) uses a **bare** `host:container` mapping with no
> `127.0.0.1:` prefix — and the pre-commit check `scripts/check_compose_port_bindings.py`
> **forbids** adding such a prefix. The host-port mapping is therefore NOT the
> access boundary. The remaining protections are (a) the in-container process
> bind — `axon` binds `AXON_MCP_HTTP_HOST` (default `127.0.0.1`), so its port is
> unreachable unless opened deliberately — and (b) whatever host firewall /
> private network you place around the stack. Qdrant, TEI (`axon-tei:80`), and the
> unauthenticated Chrome control planes listen on
> `0.0.0.0` inside their containers, so on a host reachable from an untrusted
> network their published ports ARE exposed. Restrict them with a host firewall or
> a private Docker network — do not rely on the compose file to loopback-bind them.

Hardening guidance:

- Do not run this stack as-is on a host with untrusted network reachability. Front the infra ports with a host firewall or a private/internal Docker network before exposing the host. (Do **not** add `127.0.0.1:` prefixes to the compose mappings — the repo lint rejects them.)
- For the MCP server on a non-loopback host, set a long random `AXON_MCP_HTTP_TOKEN` (`openssl rand -hex 32`) or configure `AXON_MCP_AUTH_MODE=oauth`.
- Never expose Qdrant or Chrome's CDP / management ports to a network. The upstream `headless_browser` and Chrome DevTools Protocol have **no built-in authentication** — anyone who can reach 6000/9222/9223 can run arbitrary JS, navigate to internal URLs, exfiltrate cookies from any origin Chrome has visited, and (via `Page.navigate` on `file://` URLs) read local files inside the container.

Cross-host deployments (operator-managed):

- If you point `chrome_remote_url` in `~/.axon/config.toml` at a non-loopback host (for clients running on a different machine than `axon-chrome`), **you own the auth boundary** — front the Chrome ports with an authenticated reverse proxy, an SSH tunnel, a WireGuard mesh, or equivalent. Axon does not add a token to the CDP/management endpoints because those endpoints are owned by upstream crates we do not control.
- The defense-in-depth `validate_url()` SSRF guard still runs on every URL handed to Chrome via spider (`screenshot`, `extract`, `crawl`, `map`, `scrape`), so an attacker who tricks axon into asking Chrome to fetch `http://127.0.0.1:54321/admin` is blocked at the axon layer regardless of where Chrome is hosted.

---

## 9. Operational Checklist

Before deploy:

1. `~/.axon/.env` exists and contains every required secret. Repo-local `.env`, if present for development fallback, is not committed.
2. `git diff -- . ':!*.lock'` shows no secret material in the changeset.
3. For history scans, run a dedicated tool (`gitleaks detect --source=. --log-opts="HEAD~50..HEAD"` or similar). `git diff` only sees uncommitted changes.
4. `AXON_MCP_HTTP_TOKEN` or OAuth mode is configured if `AXON_MCP_HTTP_HOST` is anything other than `127.0.0.1` / `localhost` / `::1`.
5. `~/.axon/panel-password` exists and is mode `0600` if `axon serve` will run.
6. `./scripts/axon doctor` reports Qdrant and TEI healthy.

After deploy:

1. Containers report healthy.
2. `curl http://<host>:8001/mcp` (no auth) returns `401` when the token is configured.
3. `curl -H "Authorization: Bearer <wrong>" http://<host>:8001/mcp` returns `401`.
4. Logs do not show repeated `web: rejected request with disallowed Host header` (indicates a misconfigured allowlist) or token-auth failures from your own clients.

---

## 10. Source Map

- `src/core/http/ssrf.rs` — `validate_url()`, `check_ip()`, `ssrf_blacklist_patterns()`, `SsrfBlockingResolver`
- `src/core/http/client.rs` — `HTTP_CLIENT` singleton, redirect-time SSRF re-validation, `fetch_html()`
- `src/core/http/normalize.rs` — `normalize_url()` (scheme prepend)
- `src/core/config/types/config_impls.rs` — `Config::Debug` redaction
- `src/mcp/auth.rs` — `AuthPolicy`, `build_auth_policy`, static bearer helpers, OAuth/JWT policy wiring
- `src/mcp/server/http.rs` — startup policy, host allowlist + CORS wiring, unified router
- `src/mcp/cors.rs` — CORS middleware (static `Allow-Headers`, no reflection)
- `src/web/security.rs` — `HostAllowlist`, `host_validation_middleware`
- `src/web/auth.rs` — admin panel password generation and constant-time verify
- `src/web/server.rs` — admin panel routes and authorization helper
- `docs/operations/auth/mcp-auth.md` — canonical MCP HTTP auth reference
