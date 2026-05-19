# Session: Design System Full Alignment Pass
**Date:** 2026-02-28
**Branch:** `feat/crawl-download-pack`
**Working Directory:** `apps/web`

---

## Session Overview

Completed a full design system alignment pass across the entire Next.js web app. The goal was to eliminate all structural misuses of raw pink rgba values (`rgba(255,135,175,*)`) and stale/generic shadcn tokens (`bg-border`, `text-muted-foreground`, `caret-primary`, `selection:bg-brand/*`) and replace them with the v2 design system token vocabulary. Plate.js editor components were also aligned. Build verified clean (22 routes, 0 errors) after all changes.

---

## Timeline

1. **Plate.js editor alignment** — Audited 8 editor-related files; replaced stale v1 tokens and shadcn defaults in CSS and component code.
2. **Comprehensive sweep** — Grep'd all `components/` and `app/` `.tsx` files for pink rgba and stale tokens; categorized every instance as intentional brand vs. structural misuse.
3. **Structural fixes** — Fixed 13 component files covering borders, separators, hover states, focus rings, icon colors, and text colors.
4. **Final cleanup** — Fixed 2 trailing files (`dropdown-menu.tsx`, `toolbar.tsx`) identified at end of previous session.
5. **Build verification** — `pnpm build` confirmed 22 routes, 0 TypeScript errors.

---

## Key Findings

- `--axon-accent-pink` (v1) was actually `#afd7ff` (blue), not pink — using it as `color: var(--axon-accent-pink)` in `.axon-editor code` was already correct but non-obvious; replaced with `--axon-primary` for clarity.
- `bg-border` in shadcn primitives resolves to an undefined/light-theme value in our dark setup; must always be `bg-[var(--border-subtle)]`.
- `selection:bg-brand/25` and `caret-primary` are shadcn brand tokens that don't resolve in our theme; replaced with direct rgba/token values.
- Remaining pink rgba instances are all **intentionally** semantic: error status (doctor-report), secondary info chips (pulse-chat), stdio type badge (mcp/components). None are structural.
- `[&_svg:not([class*='text-'])]:text-muted-foreground` pattern in shadcn dropdown primitives also needed tokenization.

---

## Technical Decisions

- **`--border-subtle` for idle structural borders** (not `--border-standard`) — subtle is less visually heavy for dividers and inactive input rings.
- **`--focus-ring-color` for active/focus states** — consistently blue (`rgba(135,175,255,0.5)`), replacing both incorrect pink focus rings and various raw blue rgba values.
- **Pink rgba kept** for doctor-report error border, pulse-chat source chips, and MCP stdio badge — these are semantic color usages tied to meaning, not chrome.
- **SVG fallback tokens** (`[&_svg:not([class*='text-'])]`) in dropdown-menu primitives updated to `text-[var(--text-dim)]` for consistency in icon rendering.
- **Transport active bg** in `mcp/components.tsx` changed from `rgba(255,135,175,0.12)` to `rgba(175,215,255,0.12)` (blue-tinted) to match active state language.

---

## Files Modified

### Plate.js Editor Components
| File | Change |
|------|--------|
| `app/globals.css` (`.axon-editor` block, lines 543-582) | 4 stale v1 tokens → `--text-dim`, `--axon-primary`, `--text-primary`, `--border-subtle` |
| `components/ui/floating-toolbar.tsx` | Pink border → `var(--border-standard)` |
| `components/ui/toolbar.tsx` | Separator `bg-border` → `var(--border-subtle)`; tooltip border; 2× ChevronDown + DropdownMenuLabel `text-muted-foreground` → `text-[var(--text-dim)]` |
| `components/ui/editor.tsx` | `caret-primary`, `selection:bg-brand/25`, selection area brand tokens, placeholder `text-muted-foreground` |
| `components/ui/editor-static.tsx` | Placeholder `text-muted-foreground` → `text-[var(--text-dim)]` |
| `components/ui/editor-context-menu.tsx` | Border + hover bg: pink → `var(--border-standard)` / `var(--surface-elevated)` |
| `components/ui/separator.tsx` | `bg-border` → `bg-[var(--border-subtle)]` |
| `components/ui/ghost-text.tsx` | `text-muted-foreground/70` → `text-[var(--text-dim)] opacity-70` |

### Structural Pink Sweep
| File | Change |
|------|--------|
| `components/pulse/pulse-toolbar.tsx` | 7 icon button borders + 3 dividers: pink → `var(--border-subtle)` |
| `components/pulse/pulse-workspace.tsx` | Panel border: pink → `var(--border-subtle)` |
| `components/command-options-panel.tsx` | Input/select borders + pink focus rings → tokens |
| `components/recent-sessions.tsx` | Hover/loading inline styles: pink rgba → blue rgba |
| `app/settings/page.tsx` | Toggle, inputs, selects, textarea, kbd borders: pink → tokens |
| `app/mcp/components.tsx` | `INPUT_CLS`, textarea, hover card, transport toggle: pink → tokens; active bg → blue |
| `app/page.tsx` | 3 nav icon borders + omnibox inline style: pink → `var(--border-subtle)` |
| `app/mcp/page.tsx` | Header divider: pink → `var(--border-subtle)` |
| `components/pulse/pulse-chat-pane.tsx` | Citation card border + hover: pink → `var(--border-subtle)` / `var(--focus-ring-color)` |

### Final Cleanup (this session)
| File | Change |
|------|--------|
| `components/ui/dropdown-menu.tsx` | Separator `bg-border` → `bg-[var(--border-subtle)]`; SVG fallback colors + shortcut text `text-muted-foreground` → `text-[var(--text-dim)]` |
| `components/ui/toolbar.tsx` | 2× ChevronDown + DropdownMenuLabel `text-muted-foreground` → `text-[var(--text-dim)]` |

---

## Commands Executed

```bash
# Final grep audit confirming all structural pink resolved
grep -rn "rgba(255,135,175" components/ app/ --include="*.tsx" | ...

# Bulk replacement in toolbar.tsx
sed -i 's/className="size-3\.5 text-muted-foreground" data-icon/className="size-3.5 text-[var(--text-dim)]" data-icon/g' components/ui/toolbar.tsx

# SVG fallback fix in dropdown-menu.tsx
sed -i "s/\[&_svg:not(\[class\*='text-'\])\]:text-muted-foreground/.../g" components/ui/dropdown-menu.tsx

# Build verification
pnpm build
# Result: 22 routes, 0 errors
```

---

## Behavior Changes (Before → After)

| Element | Before | After |
|---------|--------|-------|
| Dropdown menu separator | Generic `bg-border` (undefined in dark theme) | `var(--border-subtle)` (explicit blue-tinted) |
| Toolbar ChevronDown icons | `text-muted-foreground` (shadcn default) | `text-[var(--text-dim)]` (#7a96b8) |
| Dropdown item SVG icons | `text-muted-foreground` fallback | `text-[var(--text-dim)]` fallback |
| DropdownMenuShortcut text | `text-muted-foreground` | `text-[var(--text-dim)]` |
| Editor text cursor | `caret-primary` (brand blue) | `caret-[var(--axon-primary)]` (#87afff) |
| Editor text selection | `bg-brand/25` (undefined) | `rgba(135,175,255,0.18)` |
| Editor placeholder | `text-muted-foreground/80` | `text-[var(--text-dim)]` |
| Ghost text (AI copilot) | `text-muted-foreground/70` | `text-[var(--text-dim)] opacity-70` |
| All structural input borders | Various pink rgba values | `var(--border-subtle)` |
| Focus rings | Various pink rgba values | `var(--focus-ring-color)` |
| MCP transport active bg | Pink rgba | Blue rgba (correct semantic) |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `pnpm build` | 22 routes, 0 errors | 22 routes, 0 errors | ✅ PASS |
| `grep -rn "bg-border" components/ app/` | 0 structural matches | 0 matches remaining | ✅ PASS |
| `grep -rn "text-muted-foreground" components/ui/toolbar.tsx` | 0 matches | 0 matches | ✅ PASS |
| `grep -rn "text-muted-foreground" components/ui/dropdown-menu.tsx` | 0 matches | 0 matches | ✅ PASS |
| Remaining pink rgba audit | Only intentional brand uses | 3 files with semantic pink (doctor error, chat chips, MCP badge) | ✅ EXPECTED |

---

## Design Token Reference (v2 Canonical)

```css
--border-subtle:   rgba(135,175,255,0.08)  /* idle structural borders */
--border-standard: rgba(135,175,255,0.15)  /* more visible borders */
--border-strong:   rgba(135,175,255,0.25)  /* emphasis borders */
--border-accent:   rgba(255,135,175,0.25)  /* intentional pink accent border */
--focus-ring-color: rgba(135,175,255,0.5) /* blue focus ring */
--surface-elevated: rgba(175,215,255,0.05) /* hover/selected surface */
--surface-float:   rgba(175,215,255,0.03)
--text-dim:        #7a96b8
--text-muted:      #93aaca
--text-secondary:  #dce6f0
--text-primary:    #e8f4f8
--axon-primary:    #87afff  /* blue */
--axon-secondary:  #ff87af  /* pink */
```

---

## Decisions Not Taken

- **Replacing semantic pink instances** — Doctor report error border, pulse-chat source chips, MCP stdio badge all retained their pink as intentional semantic color. Replacing them would lose meaningful UX distinction.
- **Touching `focus:bg-accent` / `focus:text-accent-foreground`** in dropdown sub-trigger — These shadcn state tokens were left as-is since our CSS overrides handle them correctly and they're state-specific.
- **Component-level fix for `code-node.tsx` `bg-muted`** — The `.axon-editor code` CSS rule in globals.css overrides this correctly; no component change needed.

---

## Open Questions

- `focus:text-accent-foreground` and `data-[state=open]:text-accent-foreground` in dropdown-menu sub-trigger: these resolve through shadcn `--accent-foreground` which may or may not be correctly defined in our theme. Low priority since the visual result is acceptable.

---

## Next Steps

- Visual QA pass in the browser to confirm all token replacements render correctly in dark theme.
- Consider whether `ui-chip` in `pulse-chat-pane.tsx` should use a tokenized class instead of inline pink rgba for consistency.
- Update `docs/UI-DESIGN-SYSTEM.md` with the finalized "intentional pink" exceptions list.
