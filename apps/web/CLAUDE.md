# apps/web — Axon Next.js UI
Last Modified: 2026-03-15

Next.js 16 App Router frontend for the Axon RAG system. Runs on port `49010` in Docker via the `axon-web` service.

## Commands

```bash
pnpm dev          # Dev server (Turbopack) — hot reload
pnpm build        # Production build (output: standalone)
pnpm test         # Vitest (node environment, __tests__/**/*.test.{ts,tsx})
pnpm lint         # Biome check (lint + format check)
pnpm format       # Biome format --write (auto-fix)
```

## Architecture

```
app/layout.tsx           → Providers → AppShell → {children}
app/providers.tsx        → AxonWsContext + split WsMessages contexts + TooltipProvider
components/app-shell.tsx → PulseSidebar + CmdKPalette
app/page.tsx             → DashboardPage (Omnibox + ResultsPanel + NeuralCanvas)
```

App-level navigation buttons (`/mcp`, `/agents`, `/settings`) are hosted in `AppShell`, not in individual pages.

### Pages

There are only **two Next.js page routes**:

| Route | Component | Purpose |
|-------|-----------|---------|
| `/` | `AxonShell` (via `app/page.tsx`) | Single-page shell — all pane content lives here |
| `/jobs/[id]` | `JobDetailPage` | Job detail view (standalone page, links back to shell) |

**Panes are not page routes.** Shell, Pulse editor, terminal, MCP, cortex, settings, and logs are all rendered as right-pane panels inside `AxonShell`. Pane switching is **client-side state** managed by `axon-shell-state.ts` (`rightPane` state: `'editor' | 'terminal' | 'logs' | 'mcp' | 'settings' | 'cortex' | null`). Mobile pane switching uses `mobilePane` state (`'chat' | 'sidebar' | 'pane'`). No page navigation occurs when switching panes.

### API Routes

| Route | Methods | Auth | Purpose |
|-------|---------|------|---------|
| `/api/agents` | GET | Yes | List available Claude agents via `claude agents` CLI subprocess |
| `/api/ai/chat` | POST | Yes | Generic SSE chat stream via LLM (prompt → `OPENAI_BASE_URL`) |
| `/api/ai/command` | POST | Yes | Plate.js AI editor commands (edit/generate/comment) |
| `/api/ai/copilot` | POST | Yes | Plate.js ghost-text copilot completions |
| `/api/cortex/doctor` | GET | Yes | Service health check (proxied from Rust backend) |
| `/api/cortex/domains` | GET | Yes | Indexed domain list (proxied from Rust backend) |
| `/api/cortex/overview` | GET | Yes | Aggregated dashboard overview payload (domains/sources/stats/status summary) |
| `/api/cortex/sources` | GET | Yes | Indexed URL list (proxied from Rust backend) |
| `/api/cortex/stats` | GET | Yes | Qdrant + Postgres metrics (proxied from Rust backend) |
| `/api/cortex/status` | GET | Yes | Job queue status (proxied from Rust backend) |
| `/api/cortex/suggest` | GET | Yes | Suggest new URLs to crawl based on optional `?q=` focus query |
| `/api/docs` | GET | Yes | List/read crawl output docs from `AXON_OUTPUT_DIR`; `?action=list` or `?action=read&path=<rel>` |
| `/api/jobs` | GET | Yes | Job list with strict filter validation (`type`, `status`) |
| `/api/jobs/[id]` | GET | Yes | Job detail — searches all job tables in parallel; `?includeArtifacts=1` for manifest files |
| `/api/logs` | GET | Yes | SSE stream of Docker container logs; `?service=<name>` or `?service=all`, `?tail=N` (max 1000) |
| `/api/mcp` | GET, PUT, DELETE | Yes | MCP server config read/write/delete (`mcp.json`); hot-reloaded by ACP |
| `/api/mcp/status` | GET | Yes | Probe reachability of each configured MCP server (HTTP ping or `which` check for stdio) |
| `/api/omnibox/files` | GET | Yes | List/read local doc files (Pulse docs + `docs/` dir) for omnibox completion; `?id=<source:path>` to read one |
| `/api/pulse/chat` | POST | Yes | Stream ACP chat turns via WebSocket to the Rust bridge (`runAxonCommandWsStream`) |
| `/api/pulse/config` | POST | Yes | Probe ACP config options for an agent (cached 60s, coalesced in-flight) |
| `/api/pulse/doc` | GET | Yes | Load a Pulse doc by filename |
| `/api/pulse/save` | POST | Yes | Create/update Pulse docs (`.cache/pulse/*.md`) |
| `/api/pulse/source` | GET | Yes | Fetch and sanitize remote source text (SSRF-guarded; blocks private IPs) |
| `/api/sessions/list` | GET | Yes | List recent AI sessions (Claude/Codex/Gemini); `?assistant_mode=1` for assistant view |
| `/api/sessions/[id]` | GET | Yes | Fetch and parse a session file by ID (Claude JSONL / Codex JSONL / Gemini JSON) |
| `/api/shell/tool-preferences` | GET, PUT | Yes | Load/save MCP tool enablement state and named presets (Zod-validated) |
| `/api/workspace` | GET | Yes | Browse workspace filesystem (`?action=list&path=<p>`) or read a file (`?action=read&path=<p>`); also exposes `__claude` prefix for Claude config dir |

### WebSocket Proxy (next.config.ts)

```
/ws         → AXON_BACKEND_URL/ws         (Rust axon-workers, port 49000)
/ws/shell   → 127.0.0.1:SHELL_SERVER_PORT (authenticated node-pty shell, port 49011)
/download/* → AXON_BACKEND_URL/download/*
/output/*   → AXON_BACKEND_URL/output/*
```

`next.config.ts` also applies global security headers: CSP, `X-Frame-Options`, `Referrer-Policy`, `X-Content-Type-Options`, and HSTS outside development.
It also sets cache headers for `/api/cortex/*`: `s-maxage=30, stale-while-revalidate=60`.

WS client: `hooks/use-axon-ws.ts` — exponential backoff reconnect (1s → 30s), pending message queue. Reconnects on `online`, `pageshow`, and `visibilitychange`. Appends `?token=${NEXT_PUBLIC_AXON_API_TOKEN}` to the WS URL when the env var is set — this satisfies the Rust WS gate in `crates/web.rs`.

WS protocol types: `lib/ws-protocol.ts` — all message shapes for client↔server. **Modes must match `ALLOWED_MODES` in `crates/web/execute.rs`.**

### Pulse Chat

`/api/pulse/chat/route.ts` sends prompt turns via WebSocket to the Rust ACP bridge (`runAxonCommandWsStream`). The Rust server manages a persistent ACP adapter process (claude/codex/gemini) via `crates/services/acp/`.

- ACP events (`assistant_delta`, `thinking_content`, `tool_use`, etc.) stream through the bridge callbacks
- Timeout: 300s (`CLAUDE_TIMEOUT_MS`)
- Context budget: 800k chars (~200k tokens)
- Helper state: `stream-parser.ts` exports `StreamParserState`, `createStreamParserState`, `extractToolResultText` — used by route-helpers.ts for ACP event processing

### API Contracts

`/api/jobs` query filters are validated against strict allowlists:
- `type`: `crawl | extract | embed | github | reddit | youtube`
- `status`: `pending | running | completed | failed | canceled`
- `status=failed` includes both failed and canceled jobs
- invalid filters return `400` with structured error body

`/api/pulse/source` blocks private/loopback/local-network SSRF targets and returns `code: "ssrf_blocked"` on blocked URLs.

### API Error Format

Server routes use a shared JSON error envelope:

```json
{
  "error": "Message",
  "code": "optional_machine_code",
  "errorId": "optional_debug_id",
  "detail": {}
}
```

### Pulse File Storage

Docs stored in `.cache/pulse/*.md` (resolved from workspace root via `lib/pulse/workspace-root.ts`).

Format: YAML frontmatter + markdown body.
```
---
title: "My Doc"
createdAt: "..."
updatedAt: "..."
tags: []
collections: ["cortex"]
---

# Content here
```

`lib/pulse/storage.ts` — `savePulseDoc`, `updatePulseDoc` (update-in-place), `loadPulseDoc`, `listPulseDocs`.

**Autosave pattern** (`hooks/use-pulse-autosave.ts`): `filenameRef` + `docMetaRef` pattern — refs track filenames and cached `{createdAt,updatedAt,tags,collections}` to avoid stale closure bugs.

### Editor (Plate.js)

`components/editor/` — Plate.js v52 rich text editor with AI features.

- `use-chat.ts` — live AI streaming via `/api/ai/command`
- `use-chat-fake-stream.ts` — local dev fake stream for testing UI without LLM
- AI menu (`components/ui/ai-menu.tsx`) — floating AI commands on selection
- Requires `OPENAI_BASE_URL` + `OPENAI_API_KEY` + `OPENAI_MODEL` to be set

### NeuralCanvas

Bioluminescent animated background (`components/neural-canvas/`). Canvas intensity is driven by:
- Docker container CPU stats (via WS `stats` messages)
- Command execution state (full intensity while processing)
- Command completion pulse (0.15 for 3s, then back to 0)

Profile stored in `localStorage` key `axon.web.neural-canvas.profile`. Options: `current`, `subtle`, `cinematic`, `electric`.

## Environment Variables

```bash
# Backend URL (where Rust axon-workers serve HTTP/WS)
AXON_BACKEND_URL=http://localhost:49000     # env: AXON_BACKEND_URL

# Override WS URL for the client (optional — defaults to /ws path)
NEXT_PUBLIC_AXON_WS_URL=

# Shell WebSocket port (node-pty)
SHELL_SERVER_PORT=49011                    # default

# API auth token required by middleware.ts (unless insecure dev bypass is enabled)
AXON_WEB_API_TOKEN=CHANGE_ME

# Optional browser-only API token accepted by proxy.ts for /api/* (not /ws)
AXON_WEB_BROWSER_API_TOKEN=

# Comma-separated allowed origins for /api and /ws/shell (optional)
AXON_WEB_ALLOWED_ORIGINS=

# Development-only localhost bypass for auth gates (do not enable in production)
AXON_WEB_ALLOW_INSECURE_DEV=false

# Allow ?token= query auth for /api/* routes in proxy.ts (off by default)
AXON_WEB_ALLOW_QUERY_TOKEN=false

# Optional shell-specific token/origin overrides
AXON_SHELL_WS_TOKEN=
AXON_SHELL_ALLOWED_ORIGINS=

# Shell-server hardening controls
SHELL_SERVER_MAX_CONNECTIONS=8
SHELL_SERVER_IDLE_TIMEOUT_MS=900000
SHELL_SERVER_MAX_PAYLOAD_BYTES=65536
SHELL_SERVER_MAX_RESIZE_COLS=400
SHELL_SERVER_MAX_RESIZE_ROWS=160

# Optional client-side tokens used by shell websocket URL wiring
NEXT_PUBLIC_AXON_API_TOKEN=
NEXT_PUBLIC_SHELL_WS_TOKEN=

# Optional allowlist for Pulse chat `--betas` values.
# Defaults to: interleaved-thinking
AXON_ALLOWED_CLAUDE_BETAS=interleaved-thinking

# Qdrant collection (used in Pulse doc defaults)
AXON_COLLECTION=cortex                    # default

# Plate.js AI (required for editor AI features)
OPENAI_BASE_URL=http://YOUR_LLM_HOST/v1
OPENAI_API_KEY=your-key
OPENAI_MODEL=your-model-name

# AI Gateway (required by /api/ai/command and /api/ai/copilot)
AI_GATEWAY_API_KEY=your-ai-gateway-key

# /api/logs docker socket access (disabled by default)
AXON_WEB_ENABLE_DOCKER_SOCKET_LOGS=false
AXON_WEB_DOCKER_SOCKET_PATH=/var/run/docker.sock
```

## Code Style

Biome 2.4.4 — `biome.json` at root:
- **Single quotes**, **no semicolons**
- **2-space indent**, **100 char line width**
- ESM only, named exports, no default exports
- `@` alias resolves to project root (`apps/web/`)

## Testing

```bash
pnpm test              # run all tests
pnpm test -- --watch   # watch mode
pnpm test -- <pattern> # filter by filename
```

- Vitest 4, `node` environment (not jsdom — most tests are logic/API tests)
- Test files: `__tests__/**/*.test.{ts,tsx}`
- Path alias: `@` → project root (same as build)
- No snapshot tests for UI components (use `omnibox-snapshot.test.tsx` as reference)

## Gotchas

### Claude CLI User Ownership
Claude CLI runs as `node` (UID 1000) via `s6-setuidgid`. Bind-mount dirs under `${AXON_DATA_DIR}/axon/claude` must be owned by `node`, not `root`. Fix:
```bash
# On host:
sudo chown -R jmagar:jmagar /path/to/appdata/axon/claude/
```
Container fix: `docker/web/cont-init.d/15-fix-claude-dir-ownership` runs `chown -R node:node /home/node/.claude` on every start.

### MCP Config Path
Web MCP settings (`/api/mcp`) persist MCP servers to:
- `${AXON_DATA_DIR}/axon/mcp.json` when `AXON_DATA_DIR` is set
- `~/.config/axon/mcp.json` fallback when `AXON_DATA_DIR` is unset

Pulse ACP reads MCP servers from this same file (`crates/web/execute/mcp_config.rs`), so servers added via the Web UI are passed into ACP sessions.
MCP config changes are hot-reloaded by ACP and take effect on the next turn.

### Always Dark Mode
`app/layout.tsx` hardcodes `<html className="dark">`. Do not add theme toggling without updating this.

### Turbopack
`next.config.ts` sets `turbopack: { root: __dirname }`. Do NOT use webpack plugins that lack Turbopack equivalents in dev.

### Platejs Packages Require Transpile
Next.js standalone build requires `transpilePackages` for all `@platejs/*` and `platejs` packages (already set in `next.config.ts`). Adding new Plate plugins: add to `transpilePackages`.

### pnpm Auto-Sync in Container
The `axon-web` container runs a `pnpm-watcher` s6 service that polls `pnpm-lock.yaml` every 3s and reinstalls if changed. `pnpm add <pkg>` on the host takes effect in the container within ~3s — no rebuild needed. The `node_modules` anonymous volume is root-owned; the watcher runs as root.

### Shell Server
`shell-server.mjs` is the node-pty WebSocket bridge. Runs on `SHELL_SERVER_PORT` (default 49011). It is started separately from Next.js — not part of `pnpm dev`.

- Auth required via bearer/x-api-key/query token (`AXON_SHELL_WS_TOKEN` or fallback `AXON_WEB_API_TOKEN`)
- Origin validation enforced (`AXON_SHELL_ALLOWED_ORIGINS` / `AXON_WEB_ALLOWED_ORIGINS` / same-host fallback)
- PTY child env is allowlisted and no longer inherits full `process.env`

### WS Modes Must Match Rust Allow-List
`lib/ws-protocol.ts` defines `MODES`. Any mode added here must also be added to `ALLOWED_MODES` in `crates/web/execute.rs`, or the backend will reject the request.

### WS Auth Gate
`/ws` is a Next.js rewrite (raw TCP proxy) — Next.js middleware never runs for WS upgrade requests. Auth is enforced at the Rust layer (`crates/web.rs`).

Three tokens, two surfaces:
- `AXON_WEB_API_TOKEN` — primary token, gates both `/api/*` and `/ws`
- `AXON_WEB_BROWSER_API_TOKEN` — optional second-tier token for `/api/*` only (does NOT gate `/ws`)
- `NEXT_PUBLIC_AXON_API_TOKEN` — browser-exposed copy of `AXON_WEB_API_TOKEN` (must be set to the same value)

The browser sends `NEXT_PUBLIC_AXON_API_TOKEN` as `?token=` on the WS URL and as `x-api-key` on `/api/*`. `proxy.ts` accepts either `AXON_WEB_API_TOKEN` or `AXON_WEB_BROWSER_API_TOKEN` for `/api/*` routes.

MCP OAuth `atk_` tokens do **not** work for `/ws`. MCP clients use the MCP tool API.

### Pulse Autosave — Phantom Re-Save Guard
`use-pulse-autosave.ts`: `docMetaRef` is only reset when `incoming !== filenameRef.current`. Do NOT reset it on every render or prop change — this causes ghost re-saves on first-save filename sync.

### Qdrant Pre-Delete Race (Pulse Source Updates)
When updating a Pulse doc's Qdrant embedding, always use `?wait=true` on the delete endpoint before upsert. Without it, the upsert can race the delete and stale vectors accumulate.

## Performance Patterns

- **Background Work (`after`)**: Offload non-critical side effects (like vector embedding) using Next.js `after()` to return HTTP responses immediately.
- **Component Memoization**: Wrap heavy UI components (`AxonSidebar`, `AxonPromptComposer`, `PulseEditorPane`, etc.) in `React.memo()`.
- **Action Memoization**: Use `useMemo` for complex prop objects passed to memoized children (see `useAxonShellActions`).
- **Fetch Caching**: Next.js 15+ doesn't cache `fetch` by default. Use in-memory caches or `React.cache()` for idempotent network checks (e.g., `ensuredCollections` in save route).
- **Image Optimization**: Use `next/image` instead of raw `<img>` tags for native lazy-loading and optimization.
- **Dynamic Imports**: Use `next/dynamic` with `loading` skeletons to prevent layout shift (CLS).
