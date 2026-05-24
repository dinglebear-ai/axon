# Axon Palette Tauri

Tauri v2 palette for the Axon HTTP API. The frontend uses React, Aurora registry components installed through the shadcn CLI, and an OpenAPI-generated TypeScript client.

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

Aurora tokens/components are rooted in the installed registry output:

- `src/components/aurora.css`
- `src/components/ui/aurora/*`
- `src/styles.css`

