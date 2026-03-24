# API Token Auth
Last Modified: 2026-03-24

The three API tokens cover two surfaces (`/api/*` and `/ws`). Understanding which token gates which surface prevents WebSocket auth failures.

It protects:

- `/api/*`
- `/ws`
- `/download/*`
- `/output/*`

## How the Three Tokens Work

| Variable | Scope | Purpose |
|----------|-------|---------|
| `AXON_WEB_API_TOKEN` | Server-only — **never expose to browser** | Primary token. Gates both `/api/*` (proxy.ts) and `/ws` (Rust WS gate via `?token=`). The `?token=` query param on `/ws` is a necessary limitation — WebSocket upgrade requests cannot carry custom headers. |
| `AXON_WEB_BROWSER_API_TOKEN` | Server-only (Next middleware) | Optional second-tier token for `/api/*` routes only. Does **not** gate `/ws`. When set, `NEXT_PUBLIC_AXON_API_TOKEN` must equal this value, not `AXON_WEB_API_TOKEN`. |
| `NEXT_PUBLIC_AXON_API_TOKEN` | Browser-exposed | Sent as `x-api-key` on `/api/*` and appended as `?token=` on `/ws`. Must equal `AXON_WEB_BROWSER_API_TOKEN` when that is set, otherwise must equal `AXON_WEB_API_TOKEN`. |
| `AXON_WEB_ALLOW_INSECURE_DEV` | Next middleware + shell server | Localhost-only development bypass |
| `AXON_SHELL_WS_TOKEN` | Shell websocket server | Optional dedicated token for `/ws/shell` |
| `NEXT_PUBLIC_SHELL_WS_TOKEN` | Browser shell client | Optional client token for `/ws/shell` |

## Token Matching Rules

The browser token (`NEXT_PUBLIC_AXON_API_TOKEN`) must match the token that the Rust server and Next middleware accept for the surface being accessed:

- **When `AXON_WEB_BROWSER_API_TOKEN` is NOT set:**
  - `NEXT_PUBLIC_AXON_API_TOKEN` must equal `AXON_WEB_API_TOKEN`.
  - All routes (`/api/*` and `/ws`) are gated by `AXON_WEB_API_TOKEN`.
  - The browser token matches the WS gate token — WebSocket auth works.

- **When `AXON_WEB_BROWSER_API_TOKEN` IS set:**
  - `NEXT_PUBLIC_AXON_API_TOKEN` must equal `AXON_WEB_BROWSER_API_TOKEN`.
  - `/api/*` is gated by `AXON_WEB_BROWSER_API_TOKEN`; `/ws` is gated by `AXON_WEB_API_TOKEN`.
  - The browser token **does not** match the WS gate token. WebSocket auth (`?token=`) will be rejected with `401` unless `NEXT_PUBLIC_AXON_API_TOKEN` is also set to `AXON_WEB_API_TOKEN` — but that defeats the purpose of token separation.
  - **Recommendation:** When using `AXON_WEB_BROWSER_API_TOKEN`, use a dedicated WebSocket client that sends `AXON_WEB_API_TOKEN` directly, or keep both tokens identical if WS access from the browser is required.

## Delivery

### WebSocket (`/ws`)

The web UI appends the token as a query parameter:

```text
ws://host/ws?token=<url-encoded-token>
```

The Rust server validates `?token=` against `AXON_WEB_API_TOKEN` only. `AXON_WEB_BROWSER_API_TOKEN` is not checked here.

Source:

- `apps/web/hooks/use-axon-ws.ts`
- `crates/web.rs`

### HTTP API (`/api/*`)

The Next.js client helper sends:

- `x-api-key: <token>`

The middleware also accepts:

- `Authorization: Bearer <token>`

The middleware checks `AXON_WEB_BROWSER_API_TOKEN` first (if set), then falls back to `AXON_WEB_API_TOKEN`.

Source:

- `apps/web/lib/api-fetch.ts`
- `apps/web/proxy.ts`

### Downloads (`/download/*`) and output files (`/output/*`)

These Rust endpoints accept the shared token from:

- `Authorization: Bearer <token>`
- `x-api-key: <token>`
- `?token=<token>`

Source:

- `crates/web/download.rs`
- `crates/web.rs`

## Shell WebSocket (`/ws/shell`)

The shell server runs separately on port `49011` and uses token auth only.

Priority:

1. `AXON_SHELL_WS_TOKEN`
2. `AXON_WEB_API_TOKEN`
3. `AXON_WEB_ALLOW_INSECURE_DEV=true` loopback bypass

## Security Notes

- **`AXON_WEB_API_TOKEN` is the WS gate.** Always set it for secured deployments. Do NOT expose it to the browser — keep it server-only.
- **Browser token must match the WS gate token for WebSocket auth to work.** If `AXON_WEB_BROWSER_API_TOKEN` is unset, `NEXT_PUBLIC_AXON_API_TOKEN` = `AXON_WEB_API_TOKEN`. If `AXON_WEB_BROWSER_API_TOKEN` is set, `NEXT_PUBLIC_AXON_API_TOKEN` = `AXON_WEB_BROWSER_API_TOKEN` — and browser WebSocket connections will be rejected by the Rust gate unless a separate mechanism delivers `AXON_WEB_API_TOKEN`.
- If the token changes, restart or rebuild the web app so the browser bundle picks up the new value.
- Query-string delivery for `/ws` and browser downloads is convenient but visible in URL logs. Rotate the token if those logs are exposed.

## Troubleshooting

### `401 Unauthorized` on WebSocket

1. Verify `AXON_WEB_API_TOKEN` is set on the server.
2. Verify `NEXT_PUBLIC_AXON_API_TOKEN` equals `AXON_WEB_API_TOKEN` (not `AXON_WEB_BROWSER_API_TOKEN`, which does not gate `/ws`).
3. Confirm `use-axon-ws.ts` is appending `?token=` to the WebSocket URL.

### `401 Unauthorized` on `/api/*`

1. Verify the token in `NEXT_PUBLIC_AXON_API_TOKEN` matches `AXON_WEB_BROWSER_API_TOKEN` (if set) or `AXON_WEB_API_TOKEN`.
2. Confirm `apiFetch()` is sending `x-api-key`.

### `API authentication is not configured`

`AXON_WEB_API_TOKEN` is not configured and `AXON_WEB_ALLOW_INSECURE_DEV` is false. Set the token and restart the web app.
