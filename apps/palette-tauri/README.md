# Axon Palette Tauri

Tauri v2 palette for the Axon HTTP API. The frontend uses React, Aurora registry components installed through the shadcn CLI, and an OpenAPI-generated TypeScript client.

The desktop shell launches hidden, registers a global shortcut, and exposes a tray/menu entry for showing the palette, opening settings, and quitting. The main window is an undecorated transient palette that hides on Escape, close, and blur by default.

## Commands

```bash
pnpm install
pnpm generate:api
pnpm typecheck
pnpm vite:build
pnpm dev
pnpm build
```

`generate:api` reads `https://axon.tootie.tv/api-docs/openapi.json` by default. Override with `AXON_OPENAPI_URL`.

The app reads Axon connection settings from the environment first, then `~/.axon/.env`:

- `AXON_SERVER_URL`
- `AXON_MCP_HTTP_TOKEN`
- `AXON_COLLECTION`

Runtime palette preferences are stored in the platform app config directory as `settings.json`. The settings panel can override the server URL, token, shortcut, collection, result limit, theme, and hide-on-blur behavior. Hide-on-blur is on by default so clicking outside the palette dismisses it.

Aurora tokens/components are rooted in the installed registry output:

- `src/components/aurora.css`
- `src/components/ui/aurora/*`
- `src/styles.css`
