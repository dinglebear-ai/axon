# Palette Crystalline Beautification — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Apply the Crystalline visual design to `apps/palette-tauri` — darker near-black surfaces, cyan accent system replacing rose, ghost-chip mode pill with × dismiss.

**Architecture:** Pure CSS token overrides in `aurora.css`, targeted rule replacements in `styles.css`, and a one-line JSX change in `App.tsx`. No layout changes, no new components, no logic changes.

**Tech Stack:** CSS custom properties, React 19 / TSX, Vite, Tailwind v4, pnpm

---

## File Map

| File | What changes |
|---|---|
| `apps/palette-tauri/src/components/aurora.css` | Override 4 surface tokens (page-bg, panel-medium, control-surface, shell-bg) to the darker crystalline palette |
| `apps/palette-tauri/src/styles.css` | brand dot → cyan; submit button → cyan gradient; command bar bg fix; mode pill → ghost chip; active row → inset cyan border; panel heading → ultra-muted; footer bg + text; output-body bg + border |
| `apps/palette-tauri/src/App.tsx` | Add `<span className="mode-pill-dismiss">×</span>` inside the mode pill button |

---

### Task 1: Darken surface tokens in aurora.css

**Files:**
- Modify: `apps/palette-tauri/src/components/aurora.css:1-11` (`:root, .dark` surface block)
- Modify: `apps/palette-tauri/src/components/aurora.css:134-137` (shell-bg block)

- [ ] **Step 1: Replace the four surface tokens**

  In `apps/palette-tauri/src/components/aurora.css`, find the `:root, .dark {` block at line 1. Replace these four lines:

  ```css
  /* BEFORE */
  --aurora-page-bg:       #07131c;
  --aurora-nav-bg:        #07111a;
  --aurora-panel-medium:  #102330;
  --aurora-control-surface: #0c1a24;
  ```

  With:

  ```css
  /* AFTER */
  --aurora-page-bg:       #070f18;
  --aurora-nav-bg:        #060e17;
  --aurora-panel-medium:  #0f1d2b;
  --aurora-control-surface: #071018;
  ```

- [ ] **Step 2: Flatten the shell-bg aurora wash**

  Find the `--aurora-shell-bg` block around line 134:

  ```css
  /* BEFORE */
  --aurora-shell-bg: radial-gradient(circle at 12% 0%, rgba(41, 182, 246, 0.09), transparent 28%),
                     radial-gradient(circle at 88% 0%, rgba(28, 127, 172, 0.10), transparent 24%),
                     var(--aurora-page-bg);
  ```

  Replace with a flat value (the crystalline direction drops the radial aurora wash — `#0b1622` is slightly lighter than the output pane's `#070f18`, giving a subtle depth layer):

  ```css
  --aurora-shell-bg: #0b1622;
  ```

- [ ] **Step 3: Typecheck**

  ```bash
  cd apps/palette-tauri && pnpm typecheck
  ```

  Expected: no errors (CSS-only change, TS is unaffected).

- [ ] **Step 4: Commit**

  ```bash
  git add apps/palette-tauri/src/components/aurora.css
  git commit -m "style(palette): crystalline surface tokens — darker near-black base"
  ```

---

### Task 2: Brand dot + submit button → cyan

**Files:**
- Modify: `apps/palette-tauri/src/styles.css:77-83` (brand-dot)
- Modify: `apps/palette-tauri/src/styles.css:85-118` (command-submit rules)

- [ ] **Step 1: Swap brand dot from rose to cyan**

  Find `.brand-dot` at line 77:

  ```css
  /* BEFORE */
  .brand-dot {
    width: 9px;
    height: 9px;
    border-radius: 999px;
    background: var(--aurora-accent-pink);
    box-shadow: 0 0 12px color-mix(in srgb, var(--aurora-accent-pink) 42%, transparent);
  }
  ```

  Replace with:

  ```css
  .brand-dot {
    width: 9px;
    height: 9px;
    border-radius: 999px;
    background: #29b6f6;
    box-shadow: 0 0 8px rgba(41, 182, 246, 0.55);
  }
  ```

- [ ] **Step 2: Replace submit button gradient and border**

  Find `.command-submit` at line 108:

  ```css
  /* BEFORE */
  .command-submit {
    color: var(--aurora-accent-foreground);
    background: var(--aurora-rose-gradient);
    border-color: color-mix(in srgb, var(--aurora-accent-pink) 42%, transparent);
  }

  .command-submit:disabled {
    color: var(--aurora-disabled-text);
    background: var(--aurora-disabled-surface);
    cursor: default;
  }
  ```

  Replace with:

  ```css
  .command-submit {
    color: #fff;
    background: linear-gradient(135deg, #29b6f6, #1565c0);
    border-color: rgba(41, 182, 246, 0.4);
    box-shadow: 0 2px 10px rgba(41, 182, 246, 0.35);
  }

  .command-submit:disabled {
    color: #1a3550;
    background: #0d1e2d;
    border-color: rgba(255, 255, 255, 0.07);
    box-shadow: none;
    cursor: default;
  }
  ```

  Also update the shared hover rule to not override the submit button's background. Find:

  ```css
  .titlebar-button:hover,
  .command-submit:hover:not(:disabled),
  .output-tools button:hover {
    color: var(--aurora-text-primary);
    background: var(--aurora-hover-bg);
    border-color: var(--aurora-border-default);
  }
  ```

  Replace with:

  ```css
  .titlebar-button:hover,
  .output-tools button:hover {
    color: var(--aurora-text-primary);
    background: var(--aurora-hover-bg);
    border-color: var(--aurora-border-default);
  }

  .command-submit:hover:not(:disabled) {
    background: linear-gradient(135deg, #42c8ff, #1976d2);
    box-shadow: 0 3px 14px rgba(41, 182, 246, 0.45);
  }
  ```

- [ ] **Step 3: Typecheck**

  ```bash
  cd apps/palette-tauri && pnpm typecheck
  ```

  Expected: no errors.

- [ ] **Step 4: Commit**

  ```bash
  git add apps/palette-tauri/src/styles.css
  git commit -m "style(palette): brand dot + submit button → cyan gradient"
  ```

---

### Task 3: Mode pill → ghost chip CSS

**Files:**
- Modify: `apps/palette-tauri/src/styles.css:246-263` (command-mode-pill block)

- [ ] **Step 1: Replace mode pill with ghost chip styles**

  Find `.command-mode-pill` at line 246:

  ```css
  /* BEFORE */
  .command-mode-pill {
    display: inline-flex;
    align-items: center;
    height: 30px;
    padding: 0 10px;
    color: var(--aurora-accent-foreground);
    background: color-mix(in srgb, var(--aurora-accent-primary) 18%, var(--aurora-control-surface));
    border: 1px solid color-mix(in srgb, var(--aurora-accent-primary) 45%, var(--aurora-border-default));
    border-radius: 999px;
    font-family: var(--aurora-font-mono);
    font-size: var(--aurora-type-caption);
    font-weight: var(--aurora-weight-label);
    cursor: pointer;
  }

  .command-mode-pill:hover {
    background: color-mix(in srgb, var(--aurora-accent-primary) 26%, var(--aurora-control-surface));
  }
  ```

  Replace with:

  ```css
  .command-mode-pill {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 27px;
    padding: 0 7px 0 10px;
    color: #29b6f6;
    background: transparent;
    border: 1px solid rgba(41, 182, 246, 0.28);
    border-radius: 6px;
    font-family: var(--aurora-font-mono);
    font-size: var(--aurora-type-caption);
    font-weight: var(--aurora-weight-label);
    cursor: pointer;
    white-space: nowrap;
    flex-shrink: 0;
  }

  .command-mode-pill:hover {
    background: rgba(41, 182, 246, 0.07);
    border-color: rgba(41, 182, 246, 0.42);
  }

  .mode-pill-dismiss {
    width: 15px;
    height: 15px;
    border-radius: 3px;
    background: rgba(41, 182, 246, 0.1);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-size: 12px;
    line-height: 1;
    font-style: normal;
  }
  ```

- [ ] **Step 2: Typecheck**

  ```bash
  cd apps/palette-tauri && pnpm typecheck
  ```

  Expected: no errors.

- [ ] **Step 3: Commit**

  ```bash
  git add apps/palette-tauri/src/styles.css
  git commit -m "style(palette): mode pill → ghost chip with × dismiss slot"
  ```

---

### Task 4: Add × dismiss span in App.tsx

**Files:**
- Modify: `apps/palette-tauri/src/App.tsx:370-372`

- [ ] **Step 1: Add the dismiss span inside the mode pill button**

  Find the mode pill button at line 370:

  ```tsx
  /* BEFORE */
  <button className="command-mode-pill" type="button" onClick={() => setModeAction(null)} title="Clear action mode">
    {modeAction.subcommand}
  </button>
  ```

  Replace with:

  ```tsx
  <button className="command-mode-pill" type="button" onClick={() => setModeAction(null)} title="Clear action mode">
    {modeAction.subcommand}
    <span className="mode-pill-dismiss" aria-hidden="true">×</span>
  </button>
  ```

- [ ] **Step 2: Typecheck**

  ```bash
  cd apps/palette-tauri && pnpm typecheck
  ```

  Expected: no errors.

- [ ] **Step 3: Commit**

  ```bash
  git add apps/palette-tauri/src/App.tsx
  git commit -m "style(palette): add × dismiss span to mode pill ghost chip"
  ```

---

### Task 5: Active action row → inset cyan border

**Files:**
- Modify: `apps/palette-tauri/src/styles.css:376-381` (.action-row hover/selected)

- [ ] **Step 1: Replace active row treatment**

  Find `.action-row:hover, .action-row-selected` at line 376:

  ```css
  /* BEFORE */
  .action-row:hover,
  .action-row-selected {
    background: color-mix(in srgb, var(--aurora-accent-primary) 9%, var(--aurora-control-surface));
    border-color: color-mix(in srgb, var(--aurora-accent-primary) 38%, var(--aurora-border-default));
    box-shadow: 0 1px 5px rgba(0, 0, 0, 0.12);
  }
  ```

  Replace with:

  ```css
  .action-row:hover,
  .action-row-selected {
    background: #0d1f30;
    border-color: rgba(41, 182, 246, 0.22);
    box-shadow: inset 3px 0 0 #29b6f6, 0 1px 5px rgba(0, 0, 0, 0.28);
  }
  ```

- [ ] **Step 2: Typecheck**

  ```bash
  cd apps/palette-tauri && pnpm typecheck
  ```

  Expected: no errors.

- [ ] **Step 3: Commit**

  ```bash
  git add apps/palette-tauri/src/styles.css
  git commit -m "style(palette): active action row → 3px inset cyan border"
  ```

---

### Task 6: Panel heading, footer, output body

**Files:**
- Modify: `apps/palette-tauri/src/styles.css:309-317` (.panel-heading)
- Modify: `apps/palette-tauri/src/styles.css:453-463` (.output-body)
- Modify: `apps/palette-tauri/src/styles.css:548-554` (.palette-footer)

- [ ] **Step 1: Ultra-mute the panel heading**

  Find `.panel-heading` at line 309:

  ```css
  /* BEFORE */
  .panel-heading {
    justify-content: space-between;
    margin-bottom: 8px;
    color: var(--aurora-text-muted);
    font-size: var(--aurora-type-caption);
    font-weight: var(--aurora-weight-label);
    text-transform: uppercase;
    letter-spacing: var(--aurora-letter-eyebrow);
  }
  ```

  Replace with:

  ```css
  .panel-heading {
    justify-content: space-between;
    margin-bottom: 8px;
    color: #1e3448;
    font-size: var(--aurora-type-caption);
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: var(--aurora-letter-eyebrow);
  }
  ```

- [ ] **Step 2: Darken the output body**

  Find `.output-body` at line 453:

  ```css
  /* BEFORE */
  .output-body {
    flex: 1;
    min-height: 0;
    margin: 0;
    overflow: auto;
    padding: 10px 12px;
    color: var(--aurora-text-primary);
    background: var(--aurora-control-surface);
    border: 1px solid var(--aurora-border-default);
    border-radius: 8px;
  }
  ```

  Replace with:

  ```css
  .output-body {
    flex: 1;
    min-height: 0;
    margin: 0;
    overflow: auto;
    padding: 10px 12px;
    color: var(--aurora-text-primary);
    background: #040b12;
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: 8px;
  }
  ```

- [ ] **Step 3: Restyle the footer**

  Find `.palette-footer` at line 548:

  ```css
  /* BEFORE */
  .palette-footer {
    justify-content: space-between;
    height: 34px;
    padding: 0 18px;
    background: var(--aurora-nav-bg);
    border-top: 1px solid var(--aurora-border-default);
  }
  ```

  Replace with:

  ```css
  .palette-footer {
    justify-content: space-between;
    height: 34px;
    padding: 0 18px;
    background: #060e17;
    border-top: 1px solid rgba(255, 255, 255, 0.05);
    color: #1e3448;
  }
  ```

- [ ] **Step 4: Typecheck**

  ```bash
  cd apps/palette-tauri && pnpm typecheck
  ```

  Expected: no errors.

- [ ] **Step 5: Commit**

  ```bash
  git add apps/palette-tauri/src/styles.css
  git commit -m "style(palette): panel heading + output body + footer → crystalline"
  ```

---

### Task 7: Build verification + version bump

**Files:**
- Modify: `apps/palette-tauri/package.json` (version bump)
- Modify: `Cargo.toml` (workspace version bump)
- Modify: `CHANGELOG.md` (new entry)

- [ ] **Step 1: Vite build check**

  ```bash
  cd apps/palette-tauri && pnpm vite:build
  ```

  Expected: build completes with no errors. CSS is bundled into `dist/`.

- [ ] **Step 2: Bump version in apps/palette-tauri/package.json**

  In `apps/palette-tauri/package.json`, increment the patch version:

  ```json
  /* BEFORE */
  "version": "4.12.2"

  /* AFTER */
  "version": "4.12.3"
  ```

- [ ] **Step 3: Bump version in Cargo.toml**

  In `Cargo.toml` at the repo root (line 21), increment the workspace version:

  ```toml
  # BEFORE
  version = "4.12.2"

  # AFTER
  version = "4.12.3"
  ```

- [ ] **Step 4: Add CHANGELOG entry**

  In `CHANGELOG.md`, insert immediately after the `## [Unreleased]` line (before `## [4.12.2]`):

  ```markdown
  ## [4.12.3] - 2026-05-28

  ### Changed
  - style(palette): Crystalline visual design — darker near-black surfaces, cyan accent system replacing rose, ghost chip mode pill with × dismiss
  ```

- [ ] **Step 5: Final commit**

  ```bash
  git add apps/palette-tauri/package.json Cargo.toml CHANGELOG.md
  git commit -m "chore(palette): v4.12.3 — Crystalline design complete"
  ```

- [ ] **Step 6: Push**

  ```bash
  git push
  ```
