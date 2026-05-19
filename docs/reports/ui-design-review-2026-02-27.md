# UI Design Review — axon_rust Web Application
**Date:** 2026-02-27
**Scope:** All components in `apps/web/` (76 files)
**Method:** 4 specialized review agents running in parallel, each owning a distinct domain
**Criteria:** Frontend Design Skill — typography, color, motion, spatial composition, backgrounds/depth

---

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [Cross-Cutting Violations](#cross-cutting-violations) — Issues every agent found independently
3. [Component-Specific Findings](#component-specific-findings)
   - [Global Design System & Pages](#global-design-system--pages)
   - [Core UI Primitives & Omnibox](#core-ui-primitives--omnibox)
   - [Pulse Workspace](#pulse-workspace)
   - [Results Renderers & Crawl Components](#results-renderers--crawl-components)
4. [UX Anti-Patterns](#ux-anti-patterns)
5. [Master Enhancement List](#master-enhancement-list)
6. [Recommended Execution Order](#recommended-execution-order)

---

## Executive Summary

Four independent design agents reviewed 76 UI files across the entire `apps/web/` surface. Every agent independently reached the same five conclusions — a strong signal these are systemic, not incidental.

| Dimension | Status | Verdict |
|-----------|--------|---------|
| **Typography** | No display font. Outfit is generic. | ❌ CRITICAL violation |
| **Color** | Timid palette, misnamed variables, no dominants | ❌ CRITICAL violation |
| **Motion** | 3 animations total, none in Pulse workspace | ❌ MAJOR violation |
| **Spatial Composition** | Symmetric grids everywhere, no asymmetry | ❌ MAJOR violation |
| **Backgrounds & Depth** | Flat rgba surfaces, minimal shadows, no texture | ❌ MAJOR violation |

**The UI is functionally correct but visually generic.** It reads like a competent implementation of shadcn/ui defaults — clean, maintainable, and indistinguishable from a hundred other developer tools. There is no visual identity, no typographic personality, no spatial surprise, and no kinetic feedback. Fixing the top three dimensions (typography, color, motion) alone would transform the product's perceived quality.

---

## Cross-Cutting Violations

These violations were independently identified by all four agents. They are systemic — rooted in `globals.css` and `layout.tsx` — and affect every surface.

---

### V1: Generic Typography — No Display Font, No Personality [CRITICAL]

**Files:** `app/layout.tsx:7-17`, `app/globals.css`

**What's there:**
```typescript
const outfit = Outfit({ variable: '--font-sans', ... })          // body
const jetbrainsMono = JetBrains_Mono({ variable: '--font-mono', ... }) // code
```

**What's wrong:**
- **Outfit** is a pleasant but unremarkable geometric sans — used in hundreds of SaaS products. No character.
- **No display font** at all. H1/H2/section headers use the same Outfit as body text, differentiated only by weight. There is no typographic hierarchy.
- The product is named "Axon" (neural/electrical) yet has typography that suggests "travel booking app."
- Result: the UI is visually forgotten on first glance.

**Skill requirement:** Pair a distinctive display font with a refined body font. Avoid generic families.

**Fix:**
```typescript
// Display: Space Mono (futuristic, monospace energy matching "Axon" neural branding)
// Body:    Sora (clean, contemporary, legible — not Inter)
// Mono:    JetBrains Mono (keep — excellent for code)

const spaceMono = Space_Mono({ variable: '--font-display', weight: ['400', '700'], subsets: ['latin'] })
const sora = Sora({ variable: '--font-sans', subsets: ['latin'], weight: ['300', '400', '500', '600'] })
const jetbrainsMono = JetBrains_Mono({ variable: '--font-mono', ... })
```

Then apply `--font-display` to all headings, section labels, role labels in chat, and the omnibox itself. `--font-sans` for body, hints, descriptions.

---

### V2: Timid, Evenly-Distributed Color Palette [CRITICAL]

**Files:** `app/globals.css:52-107`

**What's there:**
```css
--axon-accent-blue: #ff87af;       /* Actually PINK */
--axon-accent-pink: #afd7ff;       /* Actually CYAN */
--axon-accent-pink-strong: #87afff; /* Also blue */
```
Six nearly identical pale blue-gray text colors. Backgrounds all using the same `rgba(10, 18, 35, 0.X)` formula. Accent colors at ~70% lightness with low chroma — they don't pop.

**What's wrong:**
1. **Variable names are backwards.** Blue variable is pink. Pink variable is cyan. This is not a naming error — it's evidence the palette was assembled without a committed direction.
2. **No dominant color.** There is no single hue that *leads* the visual hierarchy. Pink and cyan are treated equally in every component.
3. **Accents are timid.** Both "accent" colors live in the same mid-tone range. Neither commands attention.
4. **Six text color variants** (`text-primary`, `text-secondary`, `text-muted`, `text-subtle`, `text-dim`, `text-placeholder`) create a gradational graying that reduces perceived contrast and hierarchy.
5. **Inline RGBA everywhere.** Components hardcode `rgba(255, 135, 175, 0.12)` rather than consuming CSS variables, making palette changes impossible without find-replace.

**Fix:**
```css
/* Rename correctly and strengthen */
:root {
  /* Brand dominants — one leads, one supports */
  --axon-primary: #87afff;          /* Bright cyan-blue — the DOMINANT accent */
  --axon-secondary: #ff87af;        /* Warm pink — SUPPORTING accent */

  /* Strong variants for active/hover states */
  --axon-primary-strong: #afd7ff;
  --axon-secondary-strong: #ff9ec0;

  /* Semantic tokens — reduce text variants from 6 to 4 */
  --axon-text-primary: #e8f4f8;
  --axon-text-secondary: #b8cfe0;
  --axon-text-muted: #7a96b8;
  --axon-text-dim: #4d6a8a;

  /* Surfaces — explicit opacity tiers */
  --surface-base: rgba(10, 18, 35, 0.85);
  --surface-elevated: rgba(10, 18, 35, 0.60);
  --surface-float: rgba(10, 18, 35, 0.35);

  /* Borders */
  --border-subtle: rgba(135, 175, 255, 0.15);
  --border-standard: rgba(135, 175, 255, 0.28);
  --border-strong: rgba(135, 175, 255, 0.40);
  --border-accent: rgba(255, 135, 175, 0.25);
}
```

Then do one audit pass to replace all inline `rgba(255, 135, 175, ...)` and `rgba(175, 215, 255, ...)` with semantic variables.

---

### V3: Minimal Motion — 3 Animations, None in the Primary Workspace [MAJOR]

**Files:** `app/globals.css:287-345` (only 3 `@keyframes` defined), all Pulse components

**What's there:**
- `@keyframes shimmer` — generic loading state
- `@keyframes omnibox-sweep` — omnibox-only
- `@keyframes omnibox-progress` — omnibox-only
- `animate-pulse` on a single dot in the loading indicator

**What's wrong:**
- Messages appear instantly (no fade-in, no slide)
- Tool badges have no hover animation (only `transition-colors duration-100`)
- Thinking blocks toggle with no content reveal animation
- Tables render all rows simultaneously — no stagger
- Cards have no hover elevation (only color change)
- Page sections have no entrance animations
- Split pane has no drag feedback
- Source list has no expand/collapse animation
- Confirmation dialogs just appear

**Fix — define a motion layer in `globals.css`:**
```css
/* Core animation library */
@keyframes fade-in-up {
  from { opacity: 0; transform: translateY(8px); }
  to   { opacity: 1; transform: translateY(0); }
}

@keyframes fade-in {
  from { opacity: 0; }
  to   { opacity: 1; }
}

@keyframes scale-in {
  from { opacity: 0; transform: scale(0.95); }
  to   { opacity: 1; transform: scale(1); }
}

@keyframes badge-glow {
  0%, 100% { box-shadow: 0 0 0 0 rgba(135, 175, 255, 0); }
  50%       { box-shadow: 0 0 12px 4px rgba(135, 175, 255, 0.25); }
}

@keyframes breathing {
  0%, 100% { opacity: 0.6; }
  50%       { opacity: 1; }
}

@keyframes check-bounce {
  0%   { transform: scale(0.6); opacity: 0; }
  60%  { transform: scale(1.15); }
  100% { transform: scale(1); opacity: 1; }
}

@keyframes divider-glow {
  from { box-shadow: none; }
  to   { box-shadow: 0 0 12px rgba(135, 175, 255, 0.3); }
}

@keyframes slide-down-reveal {
  from { max-height: 0; opacity: 0; }
  to   { max-height: 600px; opacity: 1; }
}
```

Apply to components:
- **Messages:** `animate-[fade-in-up_0.35s_cubic-bezier(0.16,1,0.3,1)_forwards]` + `animation-delay: calc(index * 30ms)`
- **Table rows:** Same stagger pattern
- **Cards on hover:** `hover:-translate-y-0.5 hover:shadow-[0_8px_20px_rgba(135,175,255,0.15)]`
- **Tool badge hover:** `badge-glow` keyframe
- **Source list expand:** `slide-down-reveal` on the container
- **Thinking block content:** `fade-in` on the revealed content div
- **Copy success:** `check-bounce` on the check icon

---

### V4: Predictable, Symmetric Layouts — No Spatial Surprise [MAJOR]

**Files:** `app/settings/page.tsx`, `components/pulse/pulse-workspace.tsx`, `components/results-panel.tsx`, all page layouts

**What's wrong:**
Every page uses the same pattern: `flex` or `grid` with uniform gutters, centered max-width column, sidebar + content. No asymmetry, no overlap, no diagonal elements, no grid-breaking composition. The result reads as "dashboard template" rather than "crafted product."

**Specific violations:**
- Settings: Standard sidebar nav (left) + centered content column. Seen in every SaaS product ever built.
- Pulse workspace: Horizontal split-pane with vertical divider. Pure symmetry.
- Results panel: Rigid two-column flex. Symmetrical sidebar.
- Doctor report: 4-equal-tile grid. All same size.
- Message bubbles: Uniform card borders, uniform padding, centered.

**Targeted fixes (without full rebuild):**

1. **Message bubbles** — introduce asymmetric alignment:
   ```jsx
   // User messages: right-aligned, offset margin
   <div className={`flex w-full ${isUser ? 'justify-end' : 'justify-start'}`}>
     <article className={isUser ? 'mr-6 max-w-[70%]' : 'ml-2 max-w-[78%]'}>
   ```

2. **Doctor report** — break uniform 4-col grid:
   ```jsx
   <div className="grid gap-4 md:grid-cols-3">
     <div className="md:col-span-2"><PrimaryMetric /></div>  {/* spans 2 */}
     <div className="space-y-2"><MiniMetric /><MiniMetric /></div>
   </div>
   ```

3. **Settings sidebar** — add accent line and offset:
   ```jsx
   <nav className="border-r border-r-[var(--border-gradient)] ...">
   ```

4. **Tool badges** — subtle vertical stagger:
   ```jsx
   style={{ transform: `translateY(${j % 2 === 0 ? '0' : '3px'})` }}
   ```

---

### V5: Flat Surfaces — No Texture, Depth, or Atmosphere [MAJOR]

**Files:** `app/globals.css:166-180`, every component file

**What's there:**
The body has two radial gradients + one linear gradient. Every component then overrides that with `rgba(10, 18, 35, 0.42)` or similar — flat, uniform, textureless.

**What's wrong:**
- Gradient stops are so low-opacity (0.10, 0.12) the effect is imperceptible
- No grain, noise, or texture layer
- No layered shadow hierarchy — all shadows are single, weak values
- Cards don't feel elevated; they feel printed flat on the screen
- No decorative borders, dividers with personality, or geometric accents

**Fix — three-tier atmospheric enhancement:**

```css
/* 1. Add grain overlay to body */
body::before {
  content: '';
  position: fixed;
  inset: 0;
  background-image: url("data:image/svg+xml,%3Csvg viewBox='0 0 200 200' xmlns='http://www.w3.org/2000/svg'%3E%3Cfilter id='n'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='0.9' numOctaves='4'/%3E%3C/filter%3E%3Crect width='200' height='200' filter='url(%23n)' opacity='0.05'/%3E%3C/svg%3E");
  background-size: 200px 200px;
  pointer-events: none;
  z-index: 0;
}

/* 2. Strengthen body background */
body {
  background:
    radial-gradient(circle at 15% 35%, rgba(135, 175, 255, 0.22), transparent 42%),
    radial-gradient(circle at 85% 20%, rgba(255, 135, 175, 0.16), transparent 45%),
    radial-gradient(circle at 50% 80%, rgba(95, 175, 135, 0.07), transparent 50%),
    linear-gradient(180deg, #020812 0%, var(--axon-bg) 50%, #020812 100%);
  background-attachment: fixed;
}

/* 3. Shadow hierarchy */
:root {
  --shadow-sm: 0 2px 6px rgba(0, 0, 0, 0.20);
  --shadow-md: 0 6px 18px rgba(0, 0, 0, 0.30), 0 0 0 1px rgba(135, 175, 255, 0.06);
  --shadow-lg: 0 12px 32px rgba(0, 0, 0, 0.40), 0 0 0 1px rgba(135, 175, 255, 0.10);
  --shadow-xl: 0 20px 48px rgba(0, 0, 0, 0.50), 0 0 0 1px rgba(135, 175, 255, 0.14);
}
```

Apply shadow tiers:
- Inline message bubbles: `--shadow-sm`
- Floating tooltips: `--shadow-lg`
- Modals/confirmations: `--shadow-xl`
- Cards on hover: step up one shadow tier

---

## Component-Specific Findings

### Global Design System & Pages

**`app/settings/page.tsx`**

| Issue | Severity | File:Line |
|-------|----------|-----------|
| Inline `rgba()` hardcoded in every section header border | MAJOR | 137 |
| Icon container (28×28px box) oversized for 3.5×3.5px icon | MINOR | 125-134 |
| Confirmation dialogs are inline state toggles, not modals | MAJOR | 303-422 |
| SectionDivider is a 1px flat line with no visual personality | MINOR | throughout |
| All section headers `text-sm font-semibold` — no hierarchy | MAJOR | 115-142 |

**`app/mcp/page.tsx` & `app/mcp/components.tsx`**

| Issue | Severity | File:Line |
|-------|----------|-----------|
| Delete confirmation appears inline replacing the card | MAJOR | mcp/page.tsx:235-259 |
| MCP form has two save handlers (form vs JSON tab) — confusing UX | MAJOR | components.tsx:336-352 |
| "Checking" status uses yellow pulse — ambiguous (looks like warning) | MINOR | components.tsx:174-186 |
| Unknown status dot: `rgba(255,255,255,0.2)` barely visible | MINOR | components.tsx:177 |
| Empty state copy: "No MCP servers configured" — no guidance | MINOR | page.tsx:211-231 |

**`app/agents/page.tsx`**

| Issue | Severity | File:Line |
|-------|----------|-----------|
| Empty state fallback is generic loading error — no contextual help | MINOR | agents page |

---

### Core UI Primitives & Omnibox

**`components/omnibox.tsx`**

| Issue | Severity | File:Line |
|-------|----------|-----------|
| Mode dropdown hidden on mobile (`showModeSelector`) — no fallback | MAJOR | 654-677, 832-848 |
| Placeholder rotates every 3.5s — interrupts user mid-type | MINOR | 492-502 |
| Status text vanishes on command completion — no success/error confirmation | MAJOR | 630-648 |
| @mention syntax undiscoverable (no hint visible until user types `@`) | MEDIUM | 928-955 |
| Icon-only buttons lack `aria-label` (only `title`) | MAJOR | 654-807 |
| Mode switch on URL detection is subtle — users miss it | MAJOR | 658-661 |
| Suggestion items appear all at once — no stagger | MINOR | 883-907 |

**`components/ui/` primitives**

| Issue | Severity | File:Line |
|-------|----------|-----------|
| `button.tsx`: disabled state = opacity-50 only (no color/border change) | MINOR | button.tsx:8 |
| `tabs.tsx`: `after:opacity-0` has no animation trigger defined | MINOR | tabs.tsx |
| `tooltip.tsx`: `animate-in fade-in-0 zoom-in-95` is good but default — no brand character | MINOR | tooltip.tsx |
| `scroll-area.tsx`: scrollbar thumb `rgba(255,135,175,0.15)` fails WCAG AA contrast | MAJOR | globals.css:279-281 |
| No `focus-visible` ring with brand color — uses muted blue | MINOR | all UI components |

---

### Pulse Workspace

**`components/pulse/pulse-chat-pane.tsx`**

| Issue | Severity | File:Line |
|-------|----------|-----------|
| Empty state has no icon — text-only card blends with messages | MEDIUM | 357-367 |
| Loading indicator is single `animate-pulse` dot + static text | MAJOR | 393-410 |
| Source list expand/collapse: no animation, button text doesn't change ("hide"/"show") | MEDIUM | 228-336 |
| `aria-expanded` set but no visual state change on the button | MEDIUM | 251-258 |

**`components/pulse/message-content.tsx`**

| Issue | Severity | File:Line |
|-------|----------|-----------|
| User message bubble gradient: 0.2→0.08 opacity — near-invisible | MAJOR | 163-167 |
| Message timestamps: `var(--text-2xs)` = 10px — below accessible minimum | MINOR | 170-188 |
| Copy button: all three states (idle/copied/failed) look identical | MINOR | 204-215 |
| Thinking block content appears instantly — no reveal animation | MINOR | 14-41 |
| Character count shown — word count is more meaningful | MINOR | 14-41 |

**`components/pulse/tool-badge.tsx`**

| Issue | Severity | File:Line |
|-------|----------|-----------|
| Pin-on-click behavior is completely undiscoverable | MAJOR | 118-217 |
| No visual indicator when tooltip is pinned | MAJOR | 118-217 |
| Badge hover: only `transition-colors duration-100` — no scale, no glow | MAJOR | 162 |

**`components/pulse/pulse-workspace.tsx`**

| Issue | Severity | File:Line |
|-------|----------|-----------|
| Split pane divider: 2px wide, `rgba(255,135,175,0.14)` — nearly invisible | MINOR | 348-362 |
| `aria-valuenow={0}` hardcoded and never updated during drag | MAJOR | 348-362 |
| Workspace container: flat `rgba(10,18,35,0.42)` — no depth | MAJOR | 314 |

**`components/pulse/pulse-toolbar.tsx`**

| Issue | Severity | File:Line |
|-------|----------|-----------|
| Document title input: no unsaved indicator | MINOR | 42-49 |

**`components/pulse/pulse-mobile-pane-switcher.tsx`**

| Issue | Severity | File:Line |
|-------|----------|-----------|
| Icon-only tabs — no text labels on mobile (hard to discover) | MEDIUM | 14-50 |
| Active state not visually distinct enough | MEDIUM | 14-50 |

---

### Results Renderers & Crawl Components

**`components/results-panel.tsx`**

| Issue | Severity | File:Line |
|-------|----------|-----------|
| Tab badges show count but not "N new" — users miss updates | MEDIUM | 111-277 |
| No virtual scrolling — 100k+ URL results render all at once in DOM | CRITICAL | throughout |

**`components/results/doctor-report.tsx`**

| Issue | Severity | File:Line |
|-------|----------|-----------|
| Failed services not visually grouped/prioritized above healthy | MAJOR | 129-131 |
| 200+ lines of service detail with no collapsible sections | MAJOR | 66-122 |
| 4-equal-tile grid — no hierarchy between metrics | MAJOR | 173-178 |

**`components/results/table-renderer.tsx`**

| Issue | Severity | File:Line |
|-------|----------|-----------|
| No pagination for large result sets | CRITICAL | 177-214 |
| All columns same width — no priority-based sizing | MAJOR | 197-210 |
| Hover: only background color change — no row elevation | MINOR | 201 |
| KV table: label and value same font size/weight | MAJOR | throughout |

**`components/results/raw-renderer.tsx`**

| Issue | Severity | File:Line |
|-------|----------|-----------|
| Loading state: generic spinner + "Processing..." — no elapsed time | MEDIUM | 18-24 |
| Empty state: "No output" — no distinction between "not yet" vs "nothing returned" | MEDIUM | 26-29 |

**`components/crawl-file-explorer.tsx`**

| Issue | Severity | File:Line |
|-------|----------|-----------|
| `role="button"` items have no `focus:ring` — keyboard nav invisible | MAJOR | 233-295 |
| File count not visible until explorer is opened | MINOR | 128-129 |

**`components/content-viewer.tsx`**

| Issue | Severity | File:Line |
|-------|----------|-----------|
| CopyButton has no visual success feedback | MINOR | 60 |
| Error state: no icon, no actionable guidance | MEDIUM | 18-24 |

**`components/command-options-panel.tsx`**

| Issue | Severity | File:Line |
|-------|----------|-----------|
| Checkbox buttons lack `focus:ring` | MAJOR | 46-71 |

---

## UX Anti-Patterns

Ordered by user impact:

| # | Pattern | Category | Files | Impact |
|---|---------|----------|-------|--------|
| 1 | **No virtual scrolling for large result sets** — 100k+ sources rendered in DOM | Performance | `results-panel.tsx`, `table-renderer.tsx` | Browser freeze |
| 2 | **Inline delete confirmation** replaces card — user can't see what they're deleting | Destructive UX | `mcp/page.tsx:235-259` | Accidental data loss |
| 3 | **Status text disappears on completion** — no success/error confirmation | Feedback | `omnibox.tsx:630-648` | User doesn't know if command worked |
| 4 | **Mode switch silent on URL detection** — users execute in wrong mode | Feedback | `omnibox.tsx:658-661` | Commands run in unexpected mode |
| 5 | **Tool pin interaction undiscoverable** — click-to-pin not signaled | Discoverability | `tool-badge.tsx:118-217` | Power feature hidden from all users |
| 6 | **@mention syntax undiscoverable** — only revealed after user types `@` | Discoverability | `omnibox.tsx:928-955` | Context feature rarely used |
| 7 | **Mode selector hidden on mobile** — no fallback for small screens | Discoverability | `omnibox.tsx:654-677` | Mobile users can't switch modes |
| 8 | **Placeholder rotation** during user typing — interrupts mental model | Flow | `omnibox.tsx:492-502` | Input field seems broken |
| 9 | **Two save handlers** (form tab vs JSON tab) — unclear what state persists | Flow | `mcp/components.tsx:336-352` | Data loss risk on tab switch |
| 10 | **Empty states are text-only** — no icon, no actionable next step | Feedback | multiple | High friction for new users |
| 11 | **Thinking block** — character count shown, not word count | Hierarchy | `message-content.tsx:14-41` | Meaningless metric |
| 12 | **`aria-valuenow={0}`** hardcoded on split pane — screen reader lies about state | Accessibility | `pulse-workspace.tsx:348` | Screen reader inaccurate |
| 13 | **Color contrast failures** — `--axon-text-dim` (#5f87af) on dark bg fails WCAG AA | Accessibility | `globals.css:98-102` | Low vision users can't read labels |
| 14 | **Scrollbar thumb** `rgba(255,135,175,0.15)` — fails WCAG AA contrast | Accessibility | `globals.css:279-281` | Invisible scrollbar |
| 15 | **File explorer** — `role="button"` elements with no `focus:ring` | Accessibility | `crawl-file-explorer.tsx:240` | Keyboard navigation invisible |

---

## Master Enhancement List

### Priority: HIGH (do first)

| # | Enhancement | Component(s) | Est. Effort |
|---|-------------|--------------|-------------|
| H1 | **Distinctive font pairing** — Space Mono (display) + Sora (body) | `layout.tsx`, `globals.css` | 2h |
| H2 | **Color palette rebuild** — fix naming, add dominants, define surface tiers | `globals.css` + audit pass | 6h |
| H3 | **Motion layer** — 8 `@keyframes`, applied to messages/tables/cards/badges | `globals.css` + 6 components | 8h |
| H4 | **Body atmosphere** — stronger radials + grain overlay pseudo-element | `globals.css` | 2h |
| H5 | **Shadow hierarchy** — `--shadow-sm/md/lg/xl` tokens, applied to all surfaces | `globals.css` + 8 components | 3h |
| H6 | **Fix WCAG contrast failures** — text-dim, scrollbar thumb | `globals.css` | 1h |
| H7 | **Keyboard focus rings** — brand-colored `focus-visible:ring` on all interactives | all UI components | 3h |
| H8 | **Virtual scrolling** — for table-renderer and results-panel large datasets | `table-renderer.tsx`, `results-panel.tsx` | 8h |

### Priority: MEDIUM (second pass)

| # | Enhancement | Component(s) | Est. Effort |
|---|-------------|--------------|-------------|
| M1 | **Empty state redesign** — icon + gradient card + actionable copy | `pulse-chat-pane.tsx`, `mcp/page.tsx`, `agents/page.tsx`, `raw-renderer.tsx` | 4h |
| M2 | **Message bubble redesign** — asymmetric alignment, stronger user/assistant color split | `message-content.tsx` | 3h |
| M3 | **Modal dialog for destructive actions** (replace inline confirmation) | `mcp/page.tsx`, `settings/page.tsx` | 4h |
| M4 | **Status bar persistence** — keep visible after command completion (success/error) | `omnibox.tsx:630-648` | 2h |
| M5 | **Tool badge discoverability** — pin indicator dot, hover label "click to pin" | `tool-badge.tsx` | 2h |
| M6 | **Source list animation** — slide-down reveal + button label toggle | `pulse-chat-pane.tsx:228-336` | 1h |
| M7 | **Thinking block improvements** — fade reveal, word count, hover affordance | `message-content.tsx:14-41` | 1h |
| M8 | **Doctor report** — failure-first sorting with visual grouping, collapsible sections | `doctor-report.tsx` | 3h |
| M9 | **Loading indicators** — elapsed time counter, phase-aware text, breathing animation | `raw-renderer.tsx`, `pulse-chat-pane.tsx` | 2h |
| M10 | **Doctor report asymmetric grid** — primary metric spans 2 cols, mini metrics sidebar | `doctor-report.tsx:173-178` | 2h |
| M11 | **@mention discoverability** — tip below input on first focus | `omnibox.tsx` | 1h |
| M12 | **Settings section headers** — typography hierarchy, size variation H2/H3 | `settings/page.tsx` | 2h |
| M13 | **Mobile pane switcher** — add text labels, full-color active state | `pulse-mobile-pane-switcher.tsx` | 1h |
| M14 | **MCP form** — single unified save button, live JSON preview, explicit sync feedback | `mcp/components.tsx:336-352` | 3h |
| M15 | **Split pane divider** — wider handle (8px), drag icon, update `aria-valuenow` dynamically | `pulse-workspace.tsx:348-362` | 2h |

### Priority: LOW (polish pass)

| # | Enhancement | Component(s) | Est. Effort |
|---|-------------|--------------|-------------|
| L1 | **Copy button success animation** — `check-bounce` keyframe, green flash | `content-viewer.tsx`, `message-content.tsx` | 1h |
| L2 | **Staggered suggestion reveals** in omnibox mode dropdown | `omnibox.tsx:883-907` | 1h |
| L3 | **Asymmetric settings layout** — right-aligned sidebar, wider content, skewed dividers | `settings/page.tsx` | 4h |
| L4 | **NeuralCanvas `zen` profile** for settings page | `neural-canvas.tsx` | 2h |
| L5 | **Unsaved title indicator** on Pulse document | `pulse-toolbar.tsx:42-49` | 1h |
| L6 | **Timestamp legibility** — increase from 10px to 11px, improve color | `message-content.tsx:170-188` | 30m |
| L7 | **Pagination or "top N" toggle** for tables with >1000 rows | `table-renderer.tsx` | 3h |
| L8 | **MCP status "Checking"** — use spinner icon instead of yellow pulse (less ambiguous) | `mcp/components.tsx:174-186` | 30m |
| L9 | **CSS custom property audit** — replace all hardcoded `rgba()` with design tokens | all components | 6h |
| L10 | **NeuralCanvas profile switcher** — visible persistent control, not buried in menu | `neural-canvas.tsx` | 2h |

---

## Recommended Execution Order

```
Phase 1 — Foundation (20h, immediate visual impact)
  H1  Font pairing (Space Mono + Sora)
  H2  Color palette rebuild + variable audit
  H3  Motion layer (8 keyframes + integration)
  H4  Atmospheric background (grain overlay + stronger gradients)
  H5  Shadow hierarchy tokens
  H6  WCAG contrast fixes
  H7  Keyboard focus rings

Phase 2 — Primary Surfaces (25h, core UX improvements)
  H8  Virtual scrolling for large result sets
  M1  Empty state redesign across all components
  M2  Message bubble asymmetric alignment + color split
  M3  Modal dialog for destructive actions
  M4  Status bar persistence
  M5  Tool badge pin discoverability
  M6  Source list animation
  M7  Thinking block improvements
  M8  Doctor report failure grouping

Phase 3 — Polish (15h, refinement)
  M9–M15  Loading indicators, doctor report grid, @mention tip, settings typography,
           mobile switcher labels, MCP form single save, split pane improvements

Phase 4 — Nice-to-Have (15h)
  L1–L10  Copy feedback, suggestion stagger, asymmetric settings, NeuralCanvas profiles,
           unsaved indicator, timestamp, pagination, MCP status, token audit
```

**Total estimated effort:** ~75h
**Quick win (8h, immediate improvement):** H1 (fonts) + H2 (color) + H4 (background) — transforms perceived quality before touching any component logic.

---

*Generated by 4-agent parallel design review. Agents covered: global design system, core UI primitives/omnibox, Pulse workspace, results/crawl renderers.*
