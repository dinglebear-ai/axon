# Axon Shell — UI Design Language

Last updated: 2026-03-09

Canonical source of truth for the Axon Shell's visual design language, component patterns, and layout architecture. The Axon Shell is a three-pane IDE-like conversation workspace built on the Axon design system (`docs/UI-DESIGN-SYSTEM.md`). This document extends that foundation with shell-specific conventions.

> **Relationship to `UI-DESIGN-SYSTEM.md`:** That document defines the global token system (colors, typography, surfaces, borders, shadows, motion). This document defines how the Axon Shell *applies* those tokens, plus shell-specific patterns not covered by the global system.

---

## Table of Contents

1. [Architecture](#1-architecture)
2. [Visual Identity](#2-visual-identity)
3. [Layout System](#3-layout-system)
4. [Glass Surface System](#4-glass-surface-system)
5. [Message Bubbles](#5-message-bubbles)
6. [Sidebar Rail](#6-sidebar-rail)
7. [Prompt Composer](#7-prompt-composer)
8. [Terminal Drawer](#8-terminal-drawer)
9. [Pane Handles](#9-pane-handles)
10. [Button Sizing](#10-button-sizing)
11. [Session List](#11-session-list)
12. [Chain of Thought](#12-chain-of-thought)
13. [Typing Indicator](#13-typing-indicator)
14. [File Attachment Pills](#14-file-attachment-pills)
15. [Confirmation Dialogs](#15-confirmation-dialogs)
16. [Responsive Breakpoints](#16-responsive-breakpoints)
17. [Animation Catalog](#17-animation-catalog)
18. [Accessibility](#18-accessibility)
19. [Anti-Patterns](#19-anti-patterns)

---

## 1. Architecture

### Component Tree

```
AxonFrame
  ├── NeuralCanvas (profile="current")
  ├── Atmospheric gradient overlays (2 layers)
  └── AxonShell (orchestrator — ~408 lines)
        ├── [Mobile] Header bar + PulseMobilePaneSwitcher
        ├── [Mobile] AxonSidebar (variant="mobile")
        ├── [Mobile] Chat pane (Conversation + AxonMessageList + AxonPromptComposer)
        ├── [Mobile] Editor pane (PulseEditorPane, dynamic import, ssr: false)
        ├── [Mobile] Terminal drawer (absolute positioned)
        ├── [Desktop] CSS Grid (3 columns, animated transitions)
        │     ├── AxonSidebar (variant="desktop") | AxonPaneHandle
        │     ├── Chat pane | AxonPaneHandle
        │     └── Editor pane | AxonPaneHandle
        └── [Desktop] Terminal strip (border-top, below grid)
```

### File Map

| File | Lines | Responsibility |
|------|-------|---------------|
| `axon-shell.tsx` | ~408 | Orchestrator — state, layout, event wiring |
| `axon-sidebar.tsx` | ~484 | Unified sidebar (mobile + desktop via `variant` prop) |
| `axon-message-list.tsx` | ~425 | `React.memo` message list + bubble constants |
| `axon-prompt-composer.tsx` | ~491 | Composer + model/permission/tools dropdowns |
| `axon-terminal-pane.tsx` | ~96 | Terminal emulator wrapper |
| `axon-pane-handle.tsx` | ~36 | Collapsed pane handle |
| `axon-frame.tsx` | ~25 | Background frame (NeuralCanvas + gradients) |
| `axon-ui-config.ts` | ~24 | Types + UI configuration constants |

---

## 2. Visual Identity

The Axon Shell sits on top of the Axon design system but adds its own atmosphere:

- **Background**: NeuralCanvas (bioluminescent animated particles) at z-0
- **Gradient overlay 1**: `radial-gradient(circle_at_top, rgba(135,175,255,0.16), transparent 26%), radial-gradient(circle_at_80%_15%, rgba(255,135,175,0.12), transparent 20%), linear-gradient(180deg, rgba(3,8,23,0.2), rgba(3,8,23,0.55))`
- **Grid overlay**: `44px × 44px` grid lines at `rgba(135,175,255,0.03)`, 25% opacity
- **Content layer**: `z-[1]` above all backgrounds

### Color Roles (Shell-Specific)

| Role | Token | Usage |
|------|-------|-------|
| User message accent | `--axon-primary` / `--axon-primary-strong` | User bubble gradient, "You" label, role dot |
| Agent message accent | `--axon-secondary` / `--axon-secondary-strong` | Agent bubble gradient, agent label, role dot, typing dots |
| Unread indicator | `--axon-primary` | Session list unread dot |
| Active pane button | `--axon-primary` bg, `--axon-bg` text | Mobile header buttons, sidebar rail active |
| MCP status: online | `rgba(64,196,128,0.12)` bg, `rgba(128,220,160,0.92)` text | Tools dropdown |
| MCP status: offline | `rgba(255,135,175,0.12)` bg, `rgba(255,170,196,0.86)` text | Tools dropdown |

---

## 3. Layout System

### Desktop (≥ `lg` / 1024px)

CSS Grid with animated column transitions:

```tsx
gridTemplateColumns: [
  sidebarOpen ? 'minmax(264px, 264px)' : '40px',   // sidebar or handle
  chatOpen    ? 'minmax(0, 1fr)'       : '40px',   // chat or handle
  editorOpen  ? 'minmax(360px, 1fr)'   : '40px',   // editor or handle
].join(' ')
```

- **Transition**: `transition-[grid-template-columns] duration-300 ease-out`
- **Terminal**: Full-width strip below the grid (not a grid column)
- **Pane handles**: 40px collapsed state with vertical label + chevron

### Mobile (< `lg`)

Single-pane switcher — only one pane visible at a time:

```
Header (h-14) → [sidebar | chat | editor]
Terminal: absolute bottom overlay (42dvh + 0.75rem padding)
```

Mobile pane selection persisted to `localStorage` key `axon.web.reboot.mobile-pane`.

---

## 4. Glass Surface System

Shell-specific glass tokens (defined in `globals.css`):

```css
--glass-panel:    rgba(6, 12, 26, 0.82)   /* Sidebar background */
--glass-chat:     rgba(8, 14, 28, 0.55)   /* Chat pane background */
--glass-overlay:  rgba(7, 12, 26, 0.96)   /* Dropdown menus, modal overlays */
--glass-editor:   rgba(6, 12, 26, 0.84)   /* Editor pane background */
--glass-terminal: rgba(6, 12, 26, 0.55)   /* Mobile terminal drawer */
--surface-input:        rgba(10, 18, 35, 0.32)  /* Input fields, inactive buttons */
--surface-input-strong: rgba(10, 18, 35, 0.45)  /* Desktop action buttons */
```

### Usage Rules

| Surface | Where | Classes |
|---------|-------|---------|
| `--glass-panel` | Sidebar, pane handles | `bg-[var(--glass-panel)]` |
| `--glass-chat` | Chat pane | `bg-[var(--glass-chat)] backdrop-blur-sm` |
| `--glass-editor` | Editor pane | `bg-[var(--glass-editor)]` |
| `--glass-terminal` | Mobile terminal drawer | `bg-[var(--glass-terminal)] backdrop-blur-xl` |
| `--glass-overlay` | All dropdown menus | `bg-[var(--glass-overlay)] backdrop-blur-xl` |
| `--surface-input` | Inactive toggle buttons | `bg-[var(--surface-input)]` |
| `--surface-input-strong` | Desktop "Terminal"/"Restore" buttons | `bg-[var(--surface-input-strong)]` |

---

## 5. Message Bubbles

### User Bubble

```tsx
const AXON_USER_BUBBLE_CLASS =
  'rounded-xl border border-[var(--border-standard)] ' +
  'bg-[linear-gradient(140deg,rgba(135,175,255,0.28),rgba(135,175,255,0.12))] ' +
  'px-4 py-3 shadow-[var(--shadow-md)] text-[var(--text-primary)] text-sm'
```

- Blue-tinted gradient (140°, 28% → 12% opacity)
- `--border-standard` border
- Role label: `text-[var(--axon-primary)]`, dot `bg-[var(--axon-primary-strong)]`

### Agent Bubble

```tsx
const AXON_ASSISTANT_BUBBLE_CLASS =
  'rounded-xl border border-[rgba(255,135,175,0.18)] ' +
  'bg-[linear-gradient(140deg,rgba(255,135,175,0.1),rgba(10,18,35,0.55))] ' +
  'px-4 py-3 shadow-[0_6px_18px_rgba(3,7,18,0.3)] text-[var(--text-secondary)] text-sm'
```

- Pink-tinted gradient (140°, 10% → dark)
- Pink border at 18% opacity
- Role label: `text-[var(--axon-secondary-strong)]`, dot `bg-[var(--axon-secondary)]`

### Responsive Sizing

| Property | Mobile | Desktop |
|----------|--------|---------|
| User max-width | `max-w-[92%]` | `max-w-[80%]` |
| Agent max-width | `max-w-[96%]` | `max-w-[88%]` |
| Bubble rounding | `rounded-[18px]` | `rounded-[22px]` |
| Bot icon (empty state) | `size-8` | `size-10` |
| File truncation | `max-w-[140px]` | `max-w-[180px]` |

### Message Actions

Hover-revealed action bar below each bubble:

```tsx
className="[@media(hover:hover)]:opacity-0 [@media(hover:hover)]:group-hover:opacity-100"
```

- Touch devices: always visible
- Desktop: hidden until hover, with `translate-y` micro-animation
- Actions: Copy (with check bounce animation), Edit (user) / Retry (agent)

### Stagger Animation

Messages enter with `animate-fade-in-up` and staggered delay:

```tsx
style={{ animationDelay: `${index * 50}ms`, animationFillMode: 'both' }}
```

---

## 6. Sidebar Rail

Unified component: `AxonSidebar` with `variant: 'mobile' | 'desktop'`.

### Variant Sizing

| Property | Desktop | Mobile |
|----------|---------|--------|
| Toolbar height | `h-10` | `h-11` |
| Title row height | `h-11` | `h-12` |
| Search input | `h-7 text-xs` | `h-9 text-[13px]` |
| Mode switcher button | `h-7` | `h-8` |
| "New item" button | `size-7` | `size-9` |

### Mode Switcher

Dropdown button in toolbar area — switches between 4 modes:

| Mode | Icon | Content |
|------|------|---------|
| `sessions` | `MessageSquareText` | Flat session list with unread indicators |
| `files` | `FolderOpen` | FileTree component with count label |
| `assistant` | `Bot` | Assistant-scoped view powered by the unified sessions endpoint (`/api/sessions/list?assistant_mode=true`) across Claude/Codex/Gemini stores |

Assistant mode sends `assistant_mode: true` in the `pulse_chat` WS flags. The backend resolves
assistant turns to `$AXON_DATA_DIR/axon/assistant` (fallback:
`~/.local/share/axon/axon/assistant`) and scopes ACP connection reuse by agent+mode.

Assistant session title/preview hygiene:
- Sidebars prefer substantive user prompts (adaptive first-vs-latest heuristic).
- `[System context ...][User message]` wrappers are stripped from both chat display and sidebar previews.

### Rail Item Active State

```tsx
function railItemClass(isActive: boolean) {
  return isActive
    ? 'border-[var(--axon-primary)] bg-[var(--surface-primary)] text-[var(--text-primary)]'
    : 'border-transparent text-[var(--text-secondary)] hover:border-[rgba(175,215,255,0.18)] hover:bg-[rgba(175,215,255,0.03)] hover:text-[var(--text-primary)]'
}
```

Active items use a `border-l-2` left accent + `--surface-primary` background.

### Branding Bar

Top-left of sidebar:

```tsx
<Sparkles className="size-3 text-[var(--axon-primary)]" />
<span className="text-[11px] uppercase tracking-[0.22em] text-[var(--text-dim)]">Axon</span>
```

---

## 7. Prompt Composer

### Panel Surface

```tsx
const AXON_COMPOSER_PANEL_CLASS =
  'border-[rgba(175,215,255,0.14)] ' +
  'bg-[linear-gradient(180deg,rgba(10,18,35,0.92),rgba(5,10,22,0.98))] ' +
  'shadow-[0_14px_40px_rgba(0,0,0,0.34)] backdrop-blur-xl'
```

Rounded container: `rounded-[18px]`

### Textarea

```tsx
className="rounded-[14px] border border-[rgba(175,215,255,0.08)] bg-[rgba(3,7,18,0.38)] px-3 py-2.5 leading-6"
```

- Compact mode: `min-h-16 max-h-56`
- Full mode: `min-h-20 max-h-72`

### Dropdown Trigger Pattern

Shared `ComposerDropdownTrigger` component:

```tsx
className="inline-flex h-8 items-center gap-2 rounded border border-[rgba(175,215,255,0.14)] bg-[rgba(255,255,255,0.04)] px-2.5 text-xs text-[var(--text-secondary)] transition-colors hover:border-[rgba(175,215,255,0.22)] hover:text-[var(--text-primary)]"
```

All dropdown content panels use:

```tsx
className="border-[var(--border-subtle)] bg-[var(--glass-overlay)] text-[var(--text-primary)] backdrop-blur-xl"
```

### Dropdown Labels

Section labels in dropdowns:

```tsx
className="px-2 py-1 text-[11px] uppercase tracking-[0.14em] text-[var(--text-dim)]"
```

---

## 8. Terminal Drawer

### Mobile

Absolute-positioned overlay at bottom of screen:

```tsx
className="pointer-events-none absolute inset-x-0 bottom-0 z-30 px-3 pb-3 animate-slide-up"
```

Inner container:

```tsx
className="rounded-[18px] border border-[var(--border-subtle)] bg-[var(--glass-terminal)] shadow-[0_-18px_48px_rgba(0,0,0,0.42)] backdrop-blur-xl"
```

- Height: `h-[42dvh] min-h-[260px]`
- Header: `h-10`, terminal icon + "Terminal" label, close button
- Chat pane gets bottom padding: `pb-[calc(42dvh+0.75rem)]` when terminal is open

### Desktop

Full-width strip below the grid:

```tsx
className="hidden shrink-0 border-t border-[var(--border-subtle)] animate-slide-up lg:block"
```

### Search Overlay

Positioned inside terminal pane:

```tsx
className="absolute right-3 top-2 z-20 flex items-center gap-1 rounded-md border border-[rgba(175,215,255,0.2)] bg-[rgba(9,18,37,0.95)] px-2 py-1"
```

---

## 9. Pane Handles

Collapsed pane indicator — 40px wide, full height:

```tsx
className="flex h-full min-h-[420px] w-10 flex-col items-center justify-center gap-3 rounded-[14px] border border-[var(--border-subtle)] bg-[var(--glass-panel)] backdrop-blur-md text-[var(--text-dim)] shadow-[var(--shadow-md)] transition-colors hover:text-[var(--axon-primary)]"
```

Label uses vertical writing mode:

```tsx
className="[writing-mode:vertical-rl] rotate-180 text-[10px] uppercase tracking-[0.35em]"
```

---

## 10. Button Sizing

### Mobile Header Buttons

All mobile header buttons (terminal toggle, sidebar, chat, editor) use identical sizing:

```
size-7 (28px × 28px) with size-3.5 icons
```

Pattern:

```tsx
className={`inline-flex size-7 items-center justify-center rounded border transition-colors ${
  isActive
    ? 'border-[rgba(175,215,255,0.25)] bg-[var(--axon-primary)] text-[var(--axon-bg)]'
    : 'border-[var(--border-subtle)] bg-[var(--surface-input)] text-[var(--text-dim)] hover:text-[var(--text-primary)]'
}`}
```

The `PulseMobilePaneSwitcher` uses slightly different styling for its chat/editor buttons:

```tsx
// Chat (active): border-[rgba(175,215,255,0.25)] bg-[var(--axon-primary)] + shadow-[var(--shadow-sm)]
// Editor (active): border-[rgba(255,135,175,0.25)] bg-[var(--axon-secondary)] + shadow-[var(--shadow-sm)]
// Both inactive: bg-[rgba(10,18,35,0.42)] + backdrop-blur-sm
```

### Desktop Header Buttons

Ghost icon buttons via shadcn `Button variant="ghost" size="icon-sm"`:

- Active pane: `text-[var(--axon-primary)]`
- Inactive pane: `text-[var(--text-secondary)]`

Text action buttons (`Terminal`, `Restore`):

```tsx
className="h-8 border border-[var(--border-subtle)] bg-[var(--surface-input-strong)] text-[var(--text-secondary)] hover:bg-[var(--surface-primary)] hover:text-[var(--text-primary)]"
```

---

## 11. Session List

Flat list (no sections) with unread indicator.

### Unread Indicator

Sessions with `hasUnread: true` show:

1. **Dot**: `size-1.5 rounded-full bg-[var(--axon-primary)]` — positioned left of the title with `mt-1.5` alignment
2. **Bold title**: `font-semibold text-[var(--text-primary)]` (read sessions use `font-medium`)

```tsx
{session.hasUnread ? (
  <span
    className="mt-1.5 size-1.5 shrink-0 rounded-full bg-[var(--axon-primary)]"
    aria-label="Unread responses"
  />
) : null}
```

### Session Item Layout

```
┌─ border-l-2 (active: --axon-primary, inactive: transparent) ─────────────┐
│  [●]  Title text (13px, font-medium/semibold)          Last message time  │
│       repo · branch · agent (11px, --text-dim)                            │
└───────────────────────────────────────────────────────────────────────────┘
```

Active session: `aria-current="true"` on the `<button>`.

---

## 12. Chain of Thought

Collapsible reasoning section inside agent messages:

```tsx
className="mt-3 rounded-2xl border border-[rgba(135,175,255,0.12)] bg-[rgba(7,12,26,0.6)] p-3"
```

- Header: "Chain of thought" text, collapsible
- Steps: `ChainOfThoughtStep` components with status indicators (complete/active/pending)
- Reasoning text: `text-xs text-muted-foreground`

---

## 13. Typing Indicator

Three dots with custom `typing-dot` keyframe animation:

```css
@keyframes typing-dot {
  0%, 80%, 100% { opacity: 0.3; transform: scale(0.8); }
  40% { opacity: 1; transform: scale(1.2); }
}
```

- Duration: `0.8s ease-in-out infinite`
- Stagger: 0ms, 200ms, 400ms delay per dot
- Dot: `size-1.5 rounded-full bg-[var(--axon-secondary)]`
- Wrapped in agent bubble styling with agent name label

---

## 14. File Attachment Pills

### In Messages

```tsx
className="inline-flex items-center gap-1.5 rounded border border-[rgba(135,175,255,0.14)] bg-[rgba(255,255,255,0.04)] px-2 py-1 text-xs leading-none text-[var(--text-secondary)]"
```

- Icon: `FileCode2 size-3.5`
- Click: opens file in editor
- `aria-label="Open ${file} in editor"`

### In Composer

```tsx
className="inline-flex items-center gap-1.5 rounded border border-[rgba(175,215,255,0.16)] bg-[rgba(255,255,255,0.04)] px-2 py-1 text-xs leading-none text-[var(--text-secondary)]"
```

- Remove button: `size-4` with `X size-3`
- File icon: `FileCode2 size-3.5 text-[var(--axon-primary)]`

---

## 15. Confirmation Dialogs

Built with custom `Confirmation` context system (not Radix AlertDialog):

```tsx
<ConfirmationContent>
  // role="alertdialog" aria-modal="true"
  // Focus trap with Tab/Shift+Tab cycling
  // Escape handler
  // Click-outside dismiss
</ConfirmationContent>
```

Panel styling:

```tsx
className="absolute right-0 top-full z-20 mt-2 w-72 rounded-2xl border border-[var(--border-subtle)] bg-[var(--glass-overlay)] p-3 shadow-[var(--shadow-lg)]"
```

Action button:

```tsx
className="bg-[var(--axon-primary)] text-[#04111f] hover:bg-[var(--axon-primary-strong)]"
```

---

## 16. Responsive Breakpoints

| Breakpoint | Layout |
|------------|--------|
| `< lg` (< 1024px) | Mobile: single-pane switcher, terminal drawer overlay |
| `≥ lg` (1024px+) | Desktop: 3-column CSS grid, terminal strip |

### Mobile-Specific Patterns

- Header: `h-14`, `bg-[rgba(7,12,26,0.86)]`
- All header buttons: `size-7` (28px)
- Sidebar uses `variant="mobile"` (taller touch targets on inputs)
- Chat and composer get `px-3 py-3` padding
- Session select → auto-switch to chat pane
- File select → auto-switch to editor pane
- Terminal: floating bottom drawer, `42dvh` height

### Desktop-Specific Patterns

- Chat header: `h-14`, `px-4`
- Chat content: `px-4 py-4`
- Composer: full-size (no `compact` prop)
- Sidebar has collapse button → `AxonPaneHandle`
- Minimum pane heights: `min-h-[420px]`

---

## 17. Animation Catalog

Shell-specific animation usage (keyframes defined in `globals.css`):

| Animation | Usage |
|-----------|-------|
| `animate-fade-in` | Sidebar, desktop panes on mount |
| `animate-fade-in-up` | Message bubbles (50ms stagger) |
| `animate-crossfade-in` | Message list session switch |
| `animate-scale-in` | Scroll-to-bottom button |
| `animate-slide-up` | Terminal drawer reveal |
| `animate-typing-dot` | Typing indicator dots (200ms stagger) |
| `animate-check-bounce` | Copy success checkmark |
| `transition-[grid-template-columns]` | Desktop pane resize (300ms ease-out) |
| `transition-colors` | All interactive state changes (default duration) |

---

## 18. Accessibility

### ARIA

| Element | ARIA | Purpose |
|---------|------|---------|
| Terminal toggle | `aria-pressed` | Toggle state |
| Sidebar button | `aria-pressed` | Toggle state |
| Pane switcher | `role="tablist"` + `role="tab"` + `aria-selected` | Tab pattern |
| Active session | `aria-current="true"` | Current selection |
| Page links | `aria-current="page"` | Active navigation |
| File buttons | `aria-label="Open ${file} in editor"` | Accessible name |
| Confirmation | `role="alertdialog"` + `aria-modal="true"` | Dialog pattern |
| Search input | `aria-label="Search ${mode}"` | Accessible name |
| Unread dot | `aria-label="Unread responses"` | Status indicator |
| Collapse button | `<span className="sr-only">` | Screen reader label |

### Focus Management

- Confirmation dialog: focus trap (Tab/Shift+Tab cycling), auto-focus first button, Escape to dismiss
- Agent items: rendered as `<div>` (not `<button>`) since they have no interaction

### Keyboard

- Escape closes confirmation dialog
- Tab cycles within focus trap
- All buttons are type="button"

---

## 19. Anti-Patterns

| Don't | Do Instead |
|-------|-----------|
| Use `min-h-[44px]` on mobile header buttons | Use `size-7` to match `PulseMobilePaneSwitcher` |
| Split sessions into "active" / "recent" sections | Use flat list with `hasUnread` dot indicator |
| Use `<button>` for non-interactive elements | Use `<div>` for display-only items (agents) |
| Use `animate-pulse` for typing indicator | Use custom `animate-typing-dot` (800ms, scale transform) |
| Assume `crypto.randomUUID()` is always available | Use `createClientMessageId()` with secure-context fallback |
| Duplicate mobile/desktop sidebar code | Use single component with `variant` prop |
| Use inline `style` objects for terminal search | Use Tailwind classes |
| Use `--axon-accent-*` or `--axon-text-*` tokens | Use v2 tokens (`--axon-primary`, `--text-primary`, etc.) |
| Create separate glass rgba values | Use `--glass-*` CSS variables |
| Add `role="option"` to `<li>` elements | Use semantic `<button>` inside `<li>` instead |
| Use `--surface-float` on opaque dark panels | Use `--surface-primary` / `--surface-primary-active` |
