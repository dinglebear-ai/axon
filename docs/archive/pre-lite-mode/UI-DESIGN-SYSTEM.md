# Axon UI Design System
Last updated: 2026-03-01

Reference for all visual decisions in `apps/web`. When building new UI, use this document exclusively — do not invent new raw color values or ad-hoc token names.

---

## Table of Contents

1. [Brand Identity](#1-brand-identity)
2. [Color Tokens (v2 — canonical)](#2-color-tokens-v2--canonical)
3. [Typography](#3-typography)
4. [Surfaces](#4-surfaces)
5. [Borders](#5-borders)
6. [Shadows](#6-shadows)
7. [Focus & Interaction States](#7-focus--interaction-states)
8. [Motion Library](#8-motion-library)
9. [Component Utility Classes](#9-component-utility-classes)
10. [Backgrounds & Atmosphere](#10-backgrounds--atmosphere)
11. [Scrollbars](#11-scrollbars)
12. [Spacing Scale](#12-spacing-scale)
13. [Mobile Rules](#13-mobile-rules)
14. [What to Avoid](#14-what-to-avoid)

---

## 1. Brand Identity

Axon's visual language is **dark neural-tech** — deep near-black backgrounds, bioluminescent blue and pink accents, subtle grain texture, and motion that feels organic rather than mechanical. The aesthetic references deep-sea bioluminescence and neural circuit maps.

### Accent Palette

| Name | Token | Hex | Role |
|------|-------|-----|------|
| Primary | `--axon-primary` | `#87afff` | Blue — primary interactive, links, active states, folder icons |
| Primary Strong | `--axon-primary-strong` | `#afd7ff` | Brighter blue — emphasis, processing indicators, pulse dot |
| Secondary | `--axon-secondary` | `#ff87af` | Pink — secondary actions, error/warn states, send button, active selection |
| Secondary Strong | `--axon-secondary-strong` | `#ff9ec0` | Brighter pink — error titles, high-emphasis secondary text |

> **Rule:** Blue (`--axon-primary`) = informational / actionable. Pink (`--axon-secondary`) = accent / status / alert. Never swap these roles.

### Status Colors

| Name | Token | Hex | RGBA background |
|------|-------|-----|-----------------|
| Success | `--axon-success` | `#82d9a0` | `--axon-success-bg` = `rgba(130,217,160,0.14)` · `--axon-success-border` = `rgba(130,217,160,0.30)` |
| Warning | `--axon-warning` | `#ffc086` | `--axon-warning-bg` = `rgba(255,192,134,0.14)` |
| Danger bg | `--axon-danger-bg` | — | `rgba(255,135,175,0.1)` |

---

## 2. Color Tokens (v2 — canonical)

All new code must use these tokens. Raw hex or rgba for anything that has a token is a bug.

### Text

```css
--text-primary:   #e8f4f8   /* High-contrast body text, headings */
--text-secondary: #b8cfe0   /* Default content text, descriptions */
--text-muted:     #7a96b8   /* Labels, placeholders, timestamps */
--text-dim:       #4d6a8a   /* Hint text, disabled labels, empty states */
```

**Usage ladder:** `primary > secondary > muted > dim`

```tsx
// ✓ Correct
<span className="text-[var(--text-secondary)]">Description</span>
<span className="text-[var(--text-dim)]">empty</span>

// ✗ Wrong — raw hex
<span style={{ color: '#b8cfe0' }}>...</span>
```

### Brand (in Tailwind classes)

```tsx
text-[var(--axon-primary)]          // blue interactive
text-[var(--axon-primary-strong)]   // brighter blue
text-[var(--axon-secondary)]        // pink accent
text-[var(--axon-secondary-strong)] // brighter pink
```

### Legacy v1 Tokens — DO NOT USE

These still exist in `:root` for backward compatibility with the Plate editor internals but must not appear in any component code:

| Old token | Maps to |
|-----------|---------|
| `--axon-text-primary` | `--text-primary` |
| `--axon-text-secondary` | `--text-secondary` |
| `--axon-text-muted` | `--text-muted` |
| `--axon-text-dim` / `--axon-text-subtle` | `--text-dim` |
| `--axon-accent-blue` | `--axon-secondary` (was mislabeled — blue name, pink value) |
| `--axon-accent-pink` | `--axon-primary` (was mislabeled — pink name, blue value) |
| `--axon-accent-blue-strong` | `--axon-secondary-strong` |
| `--axon-accent-pink-strong` | `--axon-primary` |

> The v1 names had swapped semantics. A complete sed-rename was applied in Feb 2026. If you ever see `--axon-accent-blue` or `--axon-text-*` in component code, replace them.

---

## 3. Typography

### Font Stack

| Role | CSS Variable | Next.js Variable | Loaded Font | Fallback |
|------|-------------|-----------------|-------------|----------|
| Display / Headings | `--font-display` → `--font-noto-sans` | `--font-noto-sans` | Noto Sans 300–700 | system-ui, sans-serif |
| Body / UI | `--font-sans` → `--font-noto-sans` | `--font-noto-sans` | Noto Sans 300–700 | system-ui, sans-serif |
| Code / Mono | `--font-mono` → `--font-noto-sans-mono` | `--font-noto-sans-mono` | Noto Sans Mono 400, 500, 600 | monospace |

Both `--font-display` and `--font-sans` resolve to the same Noto Sans variable — there is intentionally no separate display typeface. All `h1`–`h4` automatically get `font-display` via global CSS. Use `.font-display` class manually for section titles, branding labels, and breadcrumb current path.

```tsx
// Headings — auto
<h1>Workspace</h1>

// Manual display font (same as body, used for semantic distinction)
<span className="font-display text-sm">Explorer</span>

// Mono content
<code className="font-mono text-xs">path/to/file.ts</code>
```

### Type Scale

```css
--text-2xs:  0.625rem  /* 10px — chips, badge counts, micro labels */
--text-xs:   0.6875rem /* 11px — metadata, hints, ui-meta */
--text-sm:   0.75rem   /* 12px — secondary content, tree nodes */
--text-md:   0.8125rem /* 13px — primary content, file names, form fields */
--text-base: 0.875rem  /* 14px — body copy, descriptions */
```

Tailwind usage: `text-[length:var(--text-md)]` (requires `length:` prefix for CSS custom property font sizes in Tailwind v4).

### Line Heights

```css
--leading-tight: 1.35  /* Compact UI: buttons, labels, tree nodes */
--leading-copy:  1.5   /* Readable text: descriptions, paragraphs */
body default:    1.6   /* Long-form content */
```

### Semantic Typography Classes

| Class | Font size | Weight | Tracking | Use |
|-------|-----------|--------|----------|-----|
| `.ui-label` | `--text-2xs` | 600 | `0.1em` + uppercase | Section labels, form field labels |
| `.ui-meta` | `--text-xs` | 400 | normal | Timestamps, counts, secondary info |
| `.ui-copy` | `--text-md` | 400 | normal | Primary content text |
| `.ui-mono` | `--text-sm` | 400 | normal | Code, paths, IDs (Noto Sans Mono) |
| `.ui-long-copy` | fluid `0.75–0.875rem` | 400 | normal | Readable multi-line content |
| `.font-display` | inherited | inherited | `-0.01em` | Space Mono headings |

---

## 4. Surfaces

Surfaces are translucent dark-blue overlays that create depth over the background gradient. Use the right tier based on how "elevated" the element is.

### Dark overlay tiers

```css
--surface-base:     rgba(10, 18, 35, 0.85)  /* Panels, sidebars, dropdown BGs */
--surface-elevated: rgba(10, 18, 35, 0.60)  /* Active rows, selected items, cards */
--surface-float:    rgba(10, 18, 35, 0.35)  /* Hover states, subtle highlights */
```

**Tier ladder** (back to front): nothing → `surface-float` (hover) → `surface-elevated` (selected/active) → `surface-base` (panel/drawer)

```tsx
// Panel background
style={{ background: 'rgba(10, 18, 35, 0.97)' }}  // drawers (slightly more opaque)
className="bg-[var(--surface-base)]"               // normal panels

// Hover state
className="hover:bg-[var(--surface-float)]"

// Active/selected row
className="bg-[var(--surface-elevated)]"
```

### Cyan-tinted interactive surfaces

Used for item highlights and active states **on dark panels** where the dark `--surface-*` tiers are invisible. These are the primary-accent equivalent of the surface tier system.

```css
--surface-primary:        rgba(135, 175, 255, 0.08)  /* Item hover, progress track, subdued highlight */
--surface-primary-active: rgba(175, 215, 255, 0.12)  /* Item selected, mode chip active, strong highlight */
```

**When to use:**
- Command palette / list item hover → `--surface-primary`
- Command palette / list item selected → `--surface-primary-active`
- Mode chip active background → `--surface-primary-active`
- Progress bar track → `--surface-primary`
- Any cyan-tinted hover/active on a near-opaque dark panel

```tsx
// cmdk item or list row hover
style={{ background: 'var(--surface-primary)' }}

// cmdk item selected / mode chip active
style={{ background: 'var(--surface-primary-active)' }}
```

> **Note:** The dark `--surface-float` / `--surface-elevated` tokens are designed for layering on the semi-transparent body background. On a modal panel with `rgba(10,18,35,0.97)`, they produce no visible contrast — use `--surface-primary` / `--surface-primary-active` instead.

### Page Backgrounds

Each full-page view uses a unique radial gradient atmosphere:

**Home / main:**
```css
background:
  radial-gradient(circle at 15% 35%, rgba(135,175,255,0.22), transparent 42%),
  radial-gradient(circle at 85% 20%, rgba(255,135,175,0.16), transparent 45%),
  radial-gradient(circle at 50% 80%, rgba(95,175,135,0.07), transparent 50%),
  linear-gradient(180deg, #020812 0%, #030712 50%, #020812 100%);
background-attachment: fixed;
```

**In-page sections (Workspace, Settings etc.):**
```css
radial-gradient(ellipse at 14% 10%, rgba(135,175,255,0.08), transparent 34%),
radial-gradient(ellipse at 82% 16%, rgba(255,135,175,0.07), transparent 38%),
linear-gradient(180deg, #02040b 0%, #030712 60%, #040a14 100%)
```

**Modal / drawer overlays:** `rgba(10,18,35,0.97)` background, `bg-black/60 backdrop-blur-sm` for the scrim.

---

## 5. Borders

All borders use the blue token system for structural lines. Pink (`--border-accent`) is reserved exclusively for the omnibox container and accent form controls.

```css
--border-subtle:   rgba(135, 175, 255, 0.15)  /* Dividers, row separators, panel edges */
--border-standard: rgba(135, 175, 255, 0.28)  /* Form inputs, dropdown panels, hover borders */
--border-strong:   rgba(135, 175, 255, 0.40)  /* Active/focus borders, prominent UI edges */
--border-accent:   rgba(255, 135, 175, 0.25)  /* Omnibox container, pink-tinted controls */
```

### Usage Guide

| Context | Token |
|---------|-------|
| Section dividers, row borders, panel edges | `--border-subtle` |
| Dropdown panels, tools dialogs | `--border-standard` |
| Active file explorer left indicator | `border-l-[var(--axon-secondary)]` (2px) |
| Hover border upgrade | `hover:border-[var(--border-standard)]` |
| Omnibox container (idle) | `--border-accent` |
| Focus-visible outline | `var(--focus-ring-color)` (see §7) |

```tsx
// ✓ Correct structural border
<div className="border-b border-[var(--border-subtle)]">

// ✓ Dropdown/dialog panel
<div className="border border-[var(--border-standard)]">

// ✗ Wrong — raw rgba structural border
<div style={{ borderColor: 'rgba(255,135,175,0.12)' }}>
```

---

## 6. Shadows

```css
--shadow-sm: 0 2px 6px rgba(0,0,0,0.2)
--shadow-md: 0 6px 18px rgba(0,0,0,0.3), 0 0 0 1px rgba(135,175,255,0.06)
--shadow-lg: 0 12px 32px rgba(0,0,0,0.4), 0 0 0 1px rgba(135,175,255,0.10)
--shadow-xl: 0 20px 48px rgba(0,0,0,0.5), 0 0 0 1px rgba(135,175,255,0.14)
```

Dropdown panels and dialogs use the full shadow + ring pattern inline:
```
shadow-[0_16px_48px_rgba(0,0,0,0.5),0_0_0_1px_rgba(255,135,175,0.08)]
```

---

## 7. Focus & Interaction States

### Focus Ring

All interactive elements (buttons, inputs, links, `role="button"` divs) must show a visible focus ring on keyboard navigation.

```css
--focus-ring-color: rgba(135, 175, 255, 0.5)   /* blue, 50% opacity */
```

Global rule in `globals.css`:
```css
:focus-visible {
  outline: 2px solid var(--focus-ring-color);
  outline-offset: 2px;
}
```

For Tailwind components, add explicitly:
```tsx
className="focus-visible:outline-2 focus-visible:outline-[var(--focus-ring-color)] focus-visible:outline-offset-1"
```

### Hover Micro-interactions

```tsx
// Background lift
className="hover:bg-[var(--surface-float)]"

// Text color pop to primary
className="hover:text-[var(--axon-primary)]"

// Border upgrade
className="hover:border-[var(--border-standard)]"

// Scale press (FABs, chips)
className="transition-transform active:scale-95"
```

### Active / Selected States

```tsx
// Tree node / file row active
className="bg-[var(--surface-elevated)] text-[var(--axon-secondary)]"

// Left accent bar (file explorer)
className="border-l-2 border-l-[var(--axon-secondary)] bg-[rgba(255,135,175,0.08)]"

// Tab active
className="bg-[var(--surface-elevated)] text-[var(--axon-primary)] font-semibold"

// Mode chip active (blue tinted)
className="border-[rgba(175,215,255,0.38)] bg-[rgba(175,215,255,0.12)] text-[var(--axon-primary)]"
```

---

## 8. Motion Library

All animations are defined in `globals.css`. Use the utility classes — do not write inline `animation:` styles.

### Entrance Animations

| Class | Keyframes | Duration | Easing | Use |
|-------|-----------|----------|--------|-----|
| `animate-fade-in-up` | `fade-in-up` (opacity 0→1, translateY 8→0) | 350ms | `cubic-bezier(0.16,1,0.3,1)` — spring | Staggered list items |
| `animate-fade-in` | `fade-in` (opacity 0→1) | 250ms | ease-out | Simple reveals |
| `animate-scale-in` | `scale-in` (scale 0.95→1, opacity) | 200ms | ease-out | Popovers, modals |
| `animate-slide-down` | `slide-down-reveal` (max-height + opacity) | 300ms | `cubic-bezier(0.16,1,0.3,1)` | Expandable sections |

**Stagger pattern** (used in mode grid):
```tsx
style={{ animationDelay: `${idx * 35}ms`, animationFillMode: 'backwards' }}
className="animate-fade-in-up"
```

### Continuous Animations

| Class | Use |
|-------|-----|
| `animate-shimmer` | Loading skeleton shimmer |
| `animate-omnibox-sweep` | Processing sweep across omnibox (blue→pink gradient) |
| `animate-omnibox-progress` | Bottom progress bar during execution |
| `animate-badge-glow` | Pulsing blue glow on count badges |
| `animate-breathing` | Subtle opacity pulse on idle indicators |
| `animate-check-bounce` | Bouncy check icon on copy success |

### Transition Defaults

Most interactive elements use:
```tsx
className="transition-colors duration-150"   // hover color changes
className="transition-all duration-300"       // layout changes (sidebar expand)
className="transition-all duration-200"       // popover appear
```

---

## 9. Component Utility Classes

Semantic classes defined in `globals.css` for consistent component patterns:

### Text Utilities

```tsx
<span className="ui-label">Section Label</span>
// → 10px, 600w, 0.1em tracking, uppercase, text-dim color

<span className="ui-meta">3 pages · 2s ago</span>
// → 11px, regular, muted color

<p className="ui-copy">File description content...</p>
// → 13px, 1.5 line-height, secondary color

<code className="ui-mono">path/to/file.rs</code>
// → 12px, JetBrains Mono, tight line-height
```

### Chip Utilities

```tsx
<span className="ui-chip">http</span>
// → 10px, 600w, 0.06em tracking, uppercase (add colors separately)

<span className="ui-chip-status">running</span>
// → inline-flex, pill shape, 10px, 600w, uppercase
```

### Table Utilities

```tsx
<table className="ui-table-dense">
  <thead>
    <tr><th className="ui-table-head">URL</th></tr>
  </thead>
  <tbody>
    <tr><td className="ui-table-cell">value</td></tr>
    <tr><td className="ui-table-cell-muted">dim value</td></tr>
  </tbody>
</table>
```

---

## 10. Backgrounds & Atmosphere

### Global Grain Texture

Applied via `body::before` — a fixed SVG fractal noise overlay at 5% opacity over the entire viewport. Gives the UI a subtle organic texture. Never remove this.

### NeuralCanvas

A `<canvas>` component (`/components/neural-canvas.tsx`) renders an animated bioluminescent particle network. Used as a background on `/settings` and referenced in the home page. The canvas z-index is `0`; all content sits at `z-[1]` or above.

### Omnibox Processing State

When a job is running, the omnibox container:
1. Border changes from `--border-accent` (pink) → `rgba(175,215,255,0.4)` (blue)
2. `animate-omnibox-sweep` shimmer plays across the input area
3. `animate-omnibox-progress` bar plays along the bottom edge

Context utilization strip (when Pulse workspace has turns): a gradient bar from blue→pink that fills proportionally to `contextCharsUsed / contextBudget`.

---

## 11. Scrollbars

Custom scrollbars are applied globally. Do not override:

```css
scrollbar-width: thin;
scrollbar-color: rgba(135, 175, 255, 0.35) transparent;

/* WebKit */
::-webkit-scrollbar { width: 6px; height: 6px; }
::-webkit-scrollbar-track { background: transparent; }
::-webkit-scrollbar-thumb { background: rgba(135,175,255,0.35); border-radius: 3px; }
::-webkit-scrollbar-thumb:hover { background: rgba(135,175,255,0.45); }
```

For overflow containers on touch devices always add:
```tsx
style={{ WebkitOverflowScrolling: 'touch' } as React.CSSProperties}
```

---

## 12. Spacing Scale

```css
--space-1: 0.25rem   /* 4px  */
--space-2: 0.5rem    /* 8px  */
--space-3: 0.75rem   /* 12px */
--space-4: 1rem      /* 16px */
--space-5: 1.25rem   /* 20px */
```

Tailwind spacing utilities are preferred (`px-3`, `py-2`, `gap-1.5` etc.) over the CSS variables for most cases. The CSS variables are mainly used for component-level density control.

### Density Tokens (table rows)

```css
--density-compact-row-y:    0.375rem  /* py-1.5 equivalent */
--density-comfortable-row-y: 0.5rem  /* py-2 equivalent */
--density-compact-cell-x:    0.5rem  /* px-2 */
--density-comfortable-cell-x: 0.75rem /* px-3 */
```

---

## 13. Mobile Rules

### Touch Targets

All tappable elements must meet 44×44px minimum on mobile. Pattern:
```tsx
className="min-h-[44px] sm:min-h-0"   // height
className="min-w-[44px] sm:min-w-0"   // width (icon-only buttons)
```

### Breakpoints (Tailwind defaults)

| Prefix | Min-width | Usage in Axon |
|--------|-----------|---------------|
| (none) | 0px | Mobile-first base styles |
| `sm:` | 640px | Most mobile-vs-desktop splits |
| `md:` | 768px | CrawlFileExplorer drawer vs inline |
| `lg:` | 1024px | Settings sidebar nav |

### Sidebar / Panel Pattern

- **Mobile (< `sm`):** Full-height drawer overlay — `fixed inset-0 z-40`, panel slides from left, backdrop `bg-black/60 backdrop-blur-sm`, close on file select
- **Desktop (`sm+`):** Inline collapsible — `transition-all duration-300 w-64` or `w-0`

### FABs (Floating Action Buttons)

Mobile-only buttons that appear when panels are collapsed use:
```tsx
className="fixed bottom-[max(1rem,env(safe-area-inset-bottom,1rem))] left-4 z-40"
```
The `env(safe-area-inset-bottom)` accounts for iOS home indicator.

### Responsive Text Hiding

```tsx
<span className="hidden sm:inline">Reset to defaults</span>
<span className="sm:hidden">Reset</span>
```

---

## 14. What to Avoid

| Anti-pattern | Correct alternative |
|--------------|---------------------|
| `color: '#87afff'` (raw hex for brand) | `color: var(--axon-primary)` |
| `rgba(255,135,175,0.12)` on structural borders | `var(--border-subtle)` |
| `rgba(175,215,255,0.05)` for hover on dark panels | `var(--surface-primary)` |
| `rgba(175,215,255,0.12)` for active on dark panels | `var(--surface-primary-active)` |
| `rgba(130,217,160,0.30)` for success borders | `var(--axon-success-border)` |
| `--axon-text-primary` / `--axon-text-muted` | `--text-primary` / `--text-muted` |
| `--axon-accent-pink` / `--axon-accent-blue` | `--axon-primary` / `--axon-secondary` |
| `--axon-text-tertiary` | `--text-dim` (this token is undefined) |
| `py-1` on interactive buttons | `min-h-[44px] sm:min-h-0` on mobile |
| Inline `animation:` CSS | Use the `.animate-*` utility classes |
| `bg-black/80` on drawers | `rgba(10,18,35,0.97)` with `backdrop-blur` |
| `border-white/10` (generic whites) | `var(--border-subtle)` |
| Hardcoded `z-10`, `z-20` randomly | Follow z-index ladder: content=1, sticky headers=30, drawers=40/50 |

### Z-Index Ladder

| Value | Usage |
|-------|-------|
| `z-[1]` | Main page content above canvas background |
| `z-30` | Sticky headers |
| `z-40` | Mobile drawer backdrops, FABs |
| `z-50` | Drawer panels, tooltips, dropdowns |
