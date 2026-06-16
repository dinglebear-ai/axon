# Phase 4: React 19 / Tailwind v4 / Accessibility & Standards Review

Target: `apps/palette-tauri` (axon-palette-tauri v5.9.1) — React 19.2, React DOM 19, Tailwind CSS v4 (`@tailwindcss/vite`), TypeScript 5.9, Aurora tokens, CVA + tailwind-merge + clsx, lucide-react, shadcn-style `ui/aurora/*`. UI/UX-weighted: a11y, React idioms, Tailwind v4 idioms.

**Overall verdict:** The TypeScript discipline is excellent (strict, discriminated unions, zero `any`, type-only imports) and the Tauri/React data flow is clean. But for a *command palette* — the one UI pattern with a canonical, well-documented WAI-ARIA spec — the accessibility implementation is the weakest dimension by a wide margin: the core combobox/listbox semantics are entirely absent, streamed answers are never announced to screen readers, and the custom action-switcher menu is keyboard-inoperable. Separately, Tailwind v4 is installed correctly but **barely engaged**: it's effectively a utility-class shim bolted onto a 4,005-line hand-written CSS file, with no `@theme` bridge between the two. React 19's new affordances (Actions, `useActionState`, `use`) are not adopted where they would directly simplify the manual loading/error state machine.

The a11y findings are the headline and are weighted accordingly below.

---

## Accessibility (WCAG / WAI-ARIA) — HEADLINE

### Critical

#### A11Y-C1 — Command palette has none of the combobox/listbox ARIA pattern
**Severity: Critical.** This is a command palette; the [WAI-ARIA APG combobox-with-listbox pattern](https://www.w3.org/WAI/ARIA/apg/patterns/combobox/) is the governing standard, and essentially none of it is implemented.

Current (`PaletteCommandBar.tsx:156-163` + `ActionList.tsx:31-114`):
- The `<input className="command-input">` is a bare text input — no `role="combobox"`, no `aria-expanded`, no `aria-controls` pointing at the list, no `aria-activedescendant` tracking the highlighted row.
- `ActionList` is a `<section>` containing `<div className="action-row">` rows wrapping `<button>`s. No `role="listbox"` on the container, no `role="option"` / `aria-selected` on rows. The "selected" row is purely visual (`action-row-selected` class).
- Arrow-key navigation lives in the input's `onInputKeyDown` (`App.tsx:278-303`) and moves a `selected` *index* in React state — focus never moves and nothing tells AT that the active option changed. A screen-reader user types, hears nothing about how many results matched, arrows down, and hears nothing.

This fails WCAG 4.1.2 (Name, Role, Value) and 1.3.1 (Info & Relationships). For a keyboard-first launcher this is the most important single gap.

**Recommended** — wire the input as a combobox and the list as a listbox, keeping the existing roving-index state (just expose it via `aria-activedescendant`, which is the APG-sanctioned alternative to moving DOM focus):

```tsx
// PaletteCommandBar input
<input
  role="combobox"
  aria-expanded={showActionPanel}          // list is visible
  aria-controls="palette-action-list"
  aria-activedescendant={active ? `action-${active.subcommand}` : undefined}
  aria-autocomplete="list"
  // ...existing props
/>
```

```tsx
// ActionList
<div id="palette-action-list" role="listbox" aria-label="Actions" className="action-list">
  {filtered.map((action, index) => (
    <div
      id={`action-${action.subcommand}`}
      role="option"
      aria-selected={index === selected}
      key={action.subcommand}
      /* ...existing handlers... */
    >
      {/* row content; keep the inner <button>, or flatten so the option IS the row */}
    </div>
  ))}
</div>
```

Note the section-heading `<div className="action-section-heading">` rows (`ActionList.tsx:48-53`) sit *inside* the listbox today; under `role="listbox"` only `role="option"`/`role="group"` children are valid. Wrap each category in `role="group"` with `aria-label`, or move the headings out of the listbox subtree.

#### A11Y-C2 — Streamed/async results are never announced (no live region)
**Severity: Critical** (for an async, streaming-output app). The output region (`OutputPanel.tsx:79` `<section className="output-panel">`) and the streaming bodies (`PendingBody` `:256`, `AskConversation`/`ConversationThread` `:51-62`) have **no `aria-live`**. When `ask`/`chat` streams tokens in, or a one-shot result lands, a sighted user sees it; a screen-reader user gets silence. The only `aria-live="polite"` in the whole app is the settings connection-test result (`SettingsPanel.tsx:251`).

This fails WCAG 4.1.3 (Status Messages). Streaming answer text changing token-by-token should *not* be announced per-token (that floods AT) — announce status transitions and the settled answer.

**Recommended:**
- Add a visually-hidden polite live region for run-state transitions ("Running scrape…", "Complete, 1,240 words", "Failed: …") driven off `run.kind`. One region, terse messages, not the raw streaming buffer.
- Give the answer container `aria-live="polite"` `aria-atomic="false"` only if you debounce/settle it; otherwise announce once on terminal state. The `output-status` pill (`:101`) carrying "complete"/"failed" is a natural anchor — wrap it (or a sibling sr-only node) in the live region.
- The error view already uses `role="alert"` on the invalid-help case (`HelpResultView.tsx:46`) — good; extend that to `ErrorResultView` so failures interrupt.

### High

#### A11Y-H1 — Custom action-switcher "menu" is not keyboard-operable
**Severity: High.** `PaletteCommandBar.tsx:107-152` builds a `role="menu"` / `role="menuitem"` dropdown with `aria-haspopup="menu"` + `aria-expanded`, but it only handles **pointer** events. There is no `onKeyDown`: you cannot open it with Enter/Space-then-arrow, cannot arrow between items, cannot Escape to close (the global Escape handler will fire instead and do something unrelated), and focus is never moved into the menu or restored to the trigger on close. A `role="menu"` that can't be driven by keyboard is worse than no role — it advertises a contract to AT that the implementation breaks (WCAG 2.1.1 Keyboard).

**Recommended:** either (a) implement the [APG menu-button keyboard pattern](https://www.w3.org/WAI/ARIA/apg/patterns/menu-button/) — ArrowDown opens + focuses first item, Up/Down roves `tabIndex`, Escape closes and refocuses trigger, Home/End — or (b) drop `role="menu"` and model it as a simple `aria-expanded` disclosure of a list of plain buttons (each independently Tab-focusable). Option (b) is far less code and is honest about the interaction; menus carry heavy obligations. The click-outside handler (`:66-74`) is fine; add a keydown sibling.

#### A11Y-H2 — No focus management when overlays open/close
**Severity: High.** Settings (`App.tsx:375`), History (`:404`), and result panels mount/unmount without moving focus into them or restoring focus on close, and there is no focus trap. The `<nav className="settings-tabs">` (`SettingsPanel.tsx:145`) is a tablist-shaped UI built from buttons but lacks `role="tablist"`/`role="tab"`/`aria-selected`/`tabpanel` wiring and arrow-key roving. A keyboard/AT user opening settings stays focused on the now-hidden command bar. (WCAG 2.4.3 Focus Order.)

**Recommended:** on open, move focus to the panel's first control or a focusable container (`ref.focus()` in a layout effect); on close, restore focus to the invoking control (capture `document.activeElement` or use the known trigger). For settings tabs, adopt the APG tabs pattern or relabel as plain buttons. A small `useFocusReturn(open)` hook covers all three overlays.

### Medium

#### A11Y-M1 — `focus-visible` styling is inconsistent across the two component layers
**Severity: Medium.** The `ui/aurora/*` primitives have proper `focus-visible:ring-2` rings (`button.tsx:12`, `input.tsx:139/256-263`), but the *hand-written* controls that make up most of the palette — `.command-input`, `.command-submit`, `.action-row-main`, `.idle-tray`, `.output-tools button`, the switcher trigger — are styled in `styles.css` and need verification that each has a visible, non-`:focus` (i.e. `:focus-visible`) indicator meeting WCAG 2.4.7 + 2.4.11 (Focus Appearance). The split-brain design system (Phase 1 A-H2) means focus styling is not centralized; it must be audited per hand-rolled class. (Could not confirm every selector statically — flag for a visual focus-walk.)

#### A11Y-M2 — Mode-pill icon tile + status dot convey state by color/shape only
**Severity: Medium.** `axon-status-dot axon-status-${endpointTone}` (`PaletteCommandBar.tsx:102`) signals connection state (error/syncing) purely via a colored dot with no text alternative; `submitDisabled` feedback is likewise only the dot + a `title` on the button (`:181`). Phase 1 A-M5 noted pressing Enter with a missing token does nothing visible. For AT, none of this is perceivable. **Recommended:** give the status dot an sr-only label or `aria-label` on its container ("Server: connection error"), and surface validation as inline text tied to the input via `aria-describedby` rather than only a `title` tooltip.

#### A11Y-M3 — Color-contrast of Aurora tokens not statically verifiable — needs measurement
**Severity: Medium (unverified).** Muted text `--aurora-text-muted: #a7bcc9` on surfaces like `--aurora-control-surface: #0c1a24` is likely fine, but `.action-description`, `.action-method`, `.action-endpoint`, `small` meta text, and placeholder text (`placeholder:text-[var(--aurora-text-muted)]`) are small and muted — these are the contrast risks (WCAG 1.4.3). Cannot compute ratios from source alone. **Recommended:** run an automated contrast pass (axe/Lighthouse) against the rendered palette in both `.dark` and `.light` themes; pay attention to muted-on-control-surface small text.

### Low / Good

- **GOOD — `prefers-reduced-motion` is honored** in `aurora.css:500` (the `!important` inside it is legitimate, per Phase 1 M7). The hand-written `styles.css` animations should be spot-checked that they also fall under a reduced-motion guard, but the token layer handles it.
- **GOOD — image alt text is descriptive:** the only `<img>` (`OperationResultView.tsx:418`) has `alt={\`Screenshot of ${url}\`}`. Decorative lucide icons are consistently `aria-hidden="true"` and icon-only buttons carry `aria-label` (command bar, output tools) — this is done well.
- **GOOD — native `<select>`** in `SettingsPanel` `SelectInput` (`:368`) instead of a custom div-listbox; `MiniToggle` uses `aria-pressed` (`:382`); secret reveal toggle has a stateful `aria-label` (`:358`). These are correct.
- **A11Y-L1 — semantic HTML is mostly div-driven.** Action rows, output bodies, and most panels are `<div>`/`<span>` with class names rather than semantic elements + roles. Not a violation where roles are added, but the missing roles (A11Y-C1) are the manifestation. The `<details>`/`<summary>` "More actions" menu (`OutputPanel.tsx:126-150`) is a nice native-disclosure choice.
- **A11Y-L2 — Token input lacks `autoComplete="off"`/`spellCheck={false}`** — already filed as S-I1 in Phase 2; restating here as a forms-a11y/hygiene item.

---

## React 19 Idioms

### High

#### R-H1 — Manual async state machine where React 19 Actions/`useActionState` fit
**Severity: High (idiom/maintainability).** `useActionRunner.submit` (`useActionRunner.ts`, ~230 lines per Phase 1 L1) hand-rolls the entire pending→success/error lifecycle: it imperatively `setRun({kind:"running"})`, awaits, then `setRun({kind:"success"|"error"})`, and constructs the `{ok:false,status:0,...}` error-run object 4-5× (Phase 1 L1). It also silently early-`return`s on in-flight/missing-client/failed-validation with no user feedback (Phase 1 A-M5). This is exactly the shape React 19 `useActionState` (or an Action passed to a `<form action={…}>`) was designed to absorb: pending state, error capture, and serialization of in-flight submissions are handled by the runtime.

The one-shot, non-streaming actions (`stats`, `status`, `sources`, `doctor`, the success/error path) are a clean fit. The **streaming** and **polled-job** paths (`streaming`/`job` RunState variants) genuinely need bespoke handling and should stay imperative — Actions don't model token streams or pollers. **Recommended:** migrate the one-shot request/response actions to `useActionState`, which removes the manual `running`-flag guard, the duplicated error-run factory, and naturally surfaces validation errors (fixing A-M5); keep streaming/job paths as-is. This is a refactor, not a one-liner — scope it to the non-streaming branch.

### Medium

#### R-M1 — Effect re-binds the global keydown listener on every keystroke and stream delta
**Severity: Medium** (already filed as Phase 1 H3 / Phase 2 P-H2 — restating as a React-idiom violation). `App.tsx:98-134` lists `[browseOpen, historyOpen, modeAction, query, run, ...]` as deps, so the `window` keydown listener tears down + re-adds on every character typed and every streamed token (each delta makes a new `run` object). The idiomatic React 19 fix is the **ref-for-latest-value** pattern: keep volatile state in a ref updated each render, bind the listener once with `[]`. The adjacent `blur` listener (`:92-96`) already does this correctly — mirror it. The Escape handler's 6-branch back-stack is also a candidate to lift into the `useReducer`/discriminated-view refactor (Phase 1 A-M1).

#### R-M2 — `forwardRef` is unnecessary in React 19 (ref-as-prop)
**Severity: Medium (deprecation runway).** All seven `ui/aurora` primitives use `React.forwardRef` (`button.tsx:175`, `input.tsx:89`, `scroll-area.tsx`, `spinner.tsx`, `separator.tsx`, `badge.tsx`, `kbd.tsx`). In React 19 `ref` is an ordinary prop on function components, and `forwardRef` is on the deprecation path (React has signaled a future codemod-assisted removal). For new/maintained components the idiomatic form is to accept `ref` directly:

```tsx
// React 19
function Button({ ref, className, ...props }: ButtonProps & { ref?: React.Ref<HTMLButtonElement> }) { ... }
```

Not urgent (forwardRef still works), but since these are freshly written shadcn-style primitives they should ship in the React 19 idiom rather than the React 18 one. Also note `input.tsx:109-110` uses `useImperativeHandle(ref, () => inputRef.current!)` purely to merge an internal ref with the forwarded one — with ref-as-prop a ref-merge util (or callback ref) is cleaner.

#### R-M3 — Effects that derive state / should be event-driven
**Severity: Medium.** `App.tsx:180-182` `useEffect(() => setSelected(0), [parsed.search, modeAction])` resets selection in an effect — this is the "[you might not need an effect](https://react.dev/learn/you-might-not-need-an-effect)" reset-on-prop-change anti-pattern; it causes a second render every time the query changes. Prefer deriving the clamped selection during render (the code already clamps with `Math.min(selected, …)` at `:162`, so the reset effect is partly redundant) or resetting in the same event handler that mutates `query`/`modeAction`. The theme effect (`:136-148`) and Tauri-event listeners (`:79-96`) are legitimately effects. The `30ms setTimeout` focus hack referenced by Phase 1 L3 (`paletteView.ts focusInput`) and the `document.querySelector(".command-input")`/`.action-row-selected` DOM queries (`ActionList.tsx:27`) are imperative-DOM smells that refs would replace.

### Low / Good

- **GOOD — discriminated unions done right.** `RunState` (`runState.ts:5-44`) is a clean tagged union on `kind`, and consumers narrow with `run.kind === …` / `"text" in run`. This is textbook React/TS state modeling. (The dead divergent copy in `paletteView.ts` is Phase 1 C1 — a real bug, but the canonical type is exemplary.)
- **GOOD — `useMemo` used where it pays** (`parseCommand`, `filtered`, `client`, `switcherActions`) and *not* sprayed everywhere.
- **R-L1 — array-index keys** persist at `StatusView.tsx:58,70` (Phase 1 M4) where `job_id` is available — a correctness smell on reorderable lists.
- **R-L2 — no `React.memo`/`useCallback`** on result views / inline App callbacks (Phase 2 P-M1/P-M2). Only worth doing *together* and only if streaming jank is measured — don't pre-optimize.
- **R-L3 — `<Context>` as provider / `use()`** — N/A; the app has no context (prop-drilling instead, Phase 1 A-M2). If the setter-drilling is refactored to a `dispatch`, a single `<PaletteContext value={dispatch}>` (React 19 lets you render the context directly as the provider) would be the idiomatic vehicle.

---

## Tailwind v4 Idioms

### High

#### TW-H1 — Tailwind v4 is installed correctly but functionally bypassed; no `@theme` bridge
**Severity: High (architecture/idiom).** The v4 *setup* is correct and modern: `styles.css:1` `@import "tailwindcss"`, the `@tailwindcss/vite` plugin in `vite.config.ts:3,14`, and explicit `@source` globs (`styles.css:4,9`) including the clever streamdown-dist scan so its utilities get generated. That part is exemplary v4.

**But the engagement is lopsided:**
- **Zero `@theme`** (confirmed: `grep -c '@theme'` = 0 in both `styles.css` and `aurora.css`). The Aurora design tokens are ~80 plain CSS custom properties under `:root,.dark` in `aurora.css`. Because they're not in a `@theme` block, **Tailwind has no knowledge of them** — you cannot write `bg-accent-primary` or `text-muted`; every token reference is the verbose arbitrary-value escape hatch `bg-[var(--aurora-…)]` / `text-[var(--aurora-text-primary)]` (pervasive in `button.tsx`/`input.tsx`). This is the single biggest v4 idiom miss: v4's headline feature is the CSS-first `@theme` that turns design tokens into first-class utilities.
- **Zero `@apply`, zero `@layer`, zero `@utility`** (confirmed). The 4,005-line `styles.css` is hand-written semantic CSS (`.command-bar`, `.action-row`, `.output-panel`, …) that doesn't compose Tailwind utilities at all. The two layers don't talk: `ui/aurora` primitives are utility-first; the palette shell is classic CSS. The real design system is the CSS file (Phase 1 A-H2), and Tailwind is engaged only inside the rarely-used `ui/aurora` primitives plus streamdown's own classes.

**Recommended (incremental, not a rewrite):**
1. Promote the Aurora tokens into a `@theme` block so they become real utilities:
   ```css
   @theme {
     --color-accent-primary: #29b6f6;
     --color-text-primary: #e6f4fb;
     --color-text-muted: #a7bcc9;
     /* … */
   }
   ```
   Then `button.tsx`'s `text-[var(--aurora-text-primary)]` becomes `text-text-primary`, `focus-visible:ring-[var(--aurora-accent-primary)]` becomes `focus-visible:ring-accent-primary`, etc. (Keep the `--aurora-*` aliases for the hand-written CSS during migration; `@theme` can reference them.)
2. Decide the canonical layer (Phase 1 A-H2). If utilities win, the most-repeated `styles.css` patterns are `@apply`/`@utility` candidates. If hand-CSS wins for the shell, then trim the under-used `ui/aurora` primitives. Today you pay the maintenance cost of both with the benefits of neither fully realized.

### Medium

#### TW-M1 — CVA + tailwind-merge usage is idiomatic but `tailwind-merge` can't see arbitrary-value collisions cleanly
**Severity: Medium.** `cn()` (`utils.ts:4`) = `twMerge(clsx(...))` is the canonical shadcn helper — correct. CVA in `button.tsx` is well-structured (base + variants + `defaultVariants`). **However**, the per-variant styling is split between CVA classes *and* a parallel `VARIANT_CONFIG` object of inline `style={}` + raw `hover:[box-shadow:…]` arbitrary strings (`button.tsx:70-166`). The inline-style/CVA split means `tailwind-merge` only de-dupes the className half; the `style` object and the giant arbitrary `hover:[box-shadow:…]` literals are outside its reach, so variant overrides via `className` won't reliably win over the inline `style`. This is the same "inline style to win specificity" smell flagged in `input.tsx` (`:240` comment "inline so it wins over Tailwind"). It works, but it's fighting the framework. **Recommended:** once tokens are in `@theme`, the box-shadow/gradient values can move into theme vars and be applied via utilities or a single CSS class per variant, collapsing the `VARIANT_CONFIG` inline-style table.

### Low

- **TW-L1 — `input.tsx` border arbitrary-value bug:** `tokens ? \`border-[${tokens.border}]\` : …` (`:218-220`) interpolates a CSS `var(--…)` *into a Tailwind arbitrary class at runtime*. Tailwind compiles utilities at build time from static source scanning — a runtime-templated `border-[var(--aurora-error)]` class string is not in the scanned source, so the utility is never generated and the class is inert. The component "works" only because the same color is *also* applied via inline `style.borderColor` (`:241-243`). Dead className; rely on the inline style or a static variant class. (Cosmetic, but it's a misunderstanding of v4's static-extraction model worth noting.)
- **TW-L2 — `styles.css` dead/split rules** (Phase 1 M5) and hardcoded `#06131c`×4 (Phase 1 M6) — tidiness, not idiom; folds into the `@theme` migration (M6 becomes an `--color-on-accent` theme var).

---

## TypeScript

### Good (this is the strongest dimension)
- **`tsconfig.json` is strict and modern:** `strict: true`, `noUnusedLocals`, `noUnusedParameters`, `noFallthroughCasesInSwitch`, `target: ES2022`, `moduleResolution: bundler`, `isolatedModules`, `verbatim`-friendly. Solid.
- **Zero `any` / `as any`** in non-test `src/` (confirmed via grep).
- **Type-only imports** used consistently (`import type { … }` in `runState.ts`, `actions.ts`, etc.) — correct for `isolatedModules`.
- **Discriminated `RunState`** — see R-L (Good).

### Medium
- **TS-M1 — Untyped server payloads (`Record<string, unknown>`) + defensive key-probing.** Phase 1 A-M3: `axon-api.d.ts` (generated OpenAPI types) is imported by nothing; `bodyFor` hand-builds `Record<string,unknown>`, and views probe `markdown ?? content ?? output ?? text ?? body` (`OutputPanel.tsx:210-215`). Strict mode is undermined at the I/O boundary — the one place types matter most is untyped. Completing GitHub #177 (wire `axon-api.d.ts` `paths`/`responses` into the client + result types) would eliminate the key-probing and give the discriminated unions real teeth at the network seam.
- **TS-L1 — `RunState` divergent dead copy** (`paletteView.ts`) is Phase 1 C1; a type-level footgun (`import { RunState } from "@/lib/paletteView"` compiles wrong). Delete + re-export canonical.

---

## Deprecated / Legacy APIs

- **DEP-1 — `forwardRef`** across all `ui/aurora` primitives — on React 19's deprecation runway (= R-M2). Not broken, but legacy idiom in new code.
- **DEP-2 — lucide-react `^1.16.0` is CURRENT, not old — prior-phase concern retracted.** I verified the lockfile: `lucide-react@1.16.0` resolves with a valid integrity hash and a real `react@19.2.6` peer. lucide-react renumbered to a 1.x line; `1.16.0` is a recent, supported release, **not** "a very old major." No action — flag this as a *non-finding* correcting the scope note.
- **DEP-3 — `"use client"` directive in `input.tsx:1`** is a copy-paste artifact from a Next.js/RSC origin. It's a no-op (harmless string) in a Vite/Tauri SPA with no Server Components. Cosmetic; remove for honesty. None of the other primitives carry it.
- No deprecated React APIs (no legacy context, `componentWillMount`, string refs, `ReactDOM.render` — `main.tsx` uses `createRoot`).

---

## Build Config

- **GOOD — `vite.config.ts` is clean:** `@tailwindcss/vite` + `@vitejs/plugin-react`, `@`-alias, and a well-documented dev proxy that injects the bearer token server-side so it never ships in the bundle (`:6-32`). Tauri-aware (`clearScreen: false`, strict port).
- **BUILD-H1 — No code-splitting / `manualChunks`** (= Phase 2 P-H1). shiki + streamdown are pulled into the main chunk and the highlighter is instantiated at module-eval on the startup path, even though a fresh palette shows no markdown. `vite.config.ts` has no `build.rollupOptions.output.manualChunks`, and there is no `React.lazy`/dynamic import anywhere in `src/`. For a launcher whose time-to-interactive is the whole UX, lazy-load the markdown body behind `<Suspense>` and split shiki/streamdown into their own chunk. **Largest perceived-perf win** (cross-referenced, not re-derived).
- **BUILD-L1 — `tsconfig` has no project-references / no separate `tsconfig.node.json`** for the Vite config itself; minor, the single config compiles fine under `bundler` resolution.

---

## Priority Order (UI/UX-weighted)

1. **A11Y-C1** — combobox/listbox ARIA on the command input + action list. The defining gap for a command palette.
2. **A11Y-C2** — live region for streamed/async results so non-sighted users hear output.
3. **A11Y-H1 / A11Y-H2** — make the switcher menu keyboard-operable (or demote it to a disclosure); add focus management/return for overlays.
4. **TW-H1** — introduce a `@theme` token bridge so Tailwind v4 is actually engaged; pick the canonical layer.
5. **R-H1** — migrate one-shot actions to `useActionState` (removes the manual loading machine + surfaces validation, fixing A-M5).
6. **TS-M1 / BUILD-H1** — complete OpenAPI typing (#177); lazy-split shiki/streamdown.
7. Lower: `forwardRef`→ref-as-prop, reset-effect→derived state, `"use client"` removal, contrast measurement pass.

### Corrections to prior-phase scope
- The "lucide-react ^1.16.0 (very old major — flag)" scope note is **incorrect**: 1.16.0 is a current, valid release. Retracted (DEP-2).
- Phase context asked whether Tailwind v4 is "barely engaged" — **confirmed yes**: correct entrypoint, but zero `@theme`/`@apply`/`@layer`/`@utility`, tokens invisible to Tailwind, and a 4,005-line parallel hand-written CSS file is the real design system (TW-H1).
