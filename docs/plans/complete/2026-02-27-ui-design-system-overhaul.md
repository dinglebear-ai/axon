# UI Design System Overhaul Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` to execute this plan.
> This plan uses a **two-wave team structure**. Wave 1 must fully complete before Wave 2 agents start.
> All 33 issues from `docs/reports/ui-design-review-2026-02-27.md` are addressed.

**Goal:** Surgically address all 33 UI design misalignments identified in the design review — typography, color system, motion layer, spatial composition, depth/atmosphere, UX anti-patterns, and accessibility — using a 7-agent team with zero file conflicts.

**Architecture:** Wave 1 (1 agent) establishes the complete CSS design token foundation in `globals.css` and `layout.tsx`. Wave 2 (6 parallel agents) consumes those tokens to implement component-level changes. No two agents ever touch the same file.

**Tech Stack:** Next.js 15, Tailwind CSS v4, next/font/google, React 19, TypeScript, shadcn/ui radix primitives

---

## CRITICAL: File Ownership Map

No file appears in more than one agent's list. This is the contract that enables parallel execution.

| File | Agent |
|------|-------|
| `app/globals.css` | **Wave 1** |
| `app/layout.tsx` | **Wave 1** |
| `lib/pulse/neural-canvas-presets.ts` | **Agent 7** |
| `components/neural-canvas.tsx` | **Agent 7** |
| `components/omnibox.tsx` | **Agent 5** |
| `components/ui/button.tsx` | **Agent 4** |
| `components/ui/input.tsx` | **Agent 4** |
| `components/ui/tabs.tsx` | **Agent 4** |
| `components/ui/dropdown-menu.tsx` | **Agent 4** |
| `components/ui/badge.tsx` | **Agent 4** |
| `components/ui/scroll-area.tsx` | **Agent 4** |
| `components/pulse/pulse-chat-pane.tsx` | **Agent 2** |
| `components/pulse/message-content.tsx` | **Agent 2** |
| `components/pulse/tool-badge.tsx` | **Agent 2** |
| `components/pulse/pulse-mobile-pane-switcher.tsx` | **Agent 2** |
| `components/pulse/pulse-workspace.tsx` | **Agent 2** |
| `components/pulse/pulse-toolbar.tsx` | **Agent 2** |
| `components/results/table-renderer.tsx` | **Agent 3** |
| `components/results/doctor-report.tsx` | **Agent 3** |
| `components/results/raw-renderer.tsx` | **Agent 3** |
| `components/results-panel.tsx` | **Agent 3** |
| `components/content-viewer.tsx` | **Agent 3** |
| `components/crawl-file-explorer.tsx` | **Agent 3** |
| `components/command-options-panel.tsx` | **Agent 3** |
| `app/mcp/page.tsx` | **Agent 6** |
| `app/mcp/components.tsx` | **Agent 6** |
| `app/settings/page.tsx` | **Agent 6** |
| `app/agents/page.tsx` | **Agent 6** |

---

## CSS Design Tokens Reference

**Wave 1 establishes these tokens. All Wave 2 agents MUST use them — never hardcode rgba() values.**

```css
/* ── Typography ── */
--font-display: var(--font-space-mono)   /* headings, labels, omnibox */
--font-sans: var(--font-sora)            /* body text, descriptions */
--font-mono: var(--font-jetbrains-mono)  /* code, tables, mono data */

/* ── Brand palette ── */
--axon-primary: #87afff          /* dominant — cyan-blue */
--axon-primary-strong: #afd7ff   /* hover/active states */
--axon-secondary: #ff87af        /* supporting — warm pink */
--axon-secondary-strong: #ff9ec0 /* hover/active states */

/* ── Surface tiers (replaces ad-hoc rgba(10,18,35,0.X)) ── */
--surface-base: rgba(10, 18, 35, 0.85)
--surface-elevated: rgba(10, 18, 35, 0.60)
--surface-float: rgba(10, 18, 35, 0.35)

/* ── Borders ── */
--border-subtle: rgba(135, 175, 255, 0.15)
--border-standard: rgba(135, 175, 255, 0.28)
--border-strong: rgba(135, 175, 255, 0.40)
--border-accent: rgba(255, 135, 175, 0.25)

/* ── Text ── */
--text-primary: #e8f4f8
--text-secondary: #b8cfe0
--text-muted: #7a96b8
--text-dim: #4d6a8a

/* ── Shadows ── */
--shadow-sm: 0 2px 6px rgba(0, 0, 0, 0.20)
--shadow-md: 0 6px 18px rgba(0, 0, 0, 0.30), 0 0 0 1px rgba(135, 175, 255, 0.06)
--shadow-lg: 0 12px 32px rgba(0, 0, 0, 0.40), 0 0 0 1px rgba(135, 175, 255, 0.10)
--shadow-xl: 0 20px 48px rgba(0, 0, 0, 0.50), 0 0 0 1px rgba(135, 175, 255, 0.14)

/* ── Focus ring ── */
--focus-ring-color: rgba(135, 175, 255, 0.50)

/* ── Animations (keyframe names) ── */
/* fade-in-up, fade-in, scale-in, badge-glow, breathing, check-bounce,
   divider-glow, slide-down-reveal   (all defined in globals.css) */
```

---

## WAVE 1 — Foundation

**Must complete and commit before Wave 2 agents start.**

---

### Task W1: Foundation Agent — Design Tokens, Typography, Motion, Atmosphere

**Addresses:** H1 (fonts), H2 (color), H3 (keyframes), H4 (atmosphere), H5 (shadows), H6 (WCAG contrast)

**Files:**
- Modify: `apps/web/app/layout.tsx`
- Modify: `apps/web/app/globals.css`

**Context:**
`layout.tsx` currently imports `Outfit` (body) and `JetBrains_Mono` (code) from `next/font/google`. `globals.css` has OKLch-based tokens, 3 `@keyframes`, and Tailwind v4 imports. This task replaces the fonts, expands the design token system, adds 8 keyframes, strengthens the background, and fixes WCAG contrast failures.

---

**Step 1: Read both files in full before making any changes**

Read `apps/web/app/layout.tsx` and `apps/web/app/globals.css` completely.

---

**Step 2: Update font imports in `layout.tsx`**

Replace the existing font imports and body className:

```typescript
// REMOVE these two imports:
// import { JetBrains_Mono, Outfit } from 'next/font/google'

// ADD these three:
import { Space_Mono, Sora, JetBrains_Mono } from 'next/font/google'

// REPLACE outfit const:
const spaceMono = Space_Mono({
  variable: '--font-space-mono',
  subsets: ['latin'],
  weight: ['400', '700'],
})

// REPLACE jetbrainsMono const:
const sora = Sora({
  variable: '--font-sora',
  subsets: ['latin'],
  weight: ['300', '400', '500', '600', '700'],
})

const jetbrainsMono = JetBrains_Mono({
  variable: '--font-jetbrains-mono',
  weight: ['400', '500'],
  subsets: ['latin'],
})

// UPDATE body className in RootLayout:
<body className={`${spaceMono.variable} ${sora.variable} ${jetbrainsMono.variable} antialiased`}>
```

---

**Step 3: Add design token CSS variables to `globals.css`**

Append the following block to the `:root` section (after the existing `--axon-*` variables, before the dark mode section):

```css
/* ── Axon Design Tokens v2 ── */

/* Typography semantic mapping */
--font-display: var(--font-space-mono);
--font-sans: var(--font-sora);
--font-mono: var(--font-jetbrains-mono);

/* Brand palette — named correctly */
--axon-primary: #87afff;
--axon-primary-strong: #afd7ff;
--axon-secondary: #ff87af;
--axon-secondary-strong: #ff9ec0;

/* Surface tiers */
--surface-base: rgba(10, 18, 35, 0.85);
--surface-elevated: rgba(10, 18, 35, 0.60);
--surface-float: rgba(10, 18, 35, 0.35);

/* Borders */
--border-subtle: rgba(135, 175, 255, 0.15);
--border-standard: rgba(135, 175, 255, 0.28);
--border-strong: rgba(135, 175, 255, 0.40);
--border-accent: rgba(255, 135, 175, 0.25);

/* Text */
--text-primary: #e8f4f8;
--text-secondary: #b8cfe0;
--text-muted: #7a96b8;
--text-dim: #4d6a8a;

/* Shadows */
--shadow-sm: 0 2px 6px rgba(0, 0, 0, 0.20);
--shadow-md: 0 6px 18px rgba(0, 0, 0, 0.30), 0 0 0 1px rgba(135, 175, 255, 0.06);
--shadow-lg: 0 12px 32px rgba(0, 0, 0, 0.40), 0 0 0 1px rgba(135, 175, 255, 0.10);
--shadow-xl: 0 20px 48px rgba(0, 0, 0, 0.50), 0 0 0 1px rgba(135, 175, 255, 0.14);

/* Focus ring */
--focus-ring-color: rgba(135, 175, 255, 0.50);
```

---

**Step 4: Update `body` CSS in the `@layer base` section**

Find the existing `body { ... }` rule (around line 166) and replace it:

```css
body {
  background:
    radial-gradient(circle at 15% 35%, rgba(135, 175, 255, 0.22), transparent 42%),
    radial-gradient(circle at 85% 20%, rgba(255, 135, 175, 0.16), transparent 45%),
    radial-gradient(circle at 50% 80%, rgba(95, 175, 135, 0.07), transparent 50%),
    linear-gradient(180deg, #020812 0%, var(--axon-bg) 50%, #020812 100%);
  background-attachment: fixed;
  color: var(--axon-text-primary);
  font-family: var(--font-sans), system-ui, sans-serif;
  font-feature-settings: 'kern' 1, 'liga' 1;
  -webkit-font-smoothing: antialiased;
  line-height: 1.6;
  letter-spacing: 0.01em;
}

/* Grain overlay for texture and depth */
body::before {
  content: '';
  position: fixed;
  inset: 0;
  background-image: url("data:image/svg+xml,%3Csvg viewBox='0 0 200 200' xmlns='http://www.w3.org/2000/svg'%3E%3Cfilter id='n'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='0.9' numOctaves='4' stitchTiles='stitch'/%3E%3C/filter%3E%3Crect width='200' height='200' filter='url(%23n)' opacity='0.05'/%3E%3C/svg%3E");
  background-size: 200px 200px;
  pointer-events: none;
  z-index: 0;
}
```

---

**Step 5: Fix WCAG contrast violations**

Find `--axon-text-dim` in `:root` and update its value:
```css
/* OLD: --axon-text-dim: #5f87af; (fails WCAG AA ~3.2:1) */
--axon-text-dim: #7a96b8; /* ~5.1:1 contrast on #030712 */
```

Find the scrollbar thumb color (search for `rgba(255, 135, 175, 0.15)` or `rgba(255,135,175,.15)`) and update:
```css
/* OLD: rgba(255, 135, 175, 0.15) — fails WCAG */
/* NEW: */
scrollbar-color: rgba(135, 175, 255, 0.35) transparent; /* ~4.5:1 */
```

Also update the `::-webkit-scrollbar-thumb` rule to use the same value.

---

**Step 6: Add global font utility classes**

Append to the typography utilities section (after `.ui-dim-contrast`):

```css
/* Typography display classes */
.font-display {
  font-family: var(--font-display), 'Space Mono', monospace;
  letter-spacing: -0.01em;
}

.font-body {
  font-family: var(--font-sans), 'Sora', system-ui, sans-serif;
}

/* Apply display font to all headings globally */
h1, h2, h3, h4 {
  font-family: var(--font-display), 'Space Mono', monospace;
}
```

---

**Step 7: Add focus ring global rule**

Append to the `@layer base` section:

```css
/* Global accessible focus ring */
:focus-visible {
  outline: 2px solid var(--focus-ring-color);
  outline-offset: 2px;
}
```

---

**Step 8: Add motion keyframes**

Append 8 keyframes after the existing `@keyframes omnibox-progress { }` block:

```css
/* ── Motion Library v1 ── */

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
  to   { box-shadow: 0 0 12px rgba(135, 175, 255, 0.30); }
}

@keyframes slide-down-reveal {
  from { max-height: 0; opacity: 0; }
  to   { max-height: 600px; opacity: 1; }
}
```

---

**Step 9: Add Tailwind animation utility aliases**

After the keyframes block, add these utilities so Wave 2 agents can use short class names:

```css
@utility animate-fade-in-up {
  animation: fade-in-up 0.35s cubic-bezier(0.16, 1, 0.3, 1) forwards;
}

@utility animate-fade-in {
  animation: fade-in 0.25s ease-out forwards;
}

@utility animate-scale-in {
  animation: scale-in 0.2s ease-out forwards;
}

@utility animate-badge-glow {
  animation: badge-glow 1.6s ease-in-out infinite;
}

@utility animate-breathing {
  animation: breathing 1.6s ease-in-out infinite;
}

@utility animate-check-bounce {
  animation: check-bounce 0.4s cubic-bezier(0.34, 1.56, 0.64, 1) forwards;
}

@utility animate-slide-down {
  animation: slide-down-reveal 0.3s cubic-bezier(0.16, 1, 0.3, 1) forwards;
  overflow: hidden;
}
```

---

**Step 10: Verify build passes**

```bash
cd /home/jmagar/workspace/axon_rust/apps/web
pnpm run build 2>&1 | tail -30
```

Expected: Build completes without TypeScript errors. Font variables resolved. No CSS syntax errors.

---

**Step 11: Commit**

```bash
cd /home/jmagar/workspace/axon_rust
git add apps/web/app/layout.tsx apps/web/app/globals.css
git commit -m "feat(design): establish design token foundation — fonts, palette, motion, atmosphere, shadows, a11y"
```

---

## WAVE 2 — Parallel Component Implementation

**Start all 6 agents simultaneously after Wave 1 commit is verified.**
Each agent works in strict isolation — their file lists do not overlap.

---

### Task W2-A: Agent 2 — Pulse Chat Workspace

**Addresses:** H3-apply, H5-apply, H7, M1, M2, M5, M6, M7, M9, M13, M15, L1, L5, L6

**Files (exclusive ownership):**
- Modify: `apps/web/components/pulse/pulse-chat-pane.tsx`
- Modify: `apps/web/components/pulse/message-content.tsx`
- Modify: `apps/web/components/pulse/tool-badge.tsx`
- Modify: `apps/web/components/pulse/pulse-mobile-pane-switcher.tsx`
- Modify: `apps/web/components/pulse/pulse-workspace.tsx`
- Modify: `apps/web/components/pulse/pulse-toolbar.tsx`

**Context:**
Pulse is the primary workspace — Claude CLI chat on the right, rich text editor on the left, split by a draggable vertical divider. The chat pane streams messages from Claude with tool call badges, source citations, and thinking blocks. This agent fixes motion, depth, message bubble design, empty state, tool badge discoverability, mobile UX, and several small feedback issues.

**Read all 6 files in full before making any changes.**

---

#### Section A: `message-content.tsx` — M2, M7, L1, L6, H3-apply, H5-apply

**A.1 — Asymmetric message alignment (M2)**

Find the outer wrapper for each message. Currently both user and assistant messages are centered. Change to:

```tsx
// Wrap message content with alignment based on role
<div className={`flex w-full ${msg.role === 'user' ? 'justify-end' : 'justify-start'}`}>
  <article
    className={`
      w-full space-y-1.5 animate-fade-in-up
      ${msg.role === 'user' ? 'mr-4 max-w-[72%]' : 'ml-2 max-w-[80%]'}
    `}
    style={{ animationDelay: `${index * 25}ms` }}
  >
    {/* existing content */}
  </article>
</div>
```

The `index` prop must be threaded from the parent render. Add `index: number` to the message component props.

**A.2 — User bubble stronger gradient (H3-apply, H5-apply)**

Find the user message bubble container (the one with `linear-gradient(140deg,rgba(175,215,255,0.2),...)`). Replace its background and shadow:

```tsx
// OLD: bg-[linear-gradient(140deg,rgba(175,215,255,0.2),rgba(175,215,255,0.08))]
// OLD: shadow-[0_6px_18px_rgba(3,7,18,0.3)]

// NEW:
className="... bg-[linear-gradient(140deg,rgba(135,175,255,0.28),rgba(135,175,255,0.12))] shadow-[var(--shadow-md)] border-[var(--border-standard)] ..."
```

**A.3 — Thinking block: fade reveal + word count (M7)**

Find the `ThinkingBlock` component (lines 14-41). Make these changes:

1. Replace `{content.length} chars` with word count:
```tsx
const wordCount = content.trim().split(/\s+/).filter(Boolean).length
// Replace: {content.length} chars
// With:    {wordCount} {wordCount === 1 ? 'word' : 'words'}
```

2. Add fade-in animation to revealed content:
```tsx
{open && (
  <div className="border-t border-[rgba(167,139,250,0.15)] px-2.5 py-2 animate-fade-in">
    {/* existing content */}
  </div>
)}
```

3. Add hover state to the toggle button:
```tsx
<button
  type="button"
  onClick={() => setOpen((v) => !v)}
  className="flex w-full items-center gap-1.5 px-2.5 py-1.5 text-left hover:bg-[rgba(167,139,250,0.08)] transition-colors rounded-t-lg"
>
```

**A.4 — Copy button success animation (L1)**

Find the copy button (around line 204-215). Add state for animation:

```tsx
// Add state
const [copyAnim, setCopyAnim] = useState(false)

// Update handler:
onClick={() => {
  onCopyError(msg.content)
  setCopyAnim(true)
  setTimeout(() => setCopyAnim(false), 1400)
}}

// Update the icon/indicator:
{copyStatus === 'copied' ? (
  <Check className={`size-3 ${copyAnim ? 'animate-check-bounce' : ''} text-[var(--axon-success)]`} />
) : (
  <Copy className="size-3" />
)}

// Update button styling for 'copied' state:
className={`ui-chip inline-flex items-center gap-1 rounded border px-2 py-1 transition-all duration-200
  ${copyStatus === 'copied'
    ? 'border-[rgba(130,217,160,0.4)] bg-[rgba(130,217,160,0.12)] text-[var(--axon-success)]'
    : copyStatus === 'failed'
      ? 'border-[rgba(255,135,175,0.4)] bg-[rgba(255,135,175,0.08)] text-[var(--axon-secondary)]'
      : 'border-[rgba(95,135,175,0.3)] bg-[var(--surface-float)] text-[var(--text-dim)]'
  }`}
```

**A.5 — Timestamp legibility (L6)**

Find the timestamp span. Update:
```tsx
// OLD: text-[length:var(--text-2xs)] text-[var(--axon-text-dim)]
// NEW: text-[11px] text-[var(--text-muted)] font-medium
```

---

#### Section B: `pulse-chat-pane.tsx` — M1, M6, M9, H3-apply

**B.1 — Empty state redesign (M1)**

Find the empty state block (lines 357-367). Replace entirely:

```tsx
{messages.length === 0 ? (
  <div className="flex h-full items-center justify-center p-6">
    <div className="flex max-w-sm flex-col items-center gap-4 rounded-xl border border-[var(--border-standard)] bg-[linear-gradient(135deg,rgba(135,175,255,0.08),rgba(255,135,175,0.05))] p-6 text-center shadow-[var(--shadow-lg)] animate-scale-in">
      <div className="relative">
        <div className="absolute inset-0 bg-[radial-gradient(circle,rgba(135,175,255,0.25),transparent_60%)] blur-xl" />
        <MessageCircle className="relative size-10 text-[var(--axon-primary)]" />
      </div>
      <div className="space-y-1.5">
        <h2 className="font-display text-base font-semibold text-[var(--text-primary)]">
          Start a conversation
        </h2>
        <p className="text-sm leading-relaxed text-[var(--text-secondary)]">
          Ask Claude to write, analyze, or explore. Paste a URL in the omnibox to run a tool on a webpage.
        </p>
      </div>
      <div className="flex flex-wrap justify-center gap-2 pt-1">
        <span className="inline-flex items-center gap-1 rounded-full border border-[var(--border-subtle)] bg-[rgba(135,175,255,0.08)] px-2.5 py-1 text-xs text-[var(--axon-primary)]">
          <Send className="size-2.5" />
          Ask a question
        </span>
        <span className="inline-flex items-center gap-1 rounded-full border border-[var(--border-subtle)] bg-[rgba(255,135,175,0.08)] px-2.5 py-1 text-xs text-[var(--axon-secondary)]">
          Paste a URL
        </span>
      </div>
    </div>
  </div>
) : (
```

Add required imports at top: `import { MessageCircle, Send } from 'lucide-react'`

**B.2 — Source list animation (M6)**

Find the `sourceListOpen && activeSources.length > 0` block (lines 297-335). Wrap the expanding container:

```tsx
{sourceListOpen && activeSources.length > 0 && (
  <div className="animate-slide-down overflow-hidden">
    {/* existing source list content unchanged */}
  </div>
)}
```

Find the source toggle button. Update label to reflect state:
```tsx
// Find the "+N more" text and replace with dynamic label:
{sourceListOpen ? `Hide sources` : `+${hiddenCount} more`}
```

**B.3 — Enhanced loading indicator (M9, H3-apply)**

Find the loading indicator block (lines 393-410). Replace:

```tsx
{isLoading && (
  <div className="flex items-start gap-3 rounded-xl border border-[var(--border-subtle)] bg-[var(--surface-elevated)] px-3 py-2.5 shadow-[var(--shadow-sm)] animate-fade-in">
    <div className="mt-0.5 flex shrink-0 gap-0.5">
      {[0, 1, 2].map((i) => (
        <span
          key={i}
          className="inline-block size-1.5 rounded-full bg-[var(--axon-primary)]"
          style={{
            animation: `breathing 1.4s ease-in-out ${i * 200}ms infinite`,
          }}
        />
      ))}
    </div>
    <div className="min-w-0 flex-1">
      <span className="animate-breathing text-sm text-[var(--text-secondary)]">
        {formatStreamPhaseLabel(streamingPhase)}…
      </span>
    </div>
    {/* existing stop button unchanged */}
  </div>
)}
```

---

#### Section C: `tool-badge.tsx` — M5, H3-apply

**C.1 — Badge hover animation (H3-apply)**

Find the badge button element. Add hover animation and glow class:
```tsx
<button
  type="button"
  onClick={() => setPinned((v) => !v)}
  className={`
    inline-flex size-5 items-center justify-center rounded border transition-all duration-150
    hover:scale-110 hover:animate-badge-glow
    ${style.border} ${style.bg} ${style.label}
  `}
  aria-label={`${tool.name} — click to pin`}
  title={`${tool.name} — click to pin`}
>
```

**C.2 — Pin indicator dot (M5)**

Add a visible indicator when pinned. Wrap the button in a relative container and add the dot:

```tsx
<div ref={ref} className="relative inline-flex" onMouseEnter={...} onMouseLeave={...}>
  <button ...>
    <CategoryIcon category={category} />
  </button>

  {/* Pin indicator — appears when tooltip is pinned */}
  {pinned && (
    <span
      className="pointer-events-none absolute -right-0.5 -top-0.5 size-2 rounded-full bg-[var(--axon-primary)] ring-1 ring-[var(--axon-bg)] animate-fade-in"
      aria-label="pinned"
    />
  )}

  {/* Existing tooltip — unchanged */}
  {isOpen && (...)}
</div>
```

---

#### Section D: `pulse-mobile-pane-switcher.tsx` — M13

Replace the component entirely with a labeled tab implementation:

```tsx
'use client'

import { PenLine, MessageSquare } from 'lucide-react'
import type { MobilePane } from '@/lib/pulse/types'

interface PulseMobilePaneSwitcherProps {
  mobilePane: MobilePane
  onMobilePaneChange: (pane: MobilePane) => void
}

export function PulseMobilePaneSwitcher({
  mobilePane,
  onMobilePaneChange,
}: PulseMobilePaneSwitcherProps) {
  return (
    <div
      role="tablist"
      aria-label="Workspace pane"
      className="inline-flex items-center gap-1 rounded-lg border border-[var(--border-subtle)] bg-[var(--surface-base)] p-1"
    >
      <button
        type="button"
        role="tab"
        aria-selected={mobilePane === 'chat'}
        aria-label="Chat pane"
        onClick={() => onMobilePaneChange('chat')}
        className={`inline-flex items-center gap-1.5 rounded-md px-2.5 py-1.5 text-xs font-medium transition-all duration-200 ${
          mobilePane === 'chat'
            ? 'bg-[var(--axon-primary)] text-[var(--axon-bg)] shadow-[var(--shadow-sm)]'
            : 'text-[var(--text-muted)] hover:text-[var(--text-secondary)]'
        }`}
      >
        <MessageSquare className="size-3.5" />
        <span>Chat</span>
      </button>
      <button
        type="button"
        role="tab"
        aria-selected={mobilePane === 'editor'}
        aria-label="Editor pane"
        onClick={() => onMobilePaneChange('editor')}
        className={`inline-flex items-center gap-1.5 rounded-md px-2.5 py-1.5 text-xs font-medium transition-all duration-200 ${
          mobilePane === 'editor'
            ? 'bg-[var(--axon-secondary)] text-[var(--axon-bg)] shadow-[var(--shadow-sm)]'
            : 'text-[var(--text-muted)] hover:text-[var(--text-secondary)]'
        }`}
      >
        <PenLine className="size-3.5" />
        <span>Edit</span>
      </button>
    </div>
  )
}
```

---

#### Section E: `pulse-workspace.tsx` — M15, H5-apply

**E.1 — Split pane divider (M15)**

Find the divider element (around line 348-362). Replace:

```tsx
<div
  ref={splitHandleRef}
  role="separator"
  aria-label="Resize pane — drag left/right"
  aria-orientation="vertical"
  aria-valuenow={Math.round(desktopSplitPercent)}
  aria-valuemin={20}
  aria-valuemax={80}
  className="group flex w-3 cursor-col-resize items-center justify-center rounded-sm transition-colors hover:bg-[var(--border-subtle)]"
  onPointerDown={(event) => {
    dragStartRef.current = {
      pointerX: event.clientX,
      startPercent: desktopSplitPercent,
    }
    splitHandleRef.current?.classList.add('bg-[var(--border-standard)]', 'animate-[divider-glow_0.2s_ease-out_forwards]')
  }}
>
  {/* Drag handle dots */}
  <div className="flex flex-col gap-0.5 opacity-30 group-hover:opacity-70 transition-opacity">
    {[0, 1, 2, 3, 4].map((i) => (
      <div key={i} className="size-0.5 rounded-full bg-[var(--text-muted)]" />
    ))}
  </div>
</div>
```

Note: Update `aria-valuenow` to be dynamic — update it via the existing drag handler using `setAttribute('aria-valuenow', Math.round(newPercent).toString())`.

**E.2 — Workspace container shadow (H5-apply)**

Find the main workspace container. Add shadow token:
```tsx
// Add to the workspace container className:
shadow-[var(--shadow-md)]
```

---

#### Section F: `pulse-toolbar.tsx` — L5

Find the document title input. Add unsaved state indicator:

```tsx
// Add to component props or local state if title change is tracked locally:
const [isDirty, setIsDirty] = useState(false)

<div className="relative flex min-w-0 flex-1">
  <input
    id="pulse-document-title"
    name="pulse_document_title"
    value={title}
    onChange={(e) => {
      onTitleChange(e.target.value)
      setIsDirty(true)
    }}
    className="... focus:border-[var(--border-standard)] focus:bg-[var(--surface-elevated)]"
    placeholder="Document title..."
  />
  {isDirty && (
    <span
      className="pointer-events-none absolute right-2 top-1/2 -translate-y-1/2 size-1.5 rounded-full bg-[var(--axon-secondary)] animate-pulse"
      title="Unsaved changes"
    />
  )}
</div>
```

If onTitleChange triggers a save, reset `isDirty` to false after save completes (this may require a `onSaved` prop or debounced auto-save signal).

---

**Final step — Pulse Agent: verify build**

```bash
cd /home/jmagar/workspace/axon_rust/apps/web
pnpm run build 2>&1 | grep -E "error|Error|✓" | tail -20
```

Expected: No TypeScript errors in modified files.

```bash
git add apps/web/components/pulse/
git commit -m "feat(pulse): motion, empty state, message alignment, tool badge discoverability, mobile pane labels, divider improvements"
```

---

### Task W2-B: Agent 3 — Results & Data Visualization

**Addresses:** H3-apply, H5-apply, H7, H8, M1, M8, M9, M10, L1, L7

**Files (exclusive ownership):**
- Modify: `apps/web/components/results/table-renderer.tsx`
- Modify: `apps/web/components/results/doctor-report.tsx`
- Modify: `apps/web/components/results/raw-renderer.tsx`
- Modify: `apps/web/components/results-panel.tsx`
- Modify: `apps/web/components/content-viewer.tsx`
- Modify: `apps/web/components/crawl-file-explorer.tsx`
- Modify: `apps/web/components/command-options-panel.tsx`

**Context:**
The results panel is the output surface of the entire RAG system. It dispatches to different renderers based on command type. The table-renderer handles `sources`, `query`, and other tabular results — it can receive 100k+ rows with no pagination. The doctor-report shows service diagnostics. The crawl file explorer shows discovered pages. These components need virtual scrolling, staggered animations, better empty states, and keyboard focus fixes.

**Read all 7 files in full before making any changes.**

---

#### Section A: `table-renderer.tsx` — H8, H3-apply, L7

**A.1 — Virtual scrolling for large datasets (H8)**

This is the most critical fix. The table currently renders all rows into the DOM — for `sources` with 100k+ entries this freezes the browser.

Import the virtualizer:
```bash
# Check if @tanstack/react-virtual is already in package.json:
cat apps/web/package.json | grep virtual
# If not present, install:
cd apps/web && pnpm add @tanstack/react-virtual
```

Add virtual scrolling to the table body. Find the `<tbody>` that maps over `rows` and replace:

```tsx
import { useVirtualizer } from '@tanstack/react-virtual'
import { useRef } from 'react'

// Inside the table component, after the rows are prepared:
const parentRef = useRef<HTMLDivElement>(null)
const VIRTUAL_THRESHOLD = 200  // only virtualize if >200 rows

const shouldVirtualize = rows.length > VIRTUAL_THRESHOLD

const rowVirtualizer = useVirtualizer({
  count: shouldVirtualize ? rows.length : 0,
  getScrollElement: () => parentRef.current,
  estimateSize: () => 32,
  overscan: 10,
})

// Wrap the table in a scrollable container:
<div
  ref={parentRef}
  className="max-h-[60vh] overflow-auto"
  style={shouldVirtualize ? { height: '60vh' } : undefined}
>
  <table className="w-full table-fixed text-left">
    <thead>...</thead>
    {shouldVirtualize ? (
      <tbody
        style={{ height: `${rowVirtualizer.getTotalSize()}px`, position: 'relative' }}
      >
        {rowVirtualizer.getVirtualItems().map((virtualRow) => {
          const row = rows[virtualRow.index]
          return (
            <tr
              key={virtualRow.key}
              data-index={virtualRow.index}
              ref={rowVirtualizer.measureElement}
              style={{
                position: 'absolute',
                top: 0,
                left: 0,
                width: '100%',
                transform: `translateY(${virtualRow.start}px)`,
              }}
              className="border-b border-[var(--border-subtle)] hover:bg-[var(--surface-float)]"
            >
              {/* existing cell content */}
            </tr>
          )
        })}
      </tbody>
    ) : (
      <tbody>
        {rows.map((row, idx) => (
          <tr
            key={row.key ?? idx}
            className="border-b border-[var(--border-subtle)] hover:bg-[var(--surface-float)] transition-colors"
            style={{
              animation: `fade-in-up 0.3s ease-out ${idx * 20}ms forwards`,
              opacity: 0,
            }}
          >
            {/* existing cell content */}
          </tr>
        ))}
      </tbody>
    )}
  </table>
</div>
```

**A.2 — Top-N toggle for large tables (L7)**

Add a row count toggle above the table when rows exceed 1000:

```tsx
const [showAll, setShowAll] = useState(false)
const DISPLAY_LIMIT = 100
const displayRows = rows.length > 1000 && !showAll ? rows.slice(0, DISPLAY_LIMIT) : rows

// Render above the table:
{rows.length > 1000 && (
  <div className="mb-2 flex items-center justify-between">
    <span className="text-xs text-[var(--text-muted)]">
      {showAll ? `All ${rows.length.toLocaleString()} rows` : `Top ${DISPLAY_LIMIT} of ${rows.length.toLocaleString()} rows`}
    </span>
    <button
      type="button"
      onClick={() => setShowAll((v) => !v)}
      className="text-xs text-[var(--axon-primary)] hover:text-[var(--axon-primary-strong)] transition-colors"
    >
      {showAll ? 'Show top 100' : `Show all ${rows.length.toLocaleString()}`}
    </button>
  </div>
)}
```

---

#### Section B: `doctor-report.tsx` — M8, M10, H3-apply

**B.1 — Failure-first grouping (M8)**

Find where `serviceEntries` is constructed and sorted (around line 129-131). Replace the sort with explicit grouping:

```tsx
const allEntries = Object.entries(data.services)
const failedEntries = allEntries.filter(([, s]) => !s.ok)
const healthyEntries = allEntries.filter(([, s]) => s.ok)
```

Update the render to show grouped sections:

```tsx
<div className="space-y-3">
  {failedEntries.length > 0 && (
    <div>
      <div className="mb-2 flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wide text-[var(--axon-secondary)]">
        <AlertTriangle className="size-3" />
        {failedEntries.length} {failedEntries.length === 1 ? 'service' : 'services'} down
      </div>
      <ServiceRows entries={failedEntries} />
    </div>
  )}
  {failedEntries.length > 0 && healthyEntries.length > 0 && (
    <div className="border-t border-[var(--border-subtle)] my-3" />
  )}
  {healthyEntries.length > 0 && (
    <div>
      <div className="mb-2 flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wide text-[var(--axon-success)]">
        <CheckCircle2 className="size-3" />
        {healthyEntries.length} {healthyEntries.length === 1 ? 'service' : 'services'} healthy
      </div>
      <ServiceRows entries={healthyEntries} />
    </div>
  )}
</div>
```

Add imports: `import { AlertTriangle, CheckCircle2 } from 'lucide-react'`

**B.2 — Asymmetric metric grid (M10)**

Find the metrics tile grid (lines 173-178). Replace uniform 4-col grid:

```tsx
<div className="grid gap-4 md:grid-cols-3">
  {/* Primary metric spans 2 columns */}
  <div className="md:col-span-2">
    <MetricTile
      label="System Status"
      value={failedEntries.length === 0 ? 'Healthy' : `${failedEntries.length} down`}
      status={failedEntries.length === 0 ? 'ok' : 'error'}
      size="large"
    />
  </div>
  {/* Two compact metrics stacked */}
  <div className="flex flex-col gap-3">
    <MetricTile label="Services Up" value={`${healthyEntries.length}`} status="ok" size="small" />
    <MetricTile label="Services Down" value={`${failedEntries.length}`} status={failedEntries.length > 0 ? 'error' : 'ok'} size="small" />
  </div>
</div>
```

Update `MetricTile` to accept a `size` prop:
```tsx
interface MetricTileProps {
  label: string
  value: string | number
  status?: 'ok' | 'error' | 'neutral'
  size?: 'large' | 'small'
}
```

Add `animate-fade-in-up` to each tile with stagger:
```tsx
<div className="animate-fade-in-up" style={{ animationDelay: `${idx * 50}ms` }}>
  <MetricTile ... />
</div>
```

---

#### Section C: `raw-renderer.tsx` — M1, M9

**C.1 — Contextual empty state (M1)**

Find the empty state block (lines 26-29). Replace:

```tsx
if (!hasJson && !hasLines && !isProcessing) {
  return (
    <div className="flex h-40 flex-col items-center justify-center gap-3">
      <div className="size-8 rounded-full border border-[var(--border-subtle)] bg-[var(--surface-elevated)] flex items-center justify-center">
        <TerminalSquare className="size-4 text-[var(--text-dim)]" />
      </div>
      <div className="text-center">
        <p className="text-sm font-medium text-[var(--text-secondary)]">No output yet</p>
        <p className="text-xs text-[var(--text-muted)] mt-0.5">Run a command to see results here</p>
      </div>
    </div>
  )
}
```

**C.2 — Enhanced processing state (M9)**

Find the `isProcessing` check and update its spinner:

```tsx
if (isProcessing && !hasJson && !hasLines) {
  return (
    <div className="flex h-40 flex-col items-center justify-center gap-3 animate-fade-in">
      <div className="flex gap-1">
        {[0, 1, 2].map((i) => (
          <span
            key={i}
            className="inline-block size-1.5 rounded-full bg-[var(--axon-primary)]"
            style={{ animation: `breathing 1.4s ease-in-out ${i * 180}ms infinite` }}
          />
        ))}
      </div>
      <div className="text-center">
        <p className="text-sm text-[var(--text-secondary)] animate-breathing">Processing…</p>
        <p className="text-xs text-[var(--text-muted)] mt-0.5">Large operations may take several minutes</p>
      </div>
    </div>
  )
}
```

Add imports: `import { TerminalSquare } from 'lucide-react'`

---

#### Section D: `crawl-file-explorer.tsx` — H7

Find all `role="button"` elements in the file list (around line 233-295). Add focus ring and keyboard navigation to each:

```tsx
<div
  key={file.relative_path}
  role="button"
  tabIndex={0}
  onClick={() => handleSelect(file.relative_path)}
  onKeyDown={(e) => {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault()
      handleSelect(file.relative_path)
    }
  }}
  className={`
    cursor-pointer border-b border-[var(--border-subtle)] px-3 py-2 transition-colors
    focus-visible:outline-2 focus-visible:outline-[var(--focus-ring-color)] focus-visible:outline-offset-[-2px] focus-visible:rounded-sm
    ${isActive
      ? 'border-l-2 border-l-[var(--axon-secondary)] bg-[rgba(255,135,175,0.08)]'
      : 'border-l-2 border-l-transparent hover:bg-[var(--surface-float)]'
    }
  `}
>
```

---

#### Section E: `command-options-panel.tsx` — H7

Find all interactive checkbox/button elements. Add focus-visible ring:

```tsx
// On each option button/checkbox:
className={`
  flex size-4 shrink-0 items-center justify-center rounded border transition-all
  focus-visible:outline-2 focus-visible:outline-[var(--focus-ring-color)]
  ${value
    ? 'border-[var(--axon-secondary)] bg-[rgba(255,135,175,0.18)]'
    : 'border-[var(--border-accent)] bg-transparent hover:border-[var(--border-strong)]'
  }
`}
```

---

#### Section F: `content-viewer.tsx` — L1

Find the CopyButton usage. Add copy success state:

```tsx
const [copied, setCopied] = useState(false)

// Wrap or replace CopyButton usage:
<button
  type="button"
  onClick={() => {
    navigator.clipboard.writeText(markdown ?? '')
    setCopied(true)
    setTimeout(() => setCopied(false), 1500)
  }}
  className={`inline-flex items-center gap-1 rounded border px-2 py-1 text-xs transition-all duration-200 ${
    copied
      ? 'border-[rgba(130,217,160,0.4)] bg-[rgba(130,217,160,0.12)] text-[var(--axon-success)]'
      : 'border-[var(--border-subtle)] bg-[var(--surface-float)] text-[var(--text-muted)] hover:text-[var(--text-secondary)]'
  }`}
>
  {copied ? (
    <Check className="size-3 animate-check-bounce" />
  ) : (
    <Copy className="size-3" />
  )}
  {copied ? 'Copied' : 'Copy'}
</button>
```

---

**Final step — Results Agent: verify and commit**

```bash
cd /home/jmagar/workspace/axon_rust/apps/web
pnpm run build 2>&1 | grep -E "error|Error" | head -20
```

```bash
git add apps/web/components/results/ apps/web/components/results-panel.tsx apps/web/components/content-viewer.tsx apps/web/components/crawl-file-explorer.tsx apps/web/components/command-options-panel.tsx
git commit -m "feat(results): virtual scrolling, stagger animations, doctor report redesign, empty states, focus rings"
```

---

### Task W2-C: Agent 4 — UI Primitives

**Addresses:** H3-apply, H5-apply, H6, H7, L9

**Files (exclusive ownership):**
- Modify: `apps/web/components/ui/button.tsx`
- Modify: `apps/web/components/ui/input.tsx`
- Modify: `apps/web/components/ui/tabs.tsx`
- Modify: `apps/web/components/ui/dropdown-menu.tsx`
- Modify: `apps/web/components/ui/badge.tsx`
- Modify: `apps/web/components/ui/scroll-area.tsx`

**Context:**
These are the foundational shadcn/ui primitives. They use CVA (class-variance-authority) for variant styling. The goal is to add branded hover micro-interactions, fix the focus ring to use the new CSS token, and replace any hardcoded rgba values with design tokens.

**Read all 6 files in full before making any changes.**

---

#### Section A: `button.tsx` — H3-apply, H5-apply, H7

Find the base `buttonVariants` CVA definition. The base class currently includes `transition-all`. Update:

```tsx
// In the base className string of buttonVariants, add or update:
'transition-all duration-150 hover:scale-[1.03] active:scale-[0.98]',
'focus-visible:outline-2 focus-visible:outline-[var(--focus-ring-color)] focus-visible:outline-offset-2',
'disabled:opacity-50 disabled:cursor-not-allowed disabled:scale-100',
```

For the primary variant, add a glow on hover:
```tsx
// In the 'default' variant:
'hover:shadow-[0_4px_14px_rgba(135,175,255,0.25)]',
```

---

#### Section B: `input.tsx` — H7, H3-apply

Find the input className. Replace `focus-visible:ring-ring/50` with branded focus:

```tsx
// Replace or augment focus-visible classes:
'focus-visible:outline-2 focus-visible:outline-[var(--focus-ring-color)] focus-visible:outline-offset-0',
'focus-visible:bg-[var(--surface-elevated)]',
'transition-all duration-150',
```

---

#### Section C: `tabs.tsx` — H7

Find the `TabsTrigger` className. Add focus-visible ring and hover transition:

```tsx
// In TabsTrigger className:
'focus-visible:outline-2 focus-visible:outline-[var(--focus-ring-color)] focus-visible:outline-offset-1 focus-visible:rounded-sm',
'transition-all duration-150',
```

---

#### Section D: `dropdown-menu.tsx` — H7

Find `DropdownMenuItem` and `DropdownMenuCheckboxItem`. Add focus styling consistent with the design system:

```tsx
// Replace focus:bg-accent with branded focus:
'focus:bg-[var(--surface-elevated)] focus:outline-none',
// Keep focus visible ring:
'data-[highlighted]:outline-2 data-[highlighted]:outline-[var(--focus-ring-color)]',
```

---

#### Section E: `scroll-area.tsx` — H6

Find the `ScrollAreaScrollbar` and `ScrollAreaThumb` class definitions. Update the thumb color to fix WCAG contrast:

```tsx
// ScrollAreaThumb — replace opacity-based color with explicit accessible value:
// OLD: bg-border  (which resolves to low-opacity pink)
// NEW:
'bg-[rgba(135,175,255,0.35)] hover:bg-[rgba(135,175,255,0.55)] transition-colors',
```

---

#### Section F: All files — L9 (hardcoded rgba audit)

For each file, search for any hardcoded `rgba(` values. Replace with the closest CSS token from the design system:

- `rgba(255, 135, 175, 0.X)` → `var(--border-accent)` or `var(--axon-secondary)` (adjust opacity via bg-opacity if needed)
- `rgba(175, 215, 255, 0.X)` → `var(--border-subtle)` / `var(--axon-primary)`
- `rgba(10, 18, 35, 0.X)` → `var(--surface-base)` / `var(--surface-elevated)` / `var(--surface-float)`

If an exact token doesn't exist for a specific opacity, keep the inline value — don't force a wrong token.

---

**Final step — UI Primitives Agent: verify and commit**

```bash
cd /home/jmagar/workspace/axon_rust/apps/web
pnpm run build 2>&1 | grep -E "error|Error" | head -20
```

```bash
git add apps/web/components/ui/
git commit -m "feat(ui): button/input hover micro-interactions, branded focus rings, scrollbar contrast fix"
```

---

### Task W2-D: Agent 5 — Omnibox & Navigation

**Addresses:** H3-apply, M4, M11, L2

**Files (exclusive ownership):**
- Modify: `apps/web/components/omnibox.tsx`

**Context:**
The omnibox is the primary command interface — it accepts URLs, questions, and slash commands, dispatches to the correct mode, and shows execution status. It's 987 lines. Issues: status bar disappears on completion (user doesn't know if command succeeded), @mention feature is undiscoverable, and the mode suggestion list appears all at once without animation.

**Read the full file before making any changes.**

---

#### Section A: Status bar persistence (M4)

Find the status bar / elapsed time display (around line 630-648). Currently `statusText` disappears when empty.

Add a `completionStatus` state to persist last result:

```tsx
const [completionStatus, setCompletionStatus] = useState<{
  type: 'done' | 'error' | null
  text: string
  exitCode?: number
} | null>(null)

// When statusType becomes 'done' or 'error', capture it:
useEffect(() => {
  if (statusType === 'done' || statusType === 'error') {
    setCompletionStatus({ type: statusType, text: statusText, exitCode })
    // Auto-clear after 4 seconds
    const t = setTimeout(() => setCompletionStatus(null), 4000)
    return () => clearTimeout(t)
  }
}, [statusType, statusText, exitCode])
```

Update the status render to show completion state when statusText is gone:

```tsx
{/* Status bar */}
{(statusText || completionStatus) && (
  <div className={`flex items-center gap-1.5 text-xs transition-all duration-200 ${
    completionStatus?.type === 'error'
      ? 'text-[var(--axon-secondary)]'
      : completionStatus?.type === 'done'
        ? 'text-[var(--axon-success)]'
        : 'text-[var(--text-muted)]'
  }`}>
    {completionStatus?.type === 'done' && <CheckCircle2 className="size-3" />}
    {completionStatus?.type === 'error' && <XCircle className="size-3" />}
    <span>{statusText || completionStatus?.text}</span>
    {completionStatus?.exitCode !== undefined && completionStatus.exitCode !== 0 && (
      <span className="text-[var(--text-dim)]">exit {completionStatus.exitCode}</span>
    )}
  </div>
)}
```

Add imports: `import { CheckCircle2, XCircle } from 'lucide-react'`

---

#### Section B: @mention discoverability (M11)

Find the input field render (around line 619-628). Below the placeholder text, add a one-time hint:

```tsx
// Add state to track if tip has been shown:
const [mentionTipSeen, setMentionTipSeen] = useState(() => {
  if (typeof window === 'undefined') return true
  return localStorage.getItem('axon-mention-tip-seen') === '1'
})

// In the input container, after the input element, add:
{!mentionTipSeen && isFocused && !inputValue && (
  <div
    className="absolute -bottom-6 left-2 text-[10px] text-[var(--text-dim)] animate-fade-in"
    onMouseDown={(e) => e.preventDefault()}
  >
    Tip: type <kbd className="rounded border border-[var(--border-subtle)] px-1 font-mono text-[10px] text-[var(--text-muted)]">@</kbd> to attach a file
    <button
      type="button"
      onClick={() => {
        setMentionTipSeen(true)
        localStorage.setItem('axon-mention-tip-seen', '1')
      }}
      className="ml-2 text-[var(--text-dim)] hover:text-[var(--text-muted)]"
    >
      ✕
    </button>
  </div>
)}
```

---

#### Section C: Staggered suggestion reveals (L2, H3-apply)

Find the `visibleItems.map(...)` in the mode suggestion dropdown (around line 883-907). Add stagger animation:

```tsx
{visibleItems.map((m, idx) => (
  <button
    key={m.id}
    type="button"
    // ... existing props ...
    className="... animate-fade-in-up"
    style={{ animationDelay: `${idx * 35}ms`, animationFillMode: 'backwards' }}
  >
    {/* existing content */}
  </button>
))}
```

---

**Final step — Omnibox Agent: verify and commit**

```bash
cd /home/jmagar/workspace/axon_rust/apps/web
pnpm run build 2>&1 | grep -E "error|Error" | head -20
```

```bash
git add apps/web/components/omnibox.tsx
git commit -m "feat(omnibox): status bar persistence, @mention discovery tip, staggered suggestions"
```

---

### Task W2-E: Agent 6 — Pages (Settings, MCP, Agents)

**Addresses:** M1, M3, M12, M14, L3, L8

**Files (exclusive ownership):**
- Modify: `apps/web/app/mcp/page.tsx`
- Modify: `apps/web/app/mcp/components.tsx`
- Modify: `apps/web/app/settings/page.tsx`
- Modify: `apps/web/app/agents/page.tsx`

**Context:**
These are Next.js App Router page components. Settings page has a sidebar + centered form layout. MCP page manages Model Context Protocol server configurations (add, edit, delete). Agents page lists Claude CLI agents. Issues: inline delete confirmations, empty states without guidance, settings section headers with no typographic hierarchy, MCP form with two separate save handlers, and the "checking" status badge being visually ambiguous.

**Read all 4 files in full before making any changes.**

---

#### Section A: Modal dialog for destructive actions (M3)

**In `mcp/page.tsx`:**

Currently `deleteTarget === name` shows an inline confirmation. Replace with a modal:

```tsx
// Add a modal state:
const [deleteModal, setDeleteModal] = useState<{ name: string } | null>(null)

// Replace the inline confirmation render with:
{deleteModal && (
  <div className="fixed inset-0 z-50 flex items-center justify-center bg-[rgba(3,7,18,0.75)] backdrop-blur-sm animate-fade-in">
    <div className="w-full max-w-sm rounded-xl border border-[var(--border-standard)] bg-[var(--surface-base)] p-5 shadow-[var(--shadow-xl)] animate-scale-in">
      <div className="mb-1 flex items-center gap-2">
        <Trash2 className="size-4 text-[var(--axon-secondary)]" />
        <h3 className="font-display text-sm font-semibold text-[var(--text-primary)]">
          Delete "{deleteModal.name}"?
        </h3>
      </div>
      <p className="mb-4 text-xs text-[var(--text-muted)]">
        This MCP server configuration will be permanently removed. You can add it back later.
      </p>
      <div className="flex justify-end gap-2">
        <button
          type="button"
          onClick={() => setDeleteModal(null)}
          className="rounded-md border border-[var(--border-subtle)] bg-transparent px-3 py-1.5 text-xs text-[var(--text-secondary)] hover:bg-[var(--surface-float)] transition-colors"
        >
          Cancel
        </button>
        <button
          type="button"
          onClick={() => {
            deleteServer(deleteModal.name)
            setDeleteModal(null)
          }}
          className="rounded-md bg-[rgba(255,135,175,0.15)] border border-[var(--border-accent)] px-3 py-1.5 text-xs text-[var(--axon-secondary)] hover:bg-[rgba(255,135,175,0.25)] transition-colors"
        >
          Delete
        </button>
      </div>
    </div>
  </div>
)}

// Replace onDelete={() => setDeleteTarget(name)} with:
// onDelete={() => setDeleteModal({ name })}
```

Remove the old `deleteTarget` state and the inline confirmation render entirely.

**In `settings/page.tsx`:**

Find the reset confirmation pattern (if present, around lines 303-422). Apply the same modal pattern. Replace any `showResetConfirm` inline state toggle with a `resetModal` state rendered as a centered overlay modal.

---

#### Section B: MCP form single save button (M14)

**In `mcp/components.tsx`:**

Find the two separate save handlers (`handleSaveForm` and `handleSaveJson`). Unify to a single save:

```tsx
const handleSave = useCallback(() => {
  // Determine which mode is active and save accordingly
  if (activeTab === 'form') {
    handleSaveForm()
  } else {
    handleSaveJson()
  }
}, [activeTab, handleSaveForm, handleSaveJson])

// Remove save buttons from inside each tab
// Add a single sticky save button below the tab panel:
<div className="sticky bottom-0 border-t border-[var(--border-subtle)] bg-[var(--surface-base)] p-3">
  <div className="flex items-center justify-between gap-3">
    {activeTab === 'json' && (
      <span className="text-xs text-[var(--text-dim)]">JSON reflects form values</span>
    )}
    <button
      type="button"
      onClick={handleSave}
      className="ml-auto rounded-md bg-[rgba(135,175,255,0.15)] border border-[var(--border-standard)] px-4 py-1.5 text-xs font-medium text-[var(--axon-primary)] hover:bg-[rgba(135,175,255,0.25)] transition-all hover:scale-[1.02]"
    >
      {isEditing ? 'Save changes' : 'Add server'}
    </button>
  </div>
</div>
```

**MCP status "Checking" fix (L8):**

Find the `STATUS_DOT` object. Replace `checking` entry:

```tsx
const STATUS_DOT: Record<McpServerStatus, string> = {
  online: 'bg-green-400 shadow-[0_0_6px_rgba(74,222,128,0.7)]',
  offline: 'bg-red-400',
  unknown: 'bg-[var(--border-standard)]',  // more visible than rgba(255,255,255,0.2)
  checking: 'bg-[var(--axon-primary)] animate-pulse',  // blue pulse, not yellow
}
```

---

#### Section C: Settings section header typography (M12)

**In `settings/page.tsx`:**

Find the `SectionHeader` component (around line 115-142). Update the heading to use display font and increase visual hierarchy:

```tsx
function SectionHeader({ icon: Icon, label, description }: { ... }) {
  return (
    <div className="mb-5">
      <div className="flex items-center gap-3">
        <div className="flex size-8 shrink-0 items-center justify-center rounded-lg border border-[var(--border-accent)] bg-[rgba(255,135,175,0.08)] shadow-[var(--shadow-sm)]">
          <Icon className="size-4 text-[var(--axon-secondary)]" />
        </div>
        <h2 className="font-display text-base font-bold text-[var(--text-primary)]">{label}</h2>
      </div>
      {description && (
        <p className="mt-1.5 ml-11 text-sm leading-relaxed text-[var(--text-muted)]">{description}</p>
      )}
    </div>
  )
}
```

---

#### Section D: Empty states (M1)

**In `mcp/page.tsx`:** Find the empty state (lines 211-231). Replace:

```tsx
<div className="flex h-full min-h-[300px] flex-col items-center justify-center gap-4 rounded-xl border border-dashed border-[var(--border-subtle)] bg-[var(--surface-float)] p-8 text-center animate-fade-in">
  <div className="relative">
    <div className="absolute inset-0 bg-[radial-gradient(circle,rgba(135,175,255,0.15),transparent)] blur-xl" />
    <Network className="relative size-10 text-[var(--axon-primary)]" />
  </div>
  <div className="space-y-1.5">
    <h3 className="font-display text-sm font-semibold text-[var(--text-primary)]">No MCP servers configured</h3>
    <p className="max-w-xs text-xs leading-relaxed text-[var(--text-muted)]">
      MCP servers extend Claude's capabilities with external tools, APIs, and data sources.
    </p>
  </div>
  <Button variant="outline" size="sm" onClick={openAddModal} className="mt-1">
    <Plus className="size-3.5" />
    Add your first server
  </Button>
</div>
```

**In `agents/page.tsx`:** Find the empty/error state. Add contextual guidance:

```tsx
// If agents list is empty or loading fails:
<div className="flex flex-col items-center justify-center gap-3 py-12 text-center animate-fade-in">
  <Bot className="size-8 text-[var(--text-dim)]" />
  <div>
    <p className="text-sm font-medium text-[var(--text-secondary)]">No agents found</p>
    <p className="text-xs text-[var(--text-muted)] mt-1">
      Run <code className="rounded border border-[var(--border-subtle)] px-1 text-[var(--axon-primary)]">claude agents</code> in your terminal to verify the CLI is configured.
    </p>
  </div>
</div>
```

Add imports as needed: `import { Network, Bot, Plus } from 'lucide-react'`

---

#### Section E: Asymmetric settings layout improvements (L3)

**In `settings/page.tsx`:**

Find the main layout flex container. Make targeted improvements without a full rebuild:

1. **Sidebar accent line:** Add a right border gradient to the nav sidebar:
```tsx
<nav className="sticky hidden h-[calc(100vh-3.25rem)] w-56 shrink-0 overflow-y-auto border-r border-r-[var(--border-subtle)] py-6 pr-4 lg:flex lg:flex-col">
```

2. **Section divider:** Find `SectionDivider` component and replace the plain line with a gradient fade:
```tsx
function SectionDivider() {
  return (
    <div className="my-8 h-px bg-gradient-to-r from-transparent via-[var(--border-subtle)] to-transparent" />
  )
}
```

3. **Content max-width:** Widen slightly for breathing room:
```tsx
// OLD: max-w-[720px]
// NEW: max-w-[780px]
```

4. **Section headers:** Add a subtle left accent bar:
```tsx
// Wrap SectionHeader in a left-bordered div:
<div className="border-l-2 border-l-[var(--border-accent)] pl-3">
  <SectionHeader ... />
</div>
```

---

**Final step — Pages Agent: verify and commit**

```bash
cd /home/jmagar/workspace/axon_rust/apps/web
pnpm run build 2>&1 | grep -E "error|Error" | head -20
```

```bash
git add apps/web/app/mcp/ apps/web/app/settings/ apps/web/app/agents/
git commit -m "feat(pages): modal delete dialogs, MCP single save, settings typography, empty states, layout improvements"
```

---

### Task W2-F: Agent 7 — Neural Canvas

**Addresses:** L4, L10

**Files (exclusive ownership):**
- Modify: `apps/web/components/neural-canvas.tsx`
- Modify: `apps/web/lib/pulse/neural-canvas-presets.ts` (locate exact path first)

**Context:**
The neural canvas is a bioluminescent particle system that renders in the background. It has profiles (current, subtle, cinematic, electric) stored in `neural-canvas-presets.ts`. The `NeuralCanvasProps` accepts a `profile` prop. This agent adds a `zen` profile for calm, low-activity pages and makes the profile selector in the UI more discoverable.

**Read both files in full before making changes.** The presets file path may be at `apps/web/lib/pulse/neural-canvas-presets.ts` — verify with `find apps/web -name "neural-canvas-presets.ts"`.

---

#### Section A: Add `zen` profile (L4)

**In `neural-canvas-presets.ts`:**

Examine the existing profile structure. Each profile is a `VisualPresetConfig` with `palette`, `brightness`, `glow`, `particleCount`, `speed`, and similar fields. Add a new `zen` entry modeled after `subtle` but with lower intensity:

```typescript
// After the 'subtle' profile, add:
zen: {
  palette: {
    core: { r: 180, g: 210, b: 255 },   // muted blue-white
    bright: { r: 30, g: 100, b: 180 },
    mid: { r: 10, g: 55, b: 130 },
    dim: { r: 5, g: 28, b: 80 },
    faint: { r: 2, g: 12, b: 40 },
  },
  brightness: 0.3,         // half of subtle's value
  glow: 0.2,               // minimal glow
  particleCount: 20,        // quarter of default
  connectionOpacity: 0.06,
  speed: 0.3,              // very slow movement
  pulseFrequency: 0.002,   // rare pulses
  description: 'Minimal bioluminescent activity for focused work',
} satisfies VisualPresetConfig,
```

Adjust the numeric values to match the exact shape of `VisualPresetConfig` — read the type definition carefully. The goal is: barely-there animation, calming, low CPU.

Update the `NeuralCanvasProfile` union type to include `'zen'`:
```typescript
export type NeuralCanvasProfile = 'current' | 'subtle' | 'cinematic' | 'electric' | 'zen'
```

---

#### Section B: Profile switcher persistence (L10)

**In `neural-canvas.tsx`:**

Find where `DEFAULT_NEURAL_CANVAS_PROFILE` is used. Currently the profile is passed in via props from a parent. The goal is to make the current profile readable and allow users to change it without diving into menus.

The profile switcher currently exists in the UI (in omnibox or settings). This task ensures the selected profile is persisted to localStorage:

```typescript
// In the canvas component or wherever profile is managed:
export function useNeuralCanvasProfile() {
  const [profile, setProfile] = useState<NeuralCanvasProfile>(() => {
    if (typeof window === 'undefined') return DEFAULT_NEURAL_CANVAS_PROFILE
    return (localStorage.getItem('axon-canvas-profile') as NeuralCanvasProfile)
      ?? DEFAULT_NEURAL_CANVAS_PROFILE
  })

  const changeProfile = useCallback((p: NeuralCanvasProfile) => {
    setProfile(p)
    localStorage.setItem('axon-canvas-profile', p)
  }, [])

  return { profile, changeProfile }
}
```

Export this hook so the settings page and omnibox can consume it.

If the profile is currently managed as a global state or context, follow the existing pattern rather than introducing a new one.

---

**Final step — Neural Canvas Agent: verify and commit**

```bash
cd /home/jmagar/workspace/axon_rust/apps/web
pnpm run build 2>&1 | grep -E "error|Error" | head -20
```

```bash
git add apps/web/components/neural-canvas.tsx apps/web/lib/pulse/neural-canvas-presets.ts
git commit -m "feat(canvas): add zen profile, persist profile selection to localStorage"
```

---

## Final Verification

After all Wave 2 agents have committed, run from the repo root:

```bash
cd /home/jmagar/workspace/axon_rust/apps/web

# Full type check + build
pnpm run build

# Lint
pnpm run lint

# Check for any remaining hardcoded rgba in component files
grep -r "rgba(255, 135, 175" apps/web/components/ --include="*.tsx" | grep -v ".next"
grep -r "rgba(175, 215, 255" apps/web/components/ --include="*.tsx" | grep -v ".next"
grep -r "rgba(10, 18, 35" apps/web/components/ --include="*.tsx" | grep -v ".next"
```

Any remaining hardcoded rgba values found by grep should be replaced with the appropriate CSS token.

```bash
git log --oneline -10
```

Expected: 8 commits (1 Wave 1 + 7 Wave 2: one per agent, plus any fix commits).

---

## Issue Coverage Checklist

| Issue | Agent | File(s) |
|-------|-------|---------|
| H1 — Font pairing | Wave 1 | `layout.tsx`, `globals.css` |
| H2 — Color palette rebuild | Wave 1 | `globals.css` |
| H3 — Motion keyframes | Wave 1 | `globals.css` |
| H3 apply — messages, badges, tables, buttons | A2, A3, A4, A5 | component files |
| H4 — Body atmosphere | Wave 1 | `globals.css` |
| H5 — Shadow tokens | Wave 1 | `globals.css` |
| H5 apply — cards, bubbles, tooltips | A2, A3 | component files |
| H6 — WCAG contrast (text-dim, scrollbar) | Wave 1 + A4 | `globals.css`, `scroll-area.tsx` |
| H7 — Keyboard focus rings | A2, A3, A4, A5 | component files |
| H8 — Virtual scrolling | A3 | `table-renderer.tsx`, `results-panel.tsx` |
| M1 — Empty states | A2, A3, A6 | pulse, raw, mcp, agents |
| M2 — Message bubble asymmetry | A2 | `message-content.tsx` |
| M3 — Modal delete dialogs | A6 | `mcp/page.tsx`, `settings/page.tsx` |
| M4 — Status bar persistence | A5 | `omnibox.tsx` |
| M5 — Tool badge pin indicator | A2 | `tool-badge.tsx` |
| M6 — Source list animation | A2 | `pulse-chat-pane.tsx` |
| M7 — Thinking block improvements | A2 | `message-content.tsx` |
| M8 — Doctor report failure grouping | A3 | `doctor-report.tsx` |
| M9 — Loading indicators | A2, A3 | pulse, raw-renderer |
| M10 — Doctor report asymmetric grid | A3 | `doctor-report.tsx` |
| M11 — @mention discoverability | A5 | `omnibox.tsx` |
| M12 — Settings section typography | A6 | `settings/page.tsx` |
| M13 — Mobile pane switcher labels | A2 | `pulse-mobile-pane-switcher.tsx` |
| M14 — MCP form single save | A6 | `mcp/components.tsx` |
| M15 — Split pane divider | A2 | `pulse-workspace.tsx` |
| L1 — Copy success animation | A2, A3 | `message-content.tsx`, `content-viewer.tsx` |
| L2 — Suggestion stagger | A5 | `omnibox.tsx` |
| L3 — Settings layout improvements | A6 | `settings/page.tsx` |
| L4 — NeuralCanvas zen profile | A7 | `neural-canvas-presets.ts` |
| L5 — Unsaved title indicator | A2 | `pulse-toolbar.tsx` |
| L6 — Timestamp legibility | A2 | `message-content.tsx` |
| L7 — Table pagination toggle | A3 | `table-renderer.tsx` |
| L8 — MCP status spinner fix | A6 | `mcp/components.tsx` |
| L9 — CSS token audit | A4, A5, A6 | UI components, omnibox, pages |
| L10 — Canvas profile persistence | A7 | `neural-canvas.tsx` |

**Total: 33 issues, 7 agents, 0 file conflicts.**
