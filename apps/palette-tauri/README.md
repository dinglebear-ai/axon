# Axon Palette Tauri

> Current Palette client docs. The source-pipeline contract is
> `docs/pipeline-unification/surfaces/palette-contract.md`; generated
> DTOs/OpenAPI should drive source/job/graph and memory surfaces.

Tauri v2 desktop + Android palette for the Axon HTTP API. The frontend uses React, Aurora registry components installed through the shadcn CLI, and an OpenAPI-generated TypeScript type layer.

> **Note (M1):** OpenAPI types are generated into `src/lib/axon-api.d.ts` via `pnpm generate:api`.  Request execution is currently hand-coded in `src/lib/axonClient.ts`; a full migration to the generated request helpers is tracked in issue #177 (finding M1).  Known state: the generated `axon-api.d.ts` is **not yet imported anywhere** — it serves only as a reference for the wire shapes hand-coded in `axonClient.ts`, and responses are read by **key-probing** untyped payloads rather than against the generated types. Closing #177 is what wires the generated types in.

On desktop, the shell launches hidden, registers a global shortcut, and exposes a tray/menu entry for showing the palette, opening settings, and quitting. The desktop main window is an undecorated transient palette that hides on Escape, close, and blur by default. Android uses a normal visible full-screen main window with a phone-oriented top-anchored command surface.

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

# Tauri dev server (requires a running axon instance or AXON_DEV_SERVER env var).
# With no reachable backend the shell launches but every action fails at request
# time — for backend-free UI iteration use the fixture harness below instead.
pnpm dev

# Browser dev entry — runs the renderer in a plain browser via the dev/prod invoke
# seam (src/lib/invoke.ts). The Vite proxy forwards /v1/* to AXON_DEV_SERVER.
pnpm vite:dev

# No-backend result-view harness: renders OperationResultFixture against
# representative payloads at /?fixture=operation-results (no axon instance needed).
pnpm fixture:operation-results

# Tauri release build
pnpm build

# Rust unit tests (separate from the frontend — palette-tauri is not in the root workspace)
cargo test --manifest-path apps/palette-tauri/src-tauri/Cargo.toml
```

`generate:api` reads `apps/web/openapi/axon.json` by default (local file, offline).
Override with `AXON_OPENAPI_URL` or pass `--live` to fetch from a live instance.

## Android

Android uses Tauri 2's generated Gradle project under `src-tauri/gen/android/`. That directory is intentionally gitignored and can be regenerated from the tracked Tauri configuration.

Host prerequisites:

- JDK 21 (`JAVA_HOME`)
- Android SDK with platform-tools, build-tools, command-line tools, and an installed platform (`ANDROID_HOME` / `ANDROID_SDK_ROOT`)
- Android NDK 28.2.13676358 or newer (`NDK_HOME`); CI pins 28.2.13676358
- Rust Android targets: `aarch64-linux-android`, `armv7-linux-androideabi`, `i686-linux-android`, and `x86_64-linux-android`

```bash
# Generate/regenerate the ignored Android Gradle project.
pnpm exec tauri android init --ci

# Run against a connected device/emulator.
pnpm android:dev

# Build Android APKs (Tauri builds all supported ABIs by default).
pnpm android:build

# Fast architecture-specific CI/local smoke.
pnpm exec tauri android build --debug --target aarch64 --apk --ci

# Google Play / release artifacts.
pnpm exec tauri android build --apk --aab --ci --config src-tauri/tauri.ci.conf.json
```

The mobile surface intentionally excludes desktop-local actions whose semantics do not carry over safely: **Browser**, **Files/SFTP**, and **Terminal** are removed from mobile discovery, slash commands, and help, and their Rust IPC commands reject direct calls on mobile. Desktop-only global-shortcut, hide-on-blur, and hide-palette controls are also hidden. Remote Axon HTTP actions, GitHub browsing, settings, history, and OAuth remain available.

`.github/workflows/ci.yml` has a required `palette-tauri-android` lane that builds an ARM64 debug APK whenever Palette changes. `palette-release.yml` builds APK + AAB artifacts for the Palette `palette-v*` release stream. Release publishing requires `ANDROID_KEYSTORE_BASE64`, `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS`, and `ANDROID_KEY_PASSWORD`. Manual non-publishing runs can emit explicitly `-unsigned` inspection artifacts when those secrets are absent.

Runtime palette preferences are stored in the platform app config directory as `settings.json`. To connect, configure the Axon server URL and either a static bearer token or OAuth sign-in. The palette does not read or write the server's `~/.axon/.env` or `~/.axon/config.toml`; server runtime configuration stays on the Axon host. Palette-local preferences remain available on both clients, while desktop-only shortcut and hide-on-blur controls are hidden on mobile.

When using the browser dev entry against a public reverse-proxied Axon endpoint
for live QA, set `AXON_DEV_SERVER` to the live server and `AXON_DEV_TOKEN` to a
token the dev proxy can inject as `authorization` and `x-api-key` headers.
Set `AXON_DEV_STRIP_ORIGIN=true` explicitly only if browser-origin POSTs are
rejected before Axon handles auth; this mode is not sufficient on its own
without `AXON_DEV_TOKEN`. This is a privileged server-side proxy mode: Vite
strips the browser `Origin` header, forwards to `AXON_DEV_SERVER`, injects the
dev token when configured, and marks origin-stripped requests with
`x-axon-dev-proxy: origin-stripped`. See `vite.config.ts` for the dev proxy
setup. Keep the normal `pnpm vite:dev` default for everyday local development
so public-origin/CORS drift remains visible.

## Authentication

The palette authenticates to Axon two ways, and both can be configured at once:

- **Static bearer token** — enter it in the **Bearer token** field in Settings.
- **OAuth "Sign in with Google"** — click **Sign in with Google** in the Connection tab's **Authentication** block. The palette runs an OAuth 2.0 Authorization Code + PKCE flow (RFC 8414 discovery → RFC 7591 dynamic client registration → server-native callback polling → `/token` exchange) **entirely in the Rust shell**. The system browser is launched through Tauri's cross-platform `tauri-plugin-opener` and completes on the Axon server's HTTPS `/native/callback` endpoint; the palette polls `/native/poll` for the short-lived authorization code and then exchanges it with PKCE. No webview HTTP and no new Tauri capabilities or CSP changes are involved.

Issued credentials are stored beside `settings.json` as `<app config dir>/oauth.json` (mode `0o600`, holding the refresh token) and cached in-process. The access token is refreshed proactively (60s skew) with single-flight safety: concurrent requests at expiry produce exactly one `/token` call and one disk write, against the `token_endpoint` persisted from discovery (so reverse-proxy deployments refresh correctly). **When signed in, the OAuth token takes precedence over the static token**; if no valid OAuth token exists for the active server, the static token is used.

OAuth requires the network calls and browser-open URL to be `https` (or loopback `http`), and the target server must run with `AXON_AUTH_MODE=oauth` and dynamic client registration enabled — otherwise sign-in reports that the server does not support OAuth login and you should use a static bearer token.

**Known tradeoff:** each sign-in dynamically registers a fresh client on the server for the fixed native callback endpoint advertised by discovery. Registrations are still not reused because the palette keeps the login flow stateless across launches; server-side registration is rate-limited and bounded by operator policy, and the palette does not garbage-collect prior registrations.

## Notes

- **Frozen lockfile:** use `pnpm install --frozen-lockfile` (or `pnpm verify`) for reproducible installs.  An `.npmrc` with `frozen-lockfile=true` is included so CI tools that default to `pnpm install` also pick it up.
- **Rust tests:** `apps/palette-tauri/src-tauri` is isolated from the root Cargo workspace.  Run Rust tests explicitly with `cargo test --manifest-path apps/palette-tauri/src-tauri/Cargo.toml`.  The `palette-tauri` job in `.github/workflows/ci.yml` already includes this step.
- **CSP — `style-src 'unsafe-inline'`:** Tailwind CSS v4 emits inline `<style>` blocks via the Vite plugin, and shadcn/aurora components use CSS custom properties that cannot be nonce-hashed.  Removing `'unsafe-inline'` breaks all styling in production.  Migration path: adopt Vite's experimental CSP nonce support once stable.
- **Networking model:** In production, the renderer makes **no direct network calls** — all HTTP traffic flows through the Rust bridge via Tauri IPC (`connect-src` only allows IPC transports).  In development (`vite:dev`), a Vite proxy (see `vite.config.ts`) forwards `/v1/*` requests to the configured `AXON_DEV_SERVER` so the renderer can be tested in a plain browser.

Aurora tokens/components are rooted in the installed registry output:

- `src/components/aurora.css`
- `src/components/ui/aurora/*`
- `src/styles.css`

Components come from the `@aurora` shadcn registry, configured in `components.json`
(`"@aurora": "https://aurora.tootie.tv/r/{name}.json"`). Install or update a
primitive through the shadcn CLI, e.g.:

```bash
pnpm dlx shadcn@latest add @aurora/button
```
