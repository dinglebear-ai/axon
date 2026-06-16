# Review Scope

## Target

The Tauri desktop **command palette** application at `apps/palette-tauri`. This is the `axon-palette-tauri` desktop GUI (v5.9.1) — a Raycast/Spotlight-style command palette frontend for the Axon RAG engine. The review is **scoped specifically to UI/UX**: layout, interaction design, accessibility (a11y), visual consistency, responsiveness, keyboard/focus handling, theming (Aurora design tokens), and frontend code quality. Backend/security is de-emphasized unless it directly affects the UI.

## Stack

- React 19.2 + React DOM
- Tauri 2.10 (desktop shell; Rust backend in `src-tauri/`)
- Tailwind CSS v4 (`@tailwindcss/vite`)
- Aurora design system (shadcn-compatible registry, `var(--aurora-*)` tokens)
- shiki + streamdown (markdown/code rendering)
- class-variance-authority + tailwind-merge + clsx (styling primitives)
- Vitest + Testing Library (tests)

## Files (UI/UX-relevant frontend surface)

### App shell & entry
- `src/App.tsx` (504 lines — main palette orchestration)
- `src/main.tsx`, `index.html`
- `src/styles.css` (4005 lines — global + Tailwind + Aurora theme)
- `src/components/aurora.css` (508 lines)
- `src/fonts.css`

### Palette components (`src/components/palette/`)
- `PaletteCommandBar.tsx` (196) — command input bar
- `ActionList.tsx` (115) / `ActionIcon.tsx` (83) — action picker list
- `OutputPanel.tsx` (342) — result output container
- `OperationResultView.tsx` (466) / `OperationResultViewShared.tsx` (412) / `OperationResultFixture.tsx` (341)
- `SettingsPanel.tsx` (486) — settings UI
- `CrawlJobView.tsx` (228), `AskConversation.tsx` (105), `HistoryPanel.tsx` (68)
- `StatusView.tsx` (78), `StatsView.tsx` (97), `EvaluateView.tsx` (67)
- `HelpResultView.tsx` (116), `ErrorResultView.tsx` (127)
- `PaletteFooter.tsx` (44), `AxonMark.tsx` (22)

### Aurora UI primitives (`src/components/ui/aurora/`)
- `button.tsx`, `input.tsx`, `badge.tsx`, `kbd.tsx`, `scroll-area.tsx`, `separator.tsx`, `spinner.tsx`, `status-indicator.tsx`
- `src/components/ui/spinner.tsx`

### Supporting lib (`src/lib/`)
- `useWindowChrome.ts`, `useActionRunner.ts`, `useCrawlJob.ts` (hooks)
- `paletteView.ts`, `runState.ts`, `actions.ts`, `actionMeta.ts`, `actionHelp.ts`
- `format.ts`, `payload.ts`, `url.ts`, `utils.ts`, `streamdownConfig.ts`, `limitedStreamdownCode.ts`

## Flags

- Security Focus: no
- Performance Critical: no
- Strict Mode: no
- Framework: React 19 + Tauri 2 + Tailwind v4 + Aurora (auto-detected)

## Review Phases

1. Code Quality & Architecture (UI/component focus)
2. Security & Performance (frontend render perf focus)
3. Testing & Documentation
4. Best Practices & Standards (React/Tailwind/a11y focus)
5. Consolidated Report
