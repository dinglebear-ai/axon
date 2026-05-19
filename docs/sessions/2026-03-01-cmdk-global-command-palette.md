# Session: Global CMD+K Command Palette
Date: 2026-03-01 | Branch: feat/sidebar

## Session Overview

Implemented a floating global CMD+K command palette accessible from every page in the web app. The Omnibox on the landing page was unreachable from Cortex, Editor, Jobs, Terminal, and other routes — this palette fills that gap. Also performed a full design system alignment pass and extended the token system with three new tokens to cover a gap in the surface tier.

---

## Timeline

1. **Codebase orientation** — read `ws-protocol.ts` (MODES, NO_INPUT_MODES, WS message types), `use-axon-ws.ts` (send/subscribe API), `app-shell.tsx` (mount point), `omnibox.tsx` (existing patterns), `globals.css` (tokens, animations)
2. **Initial implementation** — created `CmdKPalette.tsx`, `CmdKOutput.tsx`, `index.ts`; mounted in `AppShell`
3. **Design system alignment** — read `docs/UI-DESIGN-SYSTEM.md`; identified every raw `rgba()` / raw hex / inline `animation:` violation
4. **Token extension** — added `--surface-primary`, `--surface-primary-active`, `--axon-success-border` to close the cyan-tinted interactive surface gap
5. **Animation compliance** — moved `cmdk-palette-in` keyframe into `globals.css` as `@utility animate-cmdk-in`

---

## Key Findings

- `cmdk` v1.1.1 was already installed — `<Command>`, `<Command.Input>`, `<Command.List>`, `<Command.Group>`, `<Command.Item>` all available
- `useAxonWs()` returns `{ send, subscribe }` — `subscribe()` returns an unsubscribe function; exec_id arrives in `command.start` message
- Design system gap: `--surface-*` tokens (`rgba(10,18,35,…)`) are invisible on modal panels with `rgba(10,18,35,0.97)` background — no existing token for cyan-tinted interactive highlights
- `@keyframes scale-in` in `globals.css` uses `transform: scale(…)` which conflicts with `transform: translate(-50%,-50%)` positioning — needed a custom keyframe that bakes translate into the animation with `animation-fill-mode: forwards`
- Biome linter added `type="button"` to buttons and `biome-ignore` comments for `noStaticElementInteractions` on the backdrop div (valid UX modal pattern)

---

## Technical Decisions

| Decision | Rationale |
|----------|-----------|
| Portal into `document.body` via `createPortal` | Avoids z-index stacking context issues from page layouts |
| `animation-fill-mode: forwards` on `animate-cmdk-in` | Bakes `translate(-50%,-50%)` into keyframe `to` state — avoids CSS `translate` property (React type ambiguity with HTML `translate` attribute) |
| Exec-id filtering on WS messages | Palette can open while another WS command is running; without filtering, output from concurrent commands bleeds in |
| `animate-breathing` for running dot | Reuses existing `globals.css` utility instead of new keyframe |
| Only `content` + `rag` categories shown | User's 9 commands; keeps list short and focused |
| `--surface-primary` / `--surface-primary-active` added to design system | Closes structural gap — cyan-tinted interactive states were consistently raw rgba throughout the codebase |

---

## Files Modified

| File | Action | Purpose |
|------|--------|---------|
| `apps/web/components/cmdk-palette/CmdKPalette.tsx` | Created | Main palette — portal shell, state machine (idle→select→input→running→done), cmdk command list, input view |
| `apps/web/components/cmdk-palette/CmdKOutput.tsx` | Created | Running + done views — streaming log lines, async progress bar, exit badge, dismiss/view-job actions |
| `apps/web/components/cmdk-palette/index.ts` | Created | Barrel export |
| `apps/web/components/app-shell.tsx` | Modified | Added `<CmdKPalette />` mount + import |
| `apps/web/app/globals.css` | Modified | Added `@keyframes cmdk-palette-in` + `@utility animate-cmdk-in`; added `--surface-primary`, `--surface-primary-active`, `--axon-success-border` tokens |
| `docs/UI-DESIGN-SYSTEM.md` | Modified | Documented new surface tokens, updated status color table, extended "What to Avoid" anti-pattern table |

---

## State Machine

```
idle → (Cmd/Ctrl+K) → select
select → (choose no-input command) → running   [skip input step]
select → (choose input command) → input
input → (Enter) → running
running → (command.done / command.error) → done
done → (Escape / Cmd+K / click outside) → idle
running → (Escape) → send cancel → idle
input → (Escape) → back → select
select → (Escape) → idle
```

---

## Design System Changes

### New tokens added to `globals.css` + `UI-DESIGN-SYSTEM.md`

```css
/* Cyan-tinted interactive surfaces — for item highlights on dark panels */
--surface-primary:        rgba(135, 175, 255, 0.08)
--surface-primary-active: rgba(175, 215, 255, 0.12)

/* Completes the success color family */
--axon-success-border: rgba(130, 217, 160, 0.30)
```

### Token violations fixed in palette (representative list)

| Raw value | Token used |
|-----------|-----------|
| `rgba(5,10,22,0.92)` | `rgba(10,18,35,0.97)` (modal standard) |
| `rgba(135,175,255,0.20)` border | `var(--border-standard)` |
| Custom inline `@keyframes` | `globals.css` + `@utility animate-cmdk-in` |
| `rgba(135,175,255,0.35)` text | `var(--text-dim)` |
| `rgba(184,207,224,0.80)` text | `var(--text-secondary)` |
| `rgba(100,220,140,0.9)` success | `var(--axon-success)` |
| `rgba(100,220,140,0.10)` | `var(--axon-success-bg)` |
| `rgba(130,217,160,0.30)` | `var(--axon-success-border)` |
| `rgba(255,135,175,0.10)` | `var(--axon-danger-bg)` |
| `rgba(175,215,255,0.08)` item hover | `var(--surface-primary)` |
| `rgba(175,215,255,0.12)` item active | `var(--surface-primary-active)` |

---

## Behavior Changes (Before → After)

| Surface | Before | After |
|---------|--------|-------|
| Any non-landing page | No way to run axon commands | CMD+K opens command palette |
| Palette trigger | N/A | `Cmd+K` / `Ctrl+K` from anywhere |
| Command filter | N/A | Type to filter via cmdk built-in |
| Sync commands (scrape, ask, query…) | N/A | Streaming log lines in palette |
| Async commands (crawl, embed…) | N/A | Phase + percent progress bar |
| Running state | N/A | Escape sends cancel message |
| Done state | N/A | Elapsed time, exit badge, View Job link |

---

## Verification Evidence

| Check | Expected | Actual | Status |
|-------|----------|--------|--------|
| `npx tsc --noEmit` | No errors in palette files | Clean (only pre-existing api-copilot test error) | ✅ |
| `cmdk` installed | v1.1.1 in package.json | Confirmed | ✅ |
| Token: `--surface-primary` | Defined in globals.css | Added at line ~139 | ✅ |
| Token: `--surface-primary-active` | Defined in globals.css | Added | ✅ |
| Token: `--axon-success-border` | Defined in globals.css | Added | ✅ |
| Animation: `animate-cmdk-in` | `@utility` in globals.css | Added | ✅ |
| Raw rgba remaining | 0 in palette files | 1 decorative box-shadow glow (acceptable) | ✅ |

---

## Risks and Rollback

- **Portal rendering**: If `document.body` is not available (SSR), `createPortal` is guarded by `mounted` state — no SSR risk
- **WS exec-id race**: Subscribe fires before `command.start` arrives; `execIdRef` starts null and only filters once set — messages before capture are accepted (safe: they belong to this command)
- **Rollback**: Remove `<CmdKPalette />` from `app-shell.tsx` and delete `components/cmdk-palette/` directory; new CSS tokens are additive and safe to leave

---

## Decisions Not Taken

| Alternative | Reason rejected |
|-------------|----------------|
| `Radix UI Dialog` instead of custom portal | cmdk already handles a11y for list navigation; adding Radix would add a dependency layer without benefit |
| Reuse `--surface-float` for item hover | `rgba(10,18,35,0.35)` is invisible on `rgba(10,18,35,0.97)` panel — wrong tier for this context |
| `CSS translate` property for centering | React's `CSSProperties.translate` is typed as string (HTML attribute conflict risk); baking translate into keyframe `to` state with `forwards` is simpler |
| Show all 5 MODES categories in palette | User wanted the 9 commands (content + rag); other categories (ingest, ops, service) add noise |

---

## Open Questions

- Should the palette show a keyboard hint in the footer (e.g., `↑↓ navigate · Enter select · Esc back`)?
- Should `--surface-primary` / `--surface-primary-active` be applied retroactively to other components that currently use raw rgba (omnibox mode chips, command-options-panel)?

---

## Next Steps

- Retroactive token sweep: replace remaining raw `rgba(175,215,255,…)` occurrences in `omnibox.tsx` and `command-options-panel.tsx` with new tokens
- Consider adding keyboard hint footer to the select phase
- Verify palette behavior in the deployed container (especially WS cancel message delivery)
