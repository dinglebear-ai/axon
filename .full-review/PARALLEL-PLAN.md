# Parallel Remediation Plan — Palette UI/UX Review

Worktree: `/home/jmagar/workspace/axon/.worktrees/palette-ui-ux-fixes`
Branch: `claude/palette-ui-ux-fixes`
App root: `apps/palette-tauri/`

## Conflict-avoidance rule (MANDATORY)

Each lane **owns a disjoint set of files**. An agent edits ONLY files in its lane. It must NOT edit any file owned by another lane. Cross-file consolidations are made safe by the **Shared API Contract** below: every lane codes against these fixed signatures/paths, so no live coordination is needed. Do **not** run `pnpm install`, `pnpm build`, `cargo build`, or tests — the orchestrator verifies centrally after all lanes finish (there is no `node_modules` here).

## Out of scope (deferred to follow-up issues — do NOT attempt)

- **A-H1** — action-behavior registry consolidation (the 7-file subcommand dispatch). Leave the switches as-is.
- **A-M1 / A-M2** — App.tsx view-state `useReducer`/discriminated-view refactor + setter-drilling dissolution. Keep the existing `useState` view flags; only make the *targeted* fixes listed per lane.

---

## Shared API Contract (all lanes code against these — fixed, do not deviate)

Canonical helper homes (Lane L creates/owns these; all other lanes IMPORT from them and DELETE their local copies):

```ts
// src/lib/url.ts  (revived as canonical home — no longer dead)
export function hostLabel(url: string): string;   // uses new URL(url).hostname; fallback url.split("/")[0] || url
export function firstUrl(text: string): string | null; // first http(s) URL in text, else null

// src/lib/payload.ts  (already exports isRecord; add the rest)
export function isRecord(v: unknown): v is Record<string, unknown>; // existing
export function firstArray(v: unknown): unknown[] | null; // first array found among record values, else null (preserve OperationResultViewShared semantics)
export function shortId(value: string): string; // if value.length > 12 → value.slice(0,12) + "…", else value. PURE truncation; callers guard empties.
export function titleCase(s: string): string; // capitalize each word at /\b\w/ boundaries (treat / and - as boundaries)

// src/lib/format.ts
export const MIN_PROGRESS_PCT = 2; // min visible progress-bar width %; consumed by App.tsx tray + CrawlJobView
```

Caller guidance for `shortId` (since semantics were unified to pure truncation):
- StatusView: `id ? shortId(id) : "—"`
- OperationResultViewShared: `id ? shortId(id) : undefined`

Test infrastructure (Lane B creates/owns; all test-writing lanes ASSUME these globals exist — do NOT stub them yourself):
- `apps/palette-tauri/src/test/setup.ts` registered via `vitest` `test.setupFiles`, providing: `@testing-library/jest-dom/vitest` matchers, `jest-axe`'s `toHaveNoViolations`, and DOM polyfills (`matchMedia`, `scrollIntoView`, `ResizeObserver`).
- New devDeps Lane B adds to `package.json`: `@testing-library/user-event`, `jest-axe`, `@types/jest-axe`, `@biomejs/biome`.
- Test-writing lanes: write `*.test.ts(x)` files importing `@testing-library/user-event` and `jest-axe` freely; the orchestrator installs deps before running.

Streamdown hardening (Lane R owns `streamdownConfig.ts`): export a single hardened `rehypePlugins` array (re-includes `sanitize` since overriding replaces the default array) with `allowedImagePrefixes: []`, `allowDataImages: false`, `allowedProtocols: ["http","https","mailto"]`, and pass it to every `<Streamdown>`.

---

## Lanes (file ownership + findings)

### Lane L — Shared lib (helpers, types, constants)
**Owns:** `src/lib/url.ts`, `payload.ts`, `paletteView.ts`, `runState.ts`, `format.ts`, `actions.ts`, `actionMeta.ts`, `actionHelp.ts`, `historyRun.ts`, `configModel.ts`, `appHelpers.ts`, `shellWords.ts`, `axonClient.ts`, `invoke.ts` + their `*.test.ts`.
**Findings:** C1 (delete dead `RunState` + `runTone`/`outputTitle`/`outputSubtitle` from paletteView; re-export canonical `RunState` from runState.ts; preserve all LIVE exports' signatures — grep usages first), H1 (implement `hostLabel`/`firstUrl` in url.ts; remove paletteView's local copies, re-export if any external importer), H2 (implement `firstArray`/`shortId`/`titleCase` in payload.ts), M3 (`MIN_PROGRESS_PCT` in format.ts), L3 (name the 30ms constant `FOCUS_DELAY_MS` in paletteView.focusInput), A-M3 *light* (type `axonClient.bodyFor`'s return; add exported response type aliases — ADDITIVE only, do NOT change shapes consumers read).

### Lane A — Command bar + action list + App a11y/perf
**Owns:** `src/App.tsx`, `components/palette/PaletteCommandBar.tsx`, `ActionList.tsx`, `ActionIcon.tsx`, `PaletteFooter.tsx`, `AxonMark.tsx`, new `src/lib/useFocusReturn.ts`, + new tests `App.test.tsx` additions, `PaletteCommandBar.test.tsx`, `ActionList.test.tsx`.
**Findings:** A11Y-C1 (combobox/listbox: input `role=combobox`+`aria-expanded`+`aria-controls`+`aria-activedescendant`+`aria-autocomplete=list`; list `role=listbox`, rows `role=option`+`aria-selected`+stable `id`; wrap section headings in `role=group`), A11Y-H1 (switcher menu keyboard ops OR demote to `aria-expanded` disclosure of plain buttons — prefer disclosure), A11Y-M2 (sr-only label on status dot; validation via `aria-describedby` text), A11Y-H2 (App side: focus into overlays on open + restore on close via new `useFocusReturn`), H3/P-H2/R-M1 (keydown listener → ref-for-latest-value, bind once `[]`), L2 (App.tsx:471 comma-operator → if/else), R-M3 (App.tsx reset-selection effect → derive in render), P-M2 (`useCallback` the handlers passed to OutputPanel/CommandBar/ActionList), ActionList L3 (replace `.action-row-selected` query with ref), M3 (import `MIN_PROGRESS_PCT`). Import any helpers from canonical homes.
**Tests:** T-C1 (jest-axe combobox + ARIA-state), T-H1 (userEvent keyboard nav: Arrow/Tab/Escape + enter-mode-vs-submit), T-M4 (switcher menu), T-L3 (use `userEvent`).

### Lane R — Result rendering + streamdown security + render perf
**Owns:** `components/palette/OutputPanel.tsx`, `OperationResultView.tsx`, `OperationResultViewShared.tsx`, `StatusView.tsx`, `StatsView.tsx`, `EvaluateView.tsx`, `HelpResultView.tsx`, `ErrorResultView.tsx`, `CrawlJobView.tsx`, `AskConversation.tsx`, `OperationResultFixture.tsx`, new `MarkdownBody.tsx`, `src/lib/streamdownConfig.ts`, `src/lib/limitedStreamdownCode.ts` + `OperationResultView.test.tsx` and new component tests.
**Findings:** H1/H2 (delete local `hostLabel`/`firstUrl`/`firstArray`/`shortId`/`titleCase`; import from url.ts/payload.ts per contract; apply shortId caller-guard), M2 (keep OutputPanel's real-typed `outputTitle`/`outputSubtitle` — they're correct vs the dead paletteView copies Lane L removes; no cross-file change needed), S-M1/S-M2/S-L1 (hardened `rehypePlugins` in streamdownConfig, applied at all `<Streamdown>`; link prefix policy), S-L2 (drop dead `file://` branch in `imagePreviewSrc`; prefer Tauri `asset:`), A11Y-C2 (polite `aria-live` live region for run-state transitions; terse, not per-token), T-M1 (add `role="alert"` to ErrorResultView), M3 (CrawlJobView import `MIN_PROGRESS_PCT`), M4 (StatusView stable `job_id` keys), L4 (OperationResultView: derive `hasStructuredOperationView` from a single map so allowlist+switch can't drift), P-M1 (`React.memo` result views; `useMemo` `firstUrl`/`readingHeaderSummary`; fix O(n²) by short-circuiting once a URL is found), P-H1 (extract markdown render into new `MarkdownBody.tsx`, load via `React.lazy`+`<Suspense>`; make highlighter lazy inside `limitedStreamdownCode.ts` — keep its public fn signature stable), D-M2 (header comment in OperationResultFixture).
**Tests:** T-M2 (render each structured view from fixture payloads), T-M3 (streamdown sanitization regression: `<script>`/`javascript:`/`onerror`/remote-img stripped), T-H2 (streaming render: emit synthetic stream events → assert transitions), T-L1 (fake timers for the 1200ms `copied` flash).

### Lane S — Settings panel
**Owns:** `components/palette/SettingsPanel.tsx`, `SettingsPanel.test.tsx`, new `components/palette/SettingsFields.tsx`.
**Findings:** L5 (extract `TextInput`/`SecretInput`/`SelectInput`/`MiniToggle` into new palette-local `SettingsFields.tsx` — NOT into ui/aurora), A11Y-H2 (settings tabs: `role="tablist"`/`role="tab"`/`aria-selected`/`tabpanel` + arrow roving, OR relabel as plain buttons), S-I1 (token input `autoComplete="off"` `autoCorrect="off"` `spellCheck={false}` `data-1p-ignore`).
**Tests:** T-H3 (real render: type server URL → assert onChange; click Save → assert onSave; toggle a switch).

### Lane U — Aurora primitives + Tailwind theme + CSS
**Owns:** `components/ui/aurora/*.tsx`, `components/ui/spinner.tsx`, `src/components/aurora.css`, `src/styles.css`, `src/lib/utils.ts`.
**Findings:** R-M2 (all primitives `forwardRef` → ref-as-prop; simplify `input.tsx` `useImperativeHandle`), TW-M1 (collapse `button.tsx` `VARIANT_CONFIG` inline-style/box-shadow into CSS/theme-driven classes where feasible), TW-L1 (fix `input.tsx:218-220` runtime-interpolated arbitrary class — rely on inline style or static variant class), DEP-3 (remove `"use client"` from input.tsx), TW-H1 (add `@theme` block bridging the ~80 Aurora tokens so `text-text-primary`/`ring-accent-primary` etc. work; KEEP `--aurora-*` aliases — additive, don't break existing `bg-[var(--aurora-*)]`), M5 (delete dead CSS: `.ask-tool-row`/`.ask-activity`/`.ask-code-mini`/`.command-action-empty` families; consolidate split-definition selectors), M6 (introduce `--aurora-on-accent` token, replace the 4 hardcoded `#06131c`), M7 (resolve action-row `!important` cluster via selector specificity; keep reduced-motion `!important`), A11Y-M1 (ensure every hand-written interactive control — `.command-input`/`.command-submit`/`.action-row-main`/`.idle-tray`/`.output-tools button`/switcher trigger — has a visible `:focus-visible` ring), D-M1 (comment at the `.action-scroll` `max-height: min(338px,…)` rule: "338px mirrors LIST_CAP in useWindowChrome.ts — keep in sync").
**Do NOT** change class names that markup in other lanes relies on (additive CSS only; renames are out of scope).

### Lane H — Hooks
**Owns:** `src/lib/useActionRunner.ts`, `useActionRunner.test.tsx`, `src/lib/useWindowChrome.ts`, `useWindowChrome.test.ts`, `src/lib/useCrawlJob.ts`.
**Findings:** L1 (extract `makeErrorRun`/`makeStreamErrorRun` factory; split crawl/stream branches out of the 230-line `submit`), R-H1 (migrate the **one-shot, non-streaming** request/response actions to React 19 `useActionState`; KEEP streaming + polled-job paths imperative), A-M5 (surface a transient `error` RunState when `!client`/missing config so Enter isn't a silent no-op), A-M4 (give streaming terminal states their own shape OR make `result` optional rather than fabricating `{status:200}`), useWindowChrome: name the magic window-size constants + add the reciprocal `// LIST_CAP mirrors .action-scroll max-height (styles.css)` note, L3 (replace `.action-scroll-viewport` query with a ref if feasible), useCrawlJob T-L1 (fake-timer the 1Hz poll in a test).
**Tests:** extend `useActionRunner.test.tsx` (useActionState path, error surfacing), keep `useWindowChrome.test.ts` green.

### Lane B — Build, deps, CI, test infra
**Owns:** `apps/palette-tauri/vite.config.ts`, `package.json`, `tsconfig.json`, `index.html`, new `biome.json`, new `src/test/setup.ts`, `.github/workflows/ci.yml`.
**Findings:** P-H1 build-side (`build.rollupOptions.output.manualChunks` for `shiki`/`streamdown`; set `build.target` to the WebView floor), CI-H1 (add `tauri build --no-bundle` step to the palette CI job before tagging), CI-H2 (add Biome: `biome.json` with `react-hooks`-equivalent + a11y lint where supported, `lint`/`format` scripts, fold `lint` into `verify`, add CI lint step), CI-H3 (add `cargo clippy --manifest-path apps/palette-tauri/src-tauri/Cargo.toml --all-targets --locked -- -D warnings` + `cargo fmt --check` steps), CI-M1 (`pnpm generate:api && git diff --exit-code src/lib/axon-api.d.ts` step), CI-M2 (done via manualChunks), CI-M3 (`test.setupFiles` + modest `coverage.thresholds` floor on lib layer), deps (add `@testing-library/user-event`, `jest-axe`, `@types/jest-axe`, `@biomejs/biome`), P-M3-part (preload Manrope in index.html), CI-L1 (have CI call `pnpm verify` or comment the duplication). Create `src/test/setup.ts` per the contract.

### Lane T — Tauri Rust
**Owns:** `apps/palette-tauri/src-tauri/src/persistence.rs` (+ its `_tests.rs` if needed).
**Findings:** S-L3 (use `OpenOptions::mode(0o600)` via `std::os::unix::fs::OpenOptionsExt` so the secret tmp file is created with the restrictive mode atomically — eliminate the umask window). Keep it minimal and behavior-preserving.

### Lane D — Documentation
**Owns:** new `apps/palette-tauri/CLAUDE.md` (+ `AGENTS.md`/`GEMINI.md` symlinks), `apps/palette-tauri/README.md`.
**Findings:** D-H1 (CLAUDE.md: data-flow/architecture overview, deliberate App.tsx orchestrator note, co-located `*.test.ts(x)` convention vs the Rust sidecar rule = D-M4), D-H2 (declare canonical design-system layer + one-line decision rule; note token discipline is good, only component-abstraction was ambiguous; mention the `@theme` bridge), D-H3 (the "how to add an action" ordered checklist of the ~9 edit sites — note the registry refactor is a tracked follow-up), D-M3 (README: state that generated `axon-api.d.ts` is not yet consumed / responses are key-probed, tracked in #177), D-L1 (README: `pnpm dev` no-backend failure mode + point to fixture mode), D-L2 (README: add `pnpm vite:dev` to commands), D-L3 (README: `@aurora` registry install path). Create the symlinks with `ln -sf CLAUDE.md AGENTS.md && ln -sf CLAUDE.md GEMINI.md`.

---

## Integration (orchestrator, after all lanes)
1. `pnpm install` (picks up Lane B's new deps).
2. `pnpm typecheck` → fix any contract mismatches.
3. `pnpm test` → fix failures.
4. `pnpm lint` (biome) + `pnpm vite:build`.
5. `cargo fmt`/`cargo check` for src-tauri.
6. Commit per-lane or grouped, push, then PR review pass.
