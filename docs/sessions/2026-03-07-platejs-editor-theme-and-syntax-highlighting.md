# PlateJS Editor — Theme, Pink Accents, Typography Polish, Syntax Highlighting

**Date:** 2026-03-07
**Branch:** feat/services-layer-refactor
**Working directory:** `apps/web/`

---

## Session Overview

Multi-part editor quality pass covering four areas:
1. **Design token cleanup** — eliminated all remaining generic shadcn tokens (`bg-accent`, `text-muted-foreground`, `bg-border`, etc.) from `toolbar.tsx` and `dropdown-menu.tsx`, replacing with Axon design system tokens
2. **Pink (`--axon-secondary`) integration** — added the brand pink to bold text, italic, H2/H3 headings, links, highlights, and blockquotes in the PlateJS editor
3. **Reboot UI typography** — confirmed Noto Sans + Noto Sans Mono are correctly wired; fixed leaks, added `font-mono` to technical strings, tightened letter-spacing and line-height on key UI elements
4. **Syntax highlighting** — diagnosed and fixed a critical gap: the live editor had no syntax highlighting because `ExtendedNodesKit` registered `CodeBlockPlugin` without `lowlight`/`CodeSyntaxPlugin`

---

## Timeline

| Time | Activity |
|---|---|
| Start | Continued from previous session — picked up remaining generic token audit |
| Phase 1 | Fixed all generic tokens in `toolbar.tsx` (8 locations) and `dropdown-menu.tsx` (9 locations) |
| Phase 2 | Added pink to editor: `strong`, `em`, `h2`, `h3`, `blockquote`, links, highlight marks |
| Phase 3 | Audited Noto Sans propagation across all reboot components; fixed typography leaks |
| Phase 4 | Diagnosed syntax highlighting gap; wired `createLowlight(all)` + `CodeSyntaxPlugin` into runtime kit |
| Phase 5 | Produced complete editor capability audit (what's missing, language support) |
| End | Session save |

---

## Key Findings

### Critical: No syntax highlighting in live editor
- `extended-nodes-kit.tsx` registered `CodeBlockPlugin.withComponent(CodeBlockElement)` — no `options.lowlight`, no `CodeSyntaxPlugin`
- `code-block-base-kit.tsx` (the SSR/static kit) correctly had `createLowlight(all)` but that kit is only used for server-side rendering
- The Axon hljs CSS theme in `globals.css` was fully written but had no tokens to color — it was effectively dead until this fix

### Generic token audit — all cleared
- `toolbar.tsx`: `bg-border`, `hover:bg-accent`, `hover:bg-muted`, `text-foreground`, `text-muted-foreground` (×3), `group-data-[pressed=true]:bg-accent` (×2), `border-input`, `bg-primary`, `text-primary-foreground`
- `dropdown-menu.tsx`: `bg-popover`, `text-popover-foreground`, `focus:bg-accent focus:text-accent-foreground` (×4), `text-muted-foreground` (×2), `bg-border`
- Both now use exclusively Axon CSS variable tokens

### Noto Sans wiring confirmed correct
- `app/layout.tsx:7-17` loads `Noto_Sans` → `--font-noto-sans` and `Noto_Sans_Mono` → `--font-noto-sans-mono`
- `globals.css:12-13` maps `--font-sans: var(--font-noto-sans)` and `--font-mono: var(--font-noto-sans-mono)`
- Tailwind v4 maps `font-sans` class → `--font-sans` and `font-mono` → `--font-mono` — chain is complete
- Only leak found: `text-muted-foreground` in `reboot-message-list.tsx:121` on reasoning text

### Editor language support
- Dropdown UI: ~80 curated languages (defined in `code-block-node.tsx:190-280`)
- Actual highlight engine: 190+ languages via `createLowlight(all)` (now wired)
- Minor dedup bug: both `Sass` and `SCSS` map to `scss`; `Visual Basic` and `VB.Net` both map to `vbnet`

### Editor capability gaps identified
- **Not in runtime at all**: text alignment, font size/color, line height, math/LaTeX, indent, @mentions, date elements, column layouts
- **Plugins loaded but no toolbar button**: highlight mark, superscript, subscript, tables, images, callouts, toggles, TOC
- **UX gaps**: no find/replace, no zen mode, slash command not discoverable from toolbar, source panel read-only

---

## Technical Decisions

### `createLowlight(all)` over selective language import
- `all` imports every lowlight-supported language (~190+), adding ~180kB to the client bundle
- Alternative: import only commonly-used languages to save bundle size
- Decision: use `all` for now because the editor is a power-user tool and the bundle cost is acceptable; can optimize later with a custom language set if needed

### CSS globals for hljs tokens instead of Tailwind arbitrary classes
- Previous approach: 2000-char Tailwind arbitrary class string on `CodeBlockElement`
- New approach: scoped `.axon-editor .hljs-*` rules in `globals.css`
- Rationale: cleaner component code, easier to update theme in one place, no Tailwind purge edge cases with dynamic class names

### Pink on `strong`/`em` vs. leaving as near-white
- Bold was `text-primary` (near-white `#e8f4f8`) — nearly invisible against the dark background
- Pink `--axon-secondary` (#ff87af) creates clear emphasis hierarchy: pink = human emphasis, blue = technical/interactive
- Italic uses `--axon-secondary-strong` (#ff9ec0) at 85% opacity — softer than bold for secondary emphasis

### Blue/Pink semantic split
- Blue (`--axon-primary`): inline code, selection, active toolbar states, H1 — "technical/interactive"
- Pink (`--axon-secondary`): bold, italic, H2, H3, links, highlights, blockquotes — "content emphasis"

---

## Files Modified

| File | Purpose |
|---|---|
| `apps/web/components/ui/toolbar.tsx` | Replaced all generic shadcn tokens with Axon vars; fixed tooltip bg, separator, arrow variants |
| `apps/web/components/ui/dropdown-menu.tsx` | Replaced `bg-popover`, all `focus:bg-accent`, `text-muted-foreground`, `bg-border` with Axon vars; darkened menu bg to `rgba(3,7,18,0.92)` with `backdrop-blur-md` |
| `apps/web/app/globals.css` | Added: H1/H2/H3 editor heading rules, bold→pink, italic→pink, blockquote→pink border, highlight rule; Reboot typography CSS; `body { font-optical-sizing, text-rendering }` |
| `apps/web/components/ui/link-node.tsx` | `text-primary underline decoration-primary` → `text-[var(--axon-secondary)] decoration-[var(--axon-secondary)] opacity-90 hover:opacity-100 transition-opacity` |
| `apps/web/components/ui/link-node-static.tsx` | Same pink treatment for SSR static renderer |
| `apps/web/components/ui/highlight-node.tsx` | `bg-highlight/30 text-inherit` → `bg-[rgba(255,135,175,0.18)] text-[var(--axon-secondary-strong)] rounded-sm px-0.5` |
| `apps/web/components/editor/plugins/extended-nodes-kit.tsx` | **Critical fix**: added `CodeSyntaxPlugin`, `CodeSyntaxLeaf`, `createLowlight(all)` — enables syntax highlighting in live editor |
| `apps/web/components/reboot/reboot-message-list.tsx` | Fixed `text-muted-foreground` → `text-[var(--text-dim)]`; added `font-mono` to file path chips |
| `apps/web/components/reboot/reboot-sidebar.tsx` | Added `rounded-md` + `font-sans` to search input |
| `apps/web/components/reboot/reboot-shell.tsx` | Chat header: `font-medium` → `font-semibold tracking-[-0.01em]`; metadata → `font-mono text-[10px] uppercase tracking-[0.12em]` |

---

## Behavior Changes (Before → After)

### Syntax highlighting
- **Before**: All code blocks rendered as plain uncolored text in the live editor
- **After**: Full lowlight syntax highlighting with the Axon color theme (keywords=pink, strings=green, numbers/attrs=blue, comments=dim italic, builtins=orange, functions=purple)

### Bold/Italic text
- **Before**: Bold was near-white (`#e8f4f8`), italic was unstyled
- **After**: Bold is hot pink (`#ff87af`), italic is soft pink (`#ff9ec0`) at 85% opacity

### Links
- **Before**: `text-primary` blue with blue underline
- **After**: Pink (`--axon-secondary`) with pink underline, 90% opacity, hover→100%

### Highlights (mark)
- **Before**: `bg-highlight/30` (generic yellow-ish)
- **After**: `rgba(255,135,175,0.18)` pink glow with `--axon-secondary-strong` text

### Blockquotes
- **Before**: No custom styling (was referenced in summary but rule was missing from CSS)
- **After**: 3px pink left border (`--axon-secondary`), faint pink glass bg, italic text

### H2/H3 headings in editor
- **Before**: No custom heading rules (inherited global heading styles)
- **After**: H2=pink (`--axon-secondary`), H3=lighter pink at 75% opacity; H1=white/primary large

### Toolbar/Dropdown menus
- **Before**: Generic shadcn accent/muted/border tokens — appeared washed-out or wrong-colored on dark background
- **After**: Axon dark glass surface tokens throughout; dropdown menus now `rgba(3,7,18,0.92)` with `backdrop-blur-md`

### Reboot chat header metadata
- **Before**: `text-xs gap-2` — looked like generic small text
- **After**: `font-mono text-[10px] uppercase tracking-[0.12em]` — reads like a terminal status line

---

## Verification Evidence

| Check | Expected | Actual | Status |
|---|---|---|---|
| `extended-nodes-kit.tsx` imports `CodeSyntaxPlugin` | present | present | ✅ |
| `extended-nodes-kit.tsx` passes `{ lowlight }` to `CodeBlockPlugin.configure` | present | present | ✅ |
| `highlight-node.tsx` uses pink bg | `rgba(255,135,175,0.18)` | `rgba(255,135,175,0.18)` | ✅ |
| `link-node.tsx` uses `--axon-secondary` | present | present | ✅ |
| `globals.css` has `.axon-editor strong { color: var(--axon-secondary) }` | present | present | ✅ |
| `globals.css` has `.axon-editor blockquote` rule | present | present | ✅ |
| `toolbar.tsx` has zero `text-muted-foreground` occurrences | 0 | 0 | ✅ |
| `dropdown-menu.tsx` has zero `focus:bg-accent` occurrences | 0 | 0 | ✅ |
| `reboot-message-list.tsx` has zero `text-muted-foreground` | 0 | 0 | ✅ |
| Search input in sidebar has `rounded-md` | present | present | ✅ |

---

## Source IDs + Collections Touched

*No Axon embed/retrieve operations were performed during this session (no web research required — work was entirely codebase modification).*

---

## Risks and Rollback

### Bundle size increase from `createLowlight(all)`
- **Risk**: Adds ~180kB to the client JS bundle (all lowlight language grammars)
- **Rollback**: Replace `import { all, createLowlight } from 'lowlight'` with a selective import in `extended-nodes-kit.tsx` e.g. `import { createLowlight } from 'lowlight'; import javascript from 'highlight.js/lib/languages/javascript'; const lowlight = createLowlight({ javascript, typescript, ... })`

### Pink on bold/italic is an opinionated change
- **Risk**: May feel too intense for professional document writing use cases
- **Rollback**: In `globals.css`, change `.axon-editor strong { color: var(--axon-secondary) }` back to `color: var(--text-primary)` and remove the `em` rule

### Dropdown menu background change
- **Risk**: `rgba(3,7,18,0.92)` is very dark — content behind could bleed through if backdrop-filter doesn't load
- **Rollback**: Revert `DropdownMenuContent` className in `dropdown-menu.tsx` to `bg-popover text-popover-foreground`

---

## Decisions Not Taken

| Alternative | Rejected Because |
|---|---|
| Selective lowlight language imports | Adds maintenance burden; the full `all` bundle cost is acceptable for a power-user tool |
| Keep bold as near-white | Invisible emphasis — the whole point of bold is to stand out |
| Font color picker for editor | Would need `FontKit` wired into runtime, `ColorPickerPlugin`, and a toolbar UI component — out of scope for this session |
| Make source panel editable (markdown → sync back) | Would need `deserializeMd` round-trip with cursor preservation; complex edge cases with partial invalid markdown |
| Align `--axon-secondary` hue for italic vs bold to be identical | Subtle differentiation (strong vs. strong-strong) helps distinguish em/strong visually |

---

## Open Questions

1. **Bundle budget**: Is 180kB for all lowlight grammars acceptable? Should we profile the initial load time?
2. **Pink intensity**: Is `#ff87af` on bold text too aggressive for long-form writing documents, or is it on-brand?
3. **Source panel write mode**: Should the markdown source panel support edits that sync back to the editor? Currently read-only.
4. **Missing plugins**: Should alignment, font size/color, math, and @mentions be wired into the runtime `CopilotKit`? They're already in `BaseEditorKit` (SSR) — just need the runtime equivalents added.
5. **Slash command discoverability**: The `/` slash command menu is fully wired but nothing in the toolbar surface hints at it. Should there be a discoverable entry point?
6. **Highlight dedup in code-block-node.tsx**: `Sass`→`scss` and `SCSS`→`scss` are both listed; `Visual Basic`→`vbnet` and `VB.Net`→`vbnet` both listed. Should deduplicate.

---

## Next Steps

1. **Wire missing runtime plugins**: Priority order — text alignment (most-requested), highlight toolbar button, superscript/subscript buttons, then font size/color if needed
2. **Fix dropdown dedup in code-block-node.tsx**: Remove duplicate language entries (`Sass`/`SCSS`, `Visual Basic`/`VB.Net`)
3. **Add toolbar hint for slash commands**: Add a `"/"` button or keyboard hint in the footer bar
4. **Bundle analysis**: Run `pnpm build --analyze` to measure lowlight bundle impact
5. **Make source panel writable**: Implement markdown→editor sync so power users can edit raw markdown and have it reflect in the rich editor
6. **Evaluate pink intensity**: Get user feedback on bold=pink before committing long-term
