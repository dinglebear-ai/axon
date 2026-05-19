# UI Design System Overhaul — Session Log
Date: 2026-02-28 07:06 EST
Branch: `feat/crawl-download-pack`

## Session Overview
Executed a 7-agent parallel team to address all 33 UI design review issues across `apps/web`. Wave 1 established the complete CSS design token foundation; 6 Wave 2 agents ran simultaneously with zero file conflicts. 5 commits landed, build passed clean throughout.

## Timeline

1. **Design review synthesis** — 4 parallel Explore agents reviewed all UI domains; findings synthesized into `docs/reports/ui-design-review-2026-02-27.md` (33 issues, H/M/L priority).
2. **Plan written** — `docs/plans/2026-02-27-ui-design-system-overhaul.md` (72 KB) detailing Wave 1/2 architecture, complete CSS token reference, per-agent file ownership map, step-by-step implementation with exact code.
3. **Team created** — `TeamCreate` for `ui-overhaul`; 7 tasks created with blockedBy dependencies (Wave 2 tasks all blocked by Task #1).
4. **Wave 1 dispatched** — `wave1-foundation` agent ran; committed `b585aef` with all design tokens, fonts, keyframes, and atmosphere changes.
5. **Wave 2 dispatched** — All 6 agents spawned in parallel immediately after Wave 1 confirmed SHA.
6. **Wave 2 completed** — All 6 agents reported commits; team cleaned up; final build verified.
7. **Push** — Changelog updated with 11 new commits; `5dc43f1` pushed.

## Key Findings

- `apps/web/app/layout.tsx` was importing `Outfit` (body) — replaced with `Space_Mono` (display) + `Sora` (body), kept `JetBrains_Mono`.
- `apps/web/app/globals.css` had 3 keyframes and no shadow/surface/border token system — expanded to 30+ CSS vars and 8 keyframes.
- `--axon-text-dim: #5f87af` failed WCAG AA (~3.2:1 contrast) — bumped to `#7a96b8` (~5.1:1).
- Scrollbar thumb `rgba(255,135,175,0.15)` (pink, low contrast) — changed to `rgba(135,175,255,0.35)` (blue, 4.5:1).
- `table-renderer.tsx` rendered all rows into DOM — `@tanstack/react-virtual` added; threshold 200 rows, top-N toggle at 1000+.
- Agent 7 (canvas) and Agent 5 (omnibox) staged concurrently — both landed in `e3a0c96`. All files correct.

## Technical Decisions

- **Two-wave architecture** — Wave 1 must commit before Wave 2 so CSS tokens exist for all component agents to reference. Dependency enforced via `TaskCreate` `blockedBy`.
- **Strict file ownership** — 28 files mapped to 7 agents, zero overlap, zero merge conflicts.
- **`satisfies VisualPresetConfig`** for zen profile — ensures structural correctness without widening the return type.
- **`useNeuralCanvasProfile` hook pattern** (not context) — least invasive; profile is currently a read-only prop, so the hook pattern adds persistence without restructuring the tree.
- **Inline rgba kept for non-token opacities** — `rgba(10,18,35,0.3/0.4/0.45)` in `style={}` props don't map to any exact surface token; kept per plan instruction.
- **`monolith` policy** — agent3 proactively split `table-renderer.tsx` into `table-primitives.tsx` + `table-views.tsx` to stay under 500-line limit.

## Files Modified

### Wave 1
| File | Change |
|------|--------|
| `apps/web/app/layout.tsx` | Outfit → Space_Mono + Sora; 3-font variable body className |
| `apps/web/app/globals.css` | 30+ design tokens, 8 keyframes, 7 @utility aliases, body gradient, grain overlay, WCAG fixes |

### Wave 2 — Agent 2 (Pulse)
| File | Change |
|------|--------|
| `components/pulse/message-content.tsx` | Asymmetric alignment, user bubble gradient, word count, copy animation, timestamp |
| `components/pulse/pulse-chat-pane.tsx` | Empty state radial glow, source slide-down, breathing loader |
| `components/pulse/tool-badge.tsx` | Hover scale+glow, pin indicator dot |
| `components/pulse/pulse-mobile-pane-switcher.tsx` | Full rewrite — labeled tablist with ARIA |
| `components/pulse/pulse-workspace.tsx` | Divider grip dots, ARIA valuenow, shadow token |
| `components/pulse/pulse-toolbar.tsx` | Unsaved indicator dot, focus design tokens |

### Wave 2 — Agent 3 (Results)
| File | Change |
|------|--------|
| `components/results/table-renderer.tsx` | Virtual scrolling (200+ rows), top-N toggle (1000+), stagger animations |
| `components/results/table-primitives.tsx` | NEW — extracted FilterInput, SortHeader, UrlCell, StatusBadge, TopNToggle, VirtualTableBody |
| `components/results/table-views.tsx` | NEW — extracted StatusTable, SuggestTable, RetrieveView (monolith split) |
| `components/results/doctor-report.tsx` | Failure-first grouping, asymmetric 2:1 metric grid, design tokens |
| `components/results/raw-renderer.tsx` | TerminalSquare empty state, breathing dot processing state |
| `components/crawl-file-explorer.tsx` | tabIndex + onKeyDown + focus-visible ring |
| `components/command-options-panel.tsx` | Focus-visible rings, --border-accent/--border-strong tokens |
| `components/content-viewer.tsx` | Inline copy button with animate-check-bounce success state |

### Wave 2 — Agent 4 (UI Primitives)
| File | Change |
|------|--------|
| `components/ui/button.tsx` | hover:scale-[1.03]/active:scale-[0.98], --focus-ring-color outline, primary glow, disabled:scale-100 |
| `components/ui/input.tsx` | Branded focus outline, surface-elevated bg on focus, transition |
| `components/ui/tabs.tsx` | Branded focus outline, duration-150 |
| `components/ui/dropdown-menu.tsx` | focus:bg-[--surface-elevated], data-[highlighted] outline ring |
| `components/ui/scroll-area.tsx` | Thumb rgba(135,175,255,0.35) + hover 0.55, WCAG fix |

### Wave 2 — Agent 5 (Omnibox)
| File | Change |
|------|--------|
| `components/omnibox.tsx` | completionStatus state (4s persist), mentionTipSeen localStorage, animate-fade-in-up stagger on suggestions |

### Wave 2 — Agent 7 (Neural Canvas)
| File | Change |
|------|--------|
| `components/neural-canvas.tsx` | useNeuralCanvasProfile hook with localStorage, exported |
| `lib/pulse/neural-canvas-presets.ts` | zen profile: brightness 0.3, density 0.4, 20 particles, high burstThreshold/calmRecovery |

### Wave 2 — Agent 6 (Pages)
| File | Change |
|------|--------|
| `app/mcp/page.tsx` | Modal delete dialog (deleteModal state), radial-glow empty state, Network icon |
| `app/mcp/components.tsx` | Unified handleSave(), sticky footer save button, STATUS_DOT checking → blue animate-pulse |
| `app/settings/page.tsx` | Modal reset dialog, SectionHeader font-display + icon container, SectionDivider gradient, sidebar border-r, border-l-2 accent bars, max-w-[780px] |
| `app/agents/page.tsx` | Empty state with Bot icon + CLI hint |

### Misc (push commit)
| File | Change |
|------|--------|
| `CHANGELOG.md` | 11 new commits documented, UI overhaul + workspace explorer highlights |
| `.monolith-allowlist` | Updated entries |
| `crates/jobs/CLAUDE.md` | Job system docs update |
| `crates/jobs/common/watchdog.rs` | Watchdog improvements |
| `crates/jobs/crawl/repo.rs` | Crawl repo fix |
| `crates/jobs/extract.rs` | Extract job improvements |

## Commits Landed

| SHA | Message |
|-----|---------|
| `b585aef` | feat(design): establish design token foundation — fonts, palette, motion, atmosphere, shadows, a11y |
| `4bdee4b` | feat(ui): button/input hover micro-interactions, branded focus rings, scrollbar contrast fix |
| `e3a0c96` | feat(omnibox): status bar persistence, @mention discovery tip, staggered suggestions [+canvas] |
| `7ca6184` | feat(pulse): motion, empty state, message alignment, tool badge discoverability, mobile pane labels, divider improvements |
| `e73906a` | feat(pages): modal delete dialogs, MCP single save, settings typography, empty states, layout improvements |
| `5dc43f1` | chore: update changelog for UI overhaul + workspace explorer; misc Rust job fixes |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `pnpm run build` | Zero TS errors, 22 routes | ✓ Compiled 7.9s, 22 routes | ✅ |
| `grep rgba(255,135,175 components/` | Zero high-contrast violations | No output | ✅ |
| `grep rgba(175,215,255 components/` | Zero legacy primary values | No output | ✅ |
| `git push` | Pushed to remote | `8e1f4e1..5dc43f1` | ✅ |
| Pre-commit hooks | All pass | biome, monolith, rustfmt, check, test (446) all pass | ✅ |

## Design Token Reference (established this session)

```css
--font-display: var(--font-space-mono)
--font-sans: var(--font-sora)
--font-mono: var(--font-jetbrains-mono)
--axon-primary: #87afff
--axon-primary-strong: #afd7ff
--axon-secondary: #ff87af
--axon-secondary-strong: #ff9ec0
--surface-base: rgba(10,18,35,0.85)
--surface-elevated: rgba(10,18,35,0.60)
--surface-float: rgba(10,18,35,0.35)
--border-subtle: rgba(135,175,255,0.15)
--border-standard: rgba(135,175,255,0.28)
--border-strong: rgba(135,175,255,0.40)
--border-accent: rgba(255,135,175,0.25)
--text-primary: #e8f4f8
--text-secondary: #b8cfe0
--text-muted: #7a96b8
--text-dim: #4d6a8a
--shadow-sm/md/lg/xl (layered with blue inner ring)
--focus-ring-color: rgba(135,175,255,0.50)
Keyframes: fade-in-up, fade-in, scale-in, badge-glow, breathing, check-bounce, divider-glow, slide-down-reveal
```

## Risks and Rollback

- **Font swap (Space_Mono):** Monospaced display font is high-contrast choice — if copy feels too technical, swap back in `layout.tsx` and remove `h1-h4` font-family rule in `globals.css`.
- **Virtual scrolling:** `@tanstack/react-virtual` is a new dependency. If issues arise, remove virtualizer and revert to plain map (threshold guard means small tables are unaffected).
- **Co-committed agent work (e3a0c96):** Omnibox and canvas files share a commit. If either needs revert, use `git revert e3a0c96` (reverts both) or cherry-pick individual file restores.
- **Rollback path:** `git revert b585aef..e73906a` (5 commits) restores pre-overhaul state.

## Decisions Not Taken

- **CSS Modules / styled-components** — Plan called for CSS custom properties in globals.css. Tailwind v4 `@utility` aliases used instead of component-scoped styles. Keeps everything in one place.
- **Separate `zen` profile file** — Kept in `neural-canvas-presets.ts` alongside other profiles. No need for new file.
- **Agent7 context integration** — Profile persistence added as a hook, not wired into existing global context, to minimize invasiveness. Can be upgraded later.

## Open Questions

- `badge.tsx` had no rgba values and no spec items — agent4 skipped it. Confirm no design debt there.
- `crawl-download-toolbar.tsx` has `rgba(10,18,35,0.4)` in inline styles — not in any agent's file scope, not a token-exact match. Leave or fix in follow-up.
- `e3a0c96` commit message says "omnibox" but includes canvas files — commit message is misleading. Acceptable for branch; clean up in squash merge if desired.

## Next Steps

- Open PR from `feat/crawl-download-pack` → `main` when ready
- Visual QA pass against design review report (`docs/reports/ui-design-review-2026-02-27.md`)
- Move plan to `docs/plans/complete/2026-02-27-ui-design-system-overhaul.md`
