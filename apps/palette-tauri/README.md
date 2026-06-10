# Axon Palette Tauri

Tauri v2 palette for the Axon HTTP API. The frontend uses React, Aurora registry components installed through the shadcn CLI, and an OpenAPI-generated TypeScript type layer.

> **Note (M1):** OpenAPI types are generated into `src/lib/axon-api.d.ts` via `pnpm generate:api`.  Request execution is currently hand-coded in `src/lib/axonClient.ts`; a full migration to the generated request helpers is tracked in issue #177 (finding M1).

The desktop shell launches hidden, registers a global shortcut, and exposes a tray/menu entry for showing the palette, opening settings, and quitting. The main window is an undecorated transient palette that hides on Escape, close, and blur by default.

## Commands

```bash
# Install dependencies — always use --frozen-lockfile in CI and for reproducible builds.
# See .npmrc for the project-level setting.
pnpm install --frozen-lockfile

# Generate the OpenAPI type layer from the local spec (offline).
# Pass --live or set AXON_OPENAPI_URL to fetch from a running instance.
pnpm generate:api

# TypeScript type check (no emit)
pnpm typecheck

# Frontend-only Vite build
pnpm vite:build

# Full CI-grade verification: frozen install + tests + typecheck + Vite build
pnpm verify

# Tauri dev server (requires a running axon instance or AXON_DEV_SERVER env var)
pnpm dev

# Tauri release build
pnpm build

# Rust unit tests (separate from the frontend — palette-tauri is not in the root workspace)
cargo test --manifest-path apps/palette-tauri/src-tauri/Cargo.toml
```

`generate:api` reads `apps/web/openapi/axon.json` by default (local file, offline).
Override with `AXON_OPENAPI_URL` or pass `--live` to fetch from a live instance.

The app reads Axon connection settings from the environment first, then `~/.axon/.env`:

- `AXON_SERVER_URL`
- `AXON_MCP_HTTP_TOKEN`
- `AXON_COLLECTION`

Runtime palette preferences are stored in the platform app config directory as `settings.json`. The settings panel can override the server URL, token, shortcut, collection, result limit, theme, and hide-on-blur behavior. Hide-on-blur is on by default so clicking outside the palette dismisses it.

## Notes

- **Frozen lockfile:** use `pnpm install --frozen-lockfile` (or `pnpm verify`) for reproducible installs.  An `.npmrc` with `frozen-lockfile=true` is included so CI tools that default to `pnpm install` also pick it up.
- **Rust tests:** `apps/palette-tauri/src-tauri` is isolated from the root Cargo workspace.  Run Rust tests explicitly with `cargo test --manifest-path apps/palette-tauri/src-tauri/Cargo.toml`.  The `palette-tauri` job in `.github/workflows/ci.yml` already includes this step.
- **CSP — `style-src 'unsafe-inline'`:** Tailwind CSS v4 emits inline `<style>` blocks via the Vite plugin, and shadcn/aurora components use CSS custom properties that cannot be nonce-hashed.  Removing `'unsafe-inline'` breaks all styling in production.  Migration path: adopt Vite's experimental CSP nonce support once stable.
- **Networking model:** In production, the renderer makes **no direct network calls** — all HTTP traffic flows through the Rust bridge via Tauri IPC (`connect-src` only allows IPC transports).  In development (`vite:dev`), a Vite proxy (see `vite.config.ts`) forwards `/v1/*` requests to the configured `AXON_DEV_SERVER` so the renderer can be tested in a plain browser.

Aurora tokens/components are rooted in the installed registry output:

- `src/components/aurora.css`
- `src/components/ui/aurora/*`
- `src/styles.css`
