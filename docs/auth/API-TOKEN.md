# API Tokens
Last Modified: 2026-05-06

This is the **index** of every authentication secret recognised by the Axon
runtime. Each section below documents one token: where it lives, how it is
created, when it is required, and what fails without it.

> Earlier revisions of this document referenced a rich web surface
> (`/ws`, `/output/*`, `/download/*`, `AXON_WEB_API_TOKEN`,
> `AXON_WEB_BROWSER_API_TOKEN`, `NEXT_PUBLIC_AXON_API_TOKEN`,
> `AXON_SHELL_WS_TOKEN`, `AXON_WEB_ALLOW_INSECURE_DEV`). **None of those
> exist any more.** The Next.js web app and shell WS server were removed
> when the runtime collapsed onto the unified `axon serve` panel. Search
> the current code base for any of those names — you will find none.

## Quick Map

| Token | Type | Surface | Required? | Section |
|-------|------|---------|-----------|---------|
| `AXON_MCP_HTTP_TOKEN` | Env-set bearer | `axon mcp --transport http` (`/mcp`) | Loopback: optional. Non-loopback: yes. | [MCP HTTP token](#mcp-http-token) |
| MCP OAuth env | Google OAuth + JWT | `/mcp`, `/v1/actions` | OAuth mode only | [MCP OAuth](#mcp-oauth) |
| Web panel password | Auto-generated, file-backed | `axon serve` web panel (`/api/panel/*`) | Always (no anonymous access) | [Web panel password](#web-panel-password) |

User-supplied third-party credentials (Tavily, OpenAI, GitHub, Reddit) are
**not** axon-issued tokens — see [Third-party credentials](#third-party-credentials)
for how they differ.

---

## MCP HTTP token

**Variable:** `AXON_MCP_HTTP_TOKEN`
**Source:** `src/mcp/auth.rs`, `src/mcp/server/http.rs`
**Detailed reference:** [`docs/auth/MCP-AUTH.md`](MCP-AUTH.md)

Bearer token gating the MCP HTTP transport. Applied only to
`axon mcp --transport http` (or `--transport both`) and the unified
`axon serve` MCP HTTP route at `/mcp`. The stdio transport has no network
listener and performs no auth.

| Property | Value |
|----------|-------|
| Storage | Process environment (`.env`, shell export, container env) |
| Format | Arbitrary opaque secret (recommended: `openssl rand -hex 32`) |
| Headers accepted | `Authorization: Bearer <token>` or `x-api-key: <token>` |
| Comparison | Constant-time (`subtle::ConstantTimeEq`) |
| Loopback bind (`127.0.0.1`, `::1`, `localhost`) | Optional — server starts and emits a one-time warning when unset |
| Non-loopback bind (`0.0.0.0`, public hostname) | Required unless OAuth mode is configured |
| Failure mode | `401 Unauthorized` on each request when token is set but request omits / mismatches header |

### Setting it

```bash
# .env
AXON_MCP_HTTP_TOKEN=$(openssl rand -hex 32)
AXON_MCP_HTTP_HOST=127.0.0.1
AXON_MCP_HTTP_PORT=8001
```

For client configuration (Claude Code, mcporter, raw `curl`) and the full
security model, see [`docs/auth/MCP-AUTH.md`](MCP-AUTH.md).

---

---

## MCP OAuth

**Variables:** `AXON_MCP_AUTH_MODE`, `AXON_MCP_PUBLIC_URL`,
`AXON_MCP_GOOGLE_CLIENT_ID`, `AXON_MCP_GOOGLE_CLIENT_SECRET`,
`AXON_MCP_AUTH_ADMIN_EMAIL`, `AXON_MCP_AUTH_ALLOWED_REDIRECT_URIS`
**Source:** `src/mcp/auth.rs`, `src/mcp/server/http.rs`
**Detailed reference:** [`docs/auth/MCP-AUTH.md`](MCP-AUTH.md)

OAuth mode is enabled with `AXON_MCP_AUTH_MODE=oauth`. It mounts lab-auth OAuth
metadata, Google login, token, JWKS, and dynamic registration routes beside
`/mcp`. Static bearer auth remains accepted when `AXON_MCP_HTTP_TOKEN` is also
set.

---

## Web panel password

**Source:** `src/web/auth.rs`, `src/web/server.rs`
**File:** `~/.axon/panel-password` (mode `0600`, owner-only)

Single shared password gating the `axon serve` admin panel. There are no
user accounts and no signup flow; everyone with the file's contents has
the same access.

| Property | Value |
|----------|-------|
| Storage | `~/.axon/panel-password` (plaintext, mode `0600`, written with `O_NOFOLLOW`) |
| Format | 32 random bytes, URL-safe base64 (no padding) |
| Generated | Automatically on first start of `axon serve` if the file does not exist |
| Persistence | Reused on subsequent starts; never rotated automatically |
| Surface gated | `/api/panel/config`, `/api/panel/ops`, `/api/panel/setup/targets` |
| Headers accepted | `Authorization: Bearer <password>` or `x-axon-panel-token: <password>` |
| Comparison | Constant-time (`subtle::ConstantTimeEq`) |
| Login flow | `POST /api/panel/login` returns the same string back when the supplied password matches; the UI then sends it on subsequent requests |
| Failure mode | `401 Unauthorized` on `/api/panel/config|ops|setup/*` |
| Static assets and `/api/panel/state`/`/login` | Unauthenticated — needed to bootstrap the login page |

### First-run behaviour

When `axon serve` generates a new password it logs it to stderr **once**:

```
Axon web panel password: <token>
Open: http://127.0.0.1:8001
```

If you miss the line, copy it back:

```bash
cat ~/.axon/panel-password
```

### Rotating

Delete the file, restart `axon serve`, copy the new password from stderr.
There is no in-product rotation API.

```bash
rm ~/.axon/panel-password
axon serve
# Axon web panel password: <new-token>
```

---

## Third-party credentials

These are **not** axon-issued tokens. They are credentials you obtain
from a third-party provider and supply via environment variables so that
Axon can call the upstream API on your behalf. They are listed here only
to disambiguate them from Axon-issued auth secrets above.

| Variable | Provider | Used by |
|----------|----------|---------|
| Gemini CLI auth / `AXON_HEADLESS_GEMINI_CMD` | Gemini headless synthesis | `ask`, `evaluate`, `suggest`, extract LLM fallback, debug, research synthesis |
| `OPENAI_API_KEY` | OpenAI-compatible LLM endpoint | Compatibility path only when an OpenAI-compatible endpoint is explicitly configured |
| `TAVILY_API_KEY` | Tavily search API | `search`, `research` |
| `GITHUB_TOKEN` | GitHub | Optional — raises rate limits on `ingest` GitHub targets |
| `REDDIT_CLIENT_ID` / `REDDIT_CLIENT_SECRET` | Reddit OAuth app | Required for `ingest` Reddit targets |

If any of these are missing or invalid, the relevant command surfaces
the upstream provider's error verbatim. They never gate the Axon HTTP
surface.

---

## Quick verification

```bash
# MCP HTTP token (returns 401 without, 200/405/406 with)
curl -s -o /dev/null -w "%{http_code}\n" http://localhost:8001/mcp
curl -s -o /dev/null -w "%{http_code}\n" \
  -H "Authorization: Bearer $AXON_MCP_HTTP_TOKEN" http://localhost:8001/mcp

# Web panel password — verify file exists
test -f ~/.axon/panel-password && echo "panel password present"
ls -l ~/.axon/panel-password   # mode 0600

# Panel login round-trip
curl -s -X POST http://localhost:8001/api/panel/login \
  -H "Content-Type: application/json" \
  -d "{\"password\":\"$(cat ~/.axon/panel-password)\"}"
# → {"ok":true,"token":"..."}
```
