# CLAUDE.md — Axon Palette (Tauri frontend)

Contributor guide for `apps/palette-tauri` — the desktop command palette for the Axon HTTP API. This is the **frontend / React side** of the app; the Rust desktop shell lives in `src-tauri/`. Read `README.md` first for the runtime/security model (CSP, IPC networking, frozen lockfile); this file covers architecture and the conventions you must follow when changing the UI.

> The palette is versioned independently from the CLI (`package.json` / `tauri.conf.json` carry the palette version; the CLI's `Cargo.toml` version is unrelated). Do not sync them.

## Architecture — unidirectional data flow with an `App.tsx` orchestrator

The frontend is deliberately a **single stateful orchestrator (`src/App.tsx`) over stateless/business-logic helpers**. This is intentional, not tech debt — keep it unless a refactor is explicitly tracked.

- **`src/App.tsx` owns view state and orchestration.** All cross-cutting UI state (`query`, `modeAction`, `selected`, `config`, `run`, the `settingsOpen`/`browseOpen`/`historyOpen` overlay flags, etc.) lives in `useState` flags in `App`. `App` wires user input → action selection → execution → result rendering, and threads state down to presentational components (`PaletteCommandBar`, `ActionList`, `OutputPanel`, `SettingsPanel`, `HistoryPanel`, `PaletteFooter`) via props. Data flows **down** as props; events flow **up** as callbacks. Child components do not own app state.
- **Business logic lives in `src/lib/*`, not in components.** Pure functions and typed models live under `src/lib/` (e.g. `actions.ts`, `actionMeta.ts`, `actionHelp.ts`, `paletteView.ts`, `format.ts`, `payload.ts`, `url.ts`, `axonClient.ts`, `configModel.ts`, `historyRun.ts`, `runState.ts`). Hooks that encapsulate stateful side effects also live there: `useActionRunner.ts` (request/response + streaming dispatch), `useCrawlJob.ts` (polled crawl-job state), `useWindowChrome.ts` (window sizing). Prefer adding logic to a `src/lib` helper with a unit test over inlining it in a component.
- **Presentational components live in `src/components/`.** `components/palette/*` are app-specific views; `components/ui/aurora/*` and `components/ui/*` are the design-system primitives (see below). Components should be thin renderers over props.

### The dev/prod invoke seam — `src/lib/invoke.ts`

Every backend call goes through the single wrapper in `src/lib/invoke.ts` — read its header comment, it is the canonical explanation. Summary:

- In the **Tauri runtime**, `invoke()` forwards to the real `@tauri-apps/api/core` invoke, and `appWindow` is the real window (event listeners wired). All HTTP goes through the Rust IPC bridge.
- In a **plain browser** (`pnpm vite:dev` / the fixture harness — used for design iteration and screenshots), `invoke()` falls back to same-origin `fetch` for `axon_http_request` (the Vite proxy forwards `/v1/*` to a live `axon serve`), and `appWindow.listen` is a no-op stub so streaming callers stay callable.
- `isTauriRuntime` distinguishes the two. **Never import `@tauri-apps/api/*` directly in app code** — go through `invoke.ts` so the browser dev path keeps working.

## Test convention — co-located `*.test.ts(x)` (NOT the Rust sidecar rule)

The repo-root `CLAUDE.md` mandates a `_tests.rs` sidecar convention with `#[path]` declarations. **That rule applies only to Rust crates — `apps/palette-tauri/src-tauri/` and the root workspace crates — NOT to this TypeScript frontend.**

The TS frontend uses **co-located Vitest test files**: a source file `foo.ts(x)` has its tests in a sibling `foo.test.ts(x)` next to it (e.g. `src/lib/format.ts` → `src/lib/format.test.ts`, `src/components/palette/OperationResultView.tsx` → `OperationResultView.test.tsx`). Run with `pnpm test` (`vitest run`). Component render tests use `@testing-library/react`; jsdom is opted into per file. When adding a source file with logic, add its co-located `*.test.ts(x)` in the same directory.

## Design system — canonical layer + decision rule

Token discipline is **already good and centralized**: every color/spacing value flows through `var(--aurora-*)` custom properties (rooted in `src/components/aurora.css` and `src/styles.css`), used in 600+ places. Do not introduce raw hex — use the Aurora tokens. (A Tailwind v4 `@theme` bridge is being added so utility classes like `text-text-primary` / `ring-accent-primary` resolve to those same tokens; the `--aurora-*` aliases stay — the bridge is additive.)

The one ambiguity worth a rule is **component abstraction**. Decision rule for adding UI:

- **Recurring interactive atom** (a button, input, badge, toggle that will appear more than once) → use or extend an Aurora primitive in `src/components/ui/aurora/`. There is exactly **one** canonical button (`ui/aurora/button.tsx`) — **never add a second button form**.
- **One-off layout / page-specific structure** → use semantic classes in `src/styles.css` (e.g. `.action-row-main`, `.command-input`, `.idle-tray`). Do not promote a one-off into a primitive.

Aurora primitives are installed from the `@aurora` shadcn registry (`components.json` → `https://aurora.tootie.tv/r/{name}.json`); see README for the install path.

## How to add a new palette action

Adding an action currently touches **~9 edit sites** across the codebase — this is the most error-prone task here and there is no `assertNever` guard, so a missed site degrades silently (e.g. a structured view falls back to raw `<pre>`). **This manual checklist is the interim process; consolidating these into a single action-behavior registry is a tracked FOLLOW-UP (A-H1) — do not attempt that refactor inline.** Edit, in order:

1. **`src/lib/actions.ts`** — add the action entry to the `ACTIONS` array.
2. **`src/lib/actions.ts`** — add the subcommand to the `PaletteSubcommand` union type.
3. **`src/lib/actionMeta.ts`** — add display metadata to `ACTION_META`.
4. **`src/lib/axonClient.ts`** — handle the new subcommand in `bodyFor` (request body shaping).
5. **`src/lib/axonClient.ts`** — add the route in `actionRouteTemplate`.
6. **`src/lib/format.ts`** — classify output in `outputKindFor` and render fallback text in `formatPayload`.
7. **`src/components/palette/OperationResultView.tsx`** — add one entry to the `STRUCTURED_VIEWS` map. Both `hasStructuredOperationView` (the allowlist) and the renderer dispatch derive from that single map, so they can no longer drift (there is no longer a hand-synced `switch`).
8. **`src/components/palette/OutputPanel.tsx`** — add an icon mapping in `outputIcon`.
9. **`src/components/palette/ActionIcon.tsx`** — add an icon mapping in `actionIcon`.

## Result-view fixture harness

`pnpm fixture:operation-results` opens the app in a browser at `/?fixture=operation-results`, which renders `src/components/palette/OperationResultFixture.tsx` instead of `App` (see `src/main.tsx`). This iterates the structured result views (`CrawlJobView`, `EvaluateView`, `StatsView`, `StatusView`, `HelpResultView`, `OperationResultViewShared`, etc.) against representative payloads **with no backend**. Add a new case by adding a fixture payload in `OperationResultFixture.tsx`. The same fixture payloads are the source for component render tests (see the test convention above).

## Commands

See `README.md` for the full command reference. Quick map: `pnpm dev` (Tauri shell), `pnpm vite:dev` (browser dev via the invoke seam), `pnpm fixture:operation-results` (no-backend result-view harness), `pnpm test`, `pnpm typecheck`, `pnpm verify`. Rust tests for the shell: `cargo test --manifest-path apps/palette-tauri/src-tauri/Cargo.toml`.
