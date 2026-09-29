# Axon Palette

Read [README](README.md), [frontend scripts](package.json), [desktop shell](src-tauri/), and [palette testing](../../docs/development/desktop-palette-testing.md). This app has a separate Rust workspace and independently managed release version; do not synchronize it to the CLI version.

## UI and action ownership

[App.tsx](src/App.tsx) orchestrates cross-cutting view state and passes props/callbacks to presentational components. Put reusable business logic in [src/lib](src/lib/), not duplicated conditionals across views.

New actions must update [actions](src/lib/actions.ts), [behavior registry](src/lib/actionRegistry.ts), [request builders](src/lib/actionRequest.ts), [formatters](src/lib/actionFormat.ts), and [display metadata](src/lib/actionMeta.ts). Add a structured renderer only when required in [OperationResultView](src/components/palette/OperationResultView.tsx). Registry exhaustiveness and shim parity must continue to catch missing behavior; do not silently fall back to raw JSON for an unsupported action.

## Runtime seam and security

Use [invoke.ts](src/lib/invoke.ts) rather than importing Tauri APIs throughout app code. Tauri uses Rust IPC networking; browser development uses the same-origin request fallback. Test both paths and do not mistake the browser fixture's stubbed events for working production streaming.

Keep API credential handling, panel unlock, CSP, IPC boundaries, and URL validation aligned with [server security](../../docs/operations/security.md) and [HTTP contracts](../../docs/reference/http-api.md). Errors shown to agents/users need failed operation, safe reason, partial state, and a retry or correction path; never expose secrets or write masked placeholders back as credentials.

## Components and tests

Use existing Aurora tokens/primitives from [components](src/components/) and [registry configuration](components.json). Reuse the canonical button and shared interactive controls; keep one-off layout in semantic styles rather than creating competing primitives.

TypeScript uses colocated *.test.ts(x) Vitest tests, not Rust *_tests.rs naming. Rust shell code follows Rust sidecar conventions. Add result cases to [OperationResultFixture](src/components/palette/OperationResultFixture.tsx); this fixture has no backend and cannot prove a real API call succeeds.

For UI/action changes run pnpm test and pnpm typecheck; use pnpm verify for full frontend validation. Check the independent shell with its src-tauri manifest when Rust changes. Keep generated API types synchronized through the app's generate:api/check:api scripts and current server OpenAPI. See [release checklist](../../docs/development/release-checklist.md) before version changes.
