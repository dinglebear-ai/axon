# Axon Web (Next.js)
Last Modified: 2026-03-03

`apps/web` is the Next.js interface for Axon. It provides a command omnibox, workspace flows, and live command execution over WebSocket.

## Run

```bash
pnpm --dir apps/web dev
```

Open `http://localhost:3000`.

## API + Shell Security

Canonical environment variable reference lives in the repository README section `Optional Web App Security (apps/web)`.
This file documents behavior and runtime semantics; keep the full variable list in one place to prevent drift.

All `app/api/*` routes are now protected by `apps/web/proxy.ts`.

- Auth headers accepted:
  - `Authorization: Bearer <token>`
  - `x-api-key: <token>`
- Required server env:
  - `AXON_WEB_API_TOKEN`
- Required browser env (only for client-initiated `/api/*` calls):
  - `NEXT_PUBLIC_AXON_API_TOKEN` (must match `AXON_WEB_API_TOKEN`)
- Origin enforcement:
  - `AXON_WEB_ALLOWED_ORIGINS` (comma-separated), or same-origin fallback if unset
- Local-only bypass (development only):
  - `AXON_WEB_ALLOW_INSECURE_DEV=true`

The terminal shell websocket (`/ws/shell`) now enforces auth and origin checks in `shell-server.mjs`.

- Preferred shell token: `AXON_SHELL_WS_TOKEN` (falls back to `AXON_WEB_API_TOKEN`)
- Optional shell-specific origin allowlist: `AXON_SHELL_ALLOWED_ORIGINS`
- Client token wiring:
  - `NEXT_PUBLIC_SHELL_WS_TOKEN` (preferred)
  - `NEXT_PUBLIC_AXON_API_TOKEN` (fallback)
- Pulse beta allowlist is controlled by `AXON_ALLOWED_CLAUDE_BETAS` (see root README env table).

### Redis Cache Runtime Note

`apps/web` API routes run in the Next.js process rooted at `apps/web`, so they do not automatically inherit root repo `.env` values.

- If you enable Redis-backed web caching, set `AXON_REDIS_URL` in `apps/web/.env.local`.
- This is the only required duplicated setting for web cache runtime scope isolation.

Next.js response hardening is configured in `next.config.ts` with CSP, `X-Frame-Options`, `Referrer-Policy`, and HSTS (non-dev).
`/api/cortex/*` responses are cache-tuned with `s-maxage=30, stale-while-revalidate=60`.

### Cortex Mission Control

The right-side Cortex pane now renders a unified **Mission Control** surface instead of separate Status/Doctor/Sources/Domains/Stats tabs.

- Aggregated API: `GET /api/cortex/overview`
- UI root: `components/cortex/mission-control-pane.tsx`
- Shell integration: `components/shell/axon-cortex-pane.tsx`

## Performance & Optimization

The web application follows a strict performance-first architecture to ensure a smooth, bioluminescent experience.

### React Rendering
- **God Hook Memoization**: `useAxonShellState` manages global shell state. To prevent re-render cascades, child components like `AxonSidebar`, `AxonPromptComposer`, and `PulseEditorPane` are wrapped in `React.memo`.
- **Prop Stability**: High-frequency state changes (typing, streaming) are isolated. Shared prop objects are stabilized via `useMemo` in `useAxonShellActions`.

### Server Side
- **Immediate Response**: The `/api/pulse/save` route uses Next.js `after()` to offload vector embedding and Qdrant operations. Users receive a save confirmation immediately, while heavy inference runs in the background.
- **Idempotent Caching**: Repetitive network checks (like verifying if a Qdrant collection exists) are cached in-memory to eliminate redundant RTTs.

### UI & Assets
- **Image Optimization**: All images use `next/image` for automatic format conversion, resizing, and lazy loading.
- **Dynamic Skeletons**: Heavy components (like the rich-text editor) use dynamic imports with tailored loading fallbacks to eliminate Cumulative Layout Shift (CLS).

## API Contracts

- `GET /api/jobs` validates `type` (`crawl|extract|embed|github|reddit|youtube`) and `status` (`pending|running|completed|failed|canceled`); invalid filters return `400`.
- `GET /api/pulse/source` enforces URL SSRF protections and returns `code: "ssrf_blocked"` when blocked.
- Shared error envelope: `{ error, code?, errorId?, detail? }`.

### Jobs API Docs

- Jobs API route and helper documentation: `apps/web/docs/jobs-api.md`

## Omnibox Behavior

The omnibox supports keyboard-first operation with explicit visual state feedback.

### Focus Shortcut

- Press `/` to focus the omnibox when focus is not already inside an editable field.
- Shortcut is ignored for `input`, `textarea`, `select`, and content-editable elements.

### Mode Mentions

- Start input with `@` to enter mode mention selection.
- Example: `@c` suggests up to 3 matching modes (`crawl`, etc.).
- `Tab` or `Enter` applies the selected mode.
- After mode selection, the mention is removed and the omnibox is cleared for the next input.
- The UI shows:
  - active mention suggestions
  - selected/hovered mention state
  - transient `Mode selected: <label>` confirmation

### File Mentions

- Use `@` mentions in normal text to attach local context files.
- Suggestions are fuzzy-ranked (exact/prefix/contains/subsequence) and include recency bias from recent picks.
- Suggestion list is capped at 3 entries.
- Sources:
  - `docs/**` (`.md`, `.mdx`, `.txt`, `.rst`)
  - `.cache/pulse/**` entries from Pulse docs storage
- Selected files are shown as removable context chips under the omnibox.

### Keyboard Controls

- `ArrowUp`/`ArrowDown`: move mention selection.
- `Tab`/`Enter`: apply selected mention.
- `Escape`: close dropdown/options and clear active mention suggestions.
- `Enter` (without mention selection): execute current command.

## File Context Injection

Before command execution, mentioned files are resolved and appended to the input as a context section.

- Up to 3 files are loaded.
- Each file contributes a capped excerpt (up to 2400 chars).
- Execution flags include:
  - `context_files=<comma-separated labels>`

## Omnibox Local File API

### `GET /api/omnibox/files`

Returns mention candidates:

```json
{
  "files": [
    {
      "id": "docs:guide/setup.md",
      "label": "setup",
      "path": "docs/guide/setup.md",
      "source": "docs"
    }
  ]
}
```

### `GET /api/omnibox/files?id=<id>`

Returns file payload for context injection:

```json
{
  "file": {
    "id": "docs:guide/setup.md",
    "label": "setup",
    "path": "docs/guide/setup.md",
    "source": "docs",
    "content": "..."
  }
}
```

Route safety:

- `id` must be prefixed by `docs:` or `pulse:`.
- path traversal (`..`) is rejected.
- resolved paths must stay under source roots.

## Environment Variables

### Web Runtime

- `AXON_BACKEND_URL`: backend HTTP/WS origin used by rewrites and server routes
- `NEXT_PUBLIC_AXON_WS_URL`: optional client-side websocket override
- `SHELL_SERVER_PORT`: shell websocket port used by `/ws/shell`
- `AXON_BIN`: optional binary override for `/api/cortex/*`
- `AXON_WORKSPACE`: workspace root path used by web-side helpers
- `AXON_DATA_DIR`: runtime data root for persisted web artifacts and config
- `AXON_OUTPUT_DIR`: output root consumed by `/api/docs`

### Web Auth

- `AXON_WEB_API_TOKEN`: required API token enforced by `proxy.ts`
- `NEXT_PUBLIC_AXON_API_TOKEN`: browser token used by client API helpers
- `AXON_WEB_BROWSER_API_TOKEN`: optional browser-only API token accepted by `proxy.ts`
- `AXON_WEB_ALLOWED_ORIGINS`: comma-separated origin allowlist for `/api/*`
- `AXON_WEB_ALLOW_INSECURE_DEV`: localhost-only auth bypass for development
- `AXON_WEB_ALLOW_QUERY_TOKEN`: enables `?token=` auth for `/api/*` when explicitly enabled
- `AXON_SHELL_WS_TOKEN`: optional shell websocket token override
- `AXON_SHELL_ALLOWED_ORIGINS`: optional shell websocket origin allowlist
- `NEXT_PUBLIC_SHELL_WS_TOKEN`: optional browser token for `/ws/shell`

### Shell Server Hardening

- `SHELL_SERVER_MAX_CONNECTIONS`: maximum concurrent shell websocket clients
- `SHELL_SERVER_IDLE_TIMEOUT_MS`: idle timeout for shell sessions
- `SHELL_SERVER_MAX_PAYLOAD_BYTES`: maximum inbound websocket payload size
- `SHELL_SERVER_MAX_RESIZE_COLS`: resize column clamp
- `SHELL_SERVER_MAX_RESIZE_ROWS`: resize row clamp

### AI Routes

- `OPENAI_BASE_URL`: upstream OpenAI-compatible base URL for `/api/ai/chat`
- `OPENAI_API_KEY`: upstream key for `/api/ai/chat`
- `OPENAI_MODEL`: default model for `/api/ai/chat`
- `AI_GATEWAY_API_KEY`: gateway key for `/api/ai/command` and `/api/ai/copilot`
- `AXON_ALLOWED_CLAUDE_BETAS`: allowlist for Pulse chat `--betas`

### Logs and Docker

- `AXON_WEB_ENABLE_DOCKER_SOCKET_LOGS`: enables `/api/logs`
- `AXON_WEB_DOCKER_SOCKET_PATH`: Docker socket path used by `/api/logs`

### Jobs and Docs

- `AXON_COLLECTION`: default collection used by Pulse docs and jobs views

## Key Files

- `components/omnibox.tsx`: omnibox interaction/state UI
- `components/cortex/mission-control-pane.tsx`: unified Cortex Mission Control UI
- `lib/omnibox.ts`: mention parsing, ranking, phase derivation helpers
- `app/api/omnibox/files/route.ts`: local docs listing + content fetch for mentions
- `hooks/use-ws-messages.ts`: split execution/workspace/action contexts + compatibility hook
- `proxy.ts`: API authentication + origin enforcement
- `shell-server.mjs`: authenticated node-pty websocket bridge with restricted child env
- `lib/command-options.ts`: shared omnibox command-option type (no component-layer coupling)
- `docs/jobs-api.md`: jobs list/detail route behavior and shared helper boundaries
- `app/evaluate/page.tsx`: side-by-side evaluate rendering showcase
- `__tests__/omnibox.test.ts`: omnibox helper unit tests
