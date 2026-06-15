# dab6 — Palette Web Visual Baseline (BEFORE migration)

This is the **"before" baseline** for `axon_rust-dab6.5`'s before/after parity diff.
Captured per `axon_rust-dab6.11`, **before** `axon_rust-dab6.3` mutates
`apps/palette-tauri/src/styles.css` (button-primitive migration / dup-CSS deletion).

| Field | Value |
|-------|-------|
| Date captured | 2026-06-15 18:18 EDT |
| Git branch | `claude/aurora-split-web` |
| Git SHA | `055e7a4d0406886842c85c9e53b7b5e8acc38113` (`055e7a4d`) |
| Bundle | `apps/palette-tauri` built with `pnpm vite:build` (vite 7.3.3) |
| Capture host | **agent-os** (Windows 11 sandbox VM) — Chrome headless via CDP |
| Render mode | Headless Chrome (`--headless=new`), DevTools Protocol screenshots |
| Backend | None — browser fallback in `src/lib/invoke.ts`; fixture route is backend-free |

## Why agent-os

dookie (the dev host) kills any long-lived local process between tool calls — a
`--remote-debugging-port` Chrome needed to drive interactive states (focus, type,
click) is SIGKILLed before it can be driven. Only a self-terminating one-shot
`chrome --screenshot` survives there, which cannot reach interactive App states.
agent-os keeps servers/Chrome alive, so the full interactive capture ran there.
The static `dist/` bundle was built on dookie, copied to agent-os, served over a
local Node static server, and driven with a CDP script (`/tmp/dab6-capture-win.mjs`).

## Screenshots

| File | Surface / state | Viewport | Notes |
|------|-----------------|----------|-------|
| `01-app-default-command-bar.png` | App default — idle command bar (logo, search input, help/send/settings icons) | 900x640 | Compact idle shell, no ActionList yet |
| `02-app-actionlist-query-s.png` | App — command bar focused, query `s`, **populated ActionList** + footer | 900x640 | Best ActionList baseline: FETCH&READ (Scrape selected w/ Run+help, Screenshot) + SEARCH&DISCOVER (Search, Sources); method chips, keyword badges, icons; footer keybind hints |
| `03-app-actionlist-query-crawl.png` | App — query `scrawl` (no match), **empty ActionList** state + footer | 900x640 | Empty-results ActionList region; footer present |
| `04-app-settings-panel.png` | App — **Settings panel** open | 900x640 | Tabs (Connection / Environment 44 / config.toml 74), Server/token/Collection fields, Global shortcut, Max results, Hide-on-blur + Open-results-inline toggles, Test connection / Close / Save footer, "not tested" status |
| `05-app-footer-and-content.png` | App — query `ask`, ActionList (Ask action) + **footer** | 900x640 | REASON category, Ask action (POST /v1/ask, Run); footer: recent / navigate / select / run / close |
| `10-fixture-operation-results.png` | **`?fixture=operation-results`** route — full OutputPanel matrix | 1200x3000 | Backend-free. Covers OperationResultView across: Structured Error, Scrape Reader (markdown+code), Retrieve Empty, Search Results, Research Summary, Ask Code Answer, Query Matches, Doctor Degraded, Long Error Body, Screenshot Preview, Watch Empty. Shiki syntax highlighting active. **Primary parity target.** |

## Surface coverage vs the dab6.11 task list

Task asked for: command bar, ActionList, settings, ask, output, footer, crawl/stats/status.

| Requested surface | Covered? | Where |
|-------------------|----------|-------|
| Command bar | ✓ | 01, 02, 03, 05 |
| ActionList (populated) | ✓ | 02, 05 |
| ActionList (empty/no-match) | ✓ | 03 |
| Settings panel | ✓ | 04 |
| Footer | ✓ | 02, 03, 04 (settings footer), 05 |
| Output panel (success + error states) | ✓ | 10 (Scrape/Retrieve/Search/Research/Query/Doctor/Screenshot/Watch + 2 error cases) |
| Ask (output/answer view) | ✓ | 10 (Ask Code Answer case w/ follow-up input); 05 (ask action pre-run) |
| Crawl (CrawlJobView live progress) | ✗ | **Not baselined** — `run.kind === "job"` requires a live backend crawl job; the fixture only exercises `OutputPanel` (success/error), not `CrawlJobView`. See gap below. |
| Stats / Status | ~ | Covered as OutputPanel result *kinds* via the fixture's Doctor/Watch/Query/Sources-style result rendering (same OutputPanel code path). No dedicated stats/status fixture exists, so these render through the same OutputPanel component captured in `10`. |

## Known coverage gaps (be honest)

1. **CrawlJobView live progress (`run.kind === "job"`)** is NOT captured. It renders
   only during an in-flight crawl with a live `axon serve` backend streaming job
   snapshots; there is no backend-free fixture for it. If dab6.3 touches crawl-job
   CSS, dab6.5 will need a live-backend capture for that one view. The fixture
   (`OperationResultFixture.tsx`) deliberately covers only `OutputPanel`, not
   `CrawlJobView` / `HistoryPanel`.
2. **HistoryPanel** is likewise not in the fixture and not captured (needs
   interaction with run history). Low risk if dab6.3 is button/CSS-scoped.
3. **Hover / focus-ring / active button states** are not separately captured — the
   shots are at-rest renders (one element shows its default selected state in 02/05).

## Reproduce / re-capture (for dab6.5 "after")

```bash
# On dookie:
cd apps/palette-tauri && pnpm install --frozen-lockfile && pnpm vite:build
tar czf /tmp/dab6-dist.tgz -C dist .
scp /tmp/dab6-dist.tgz agent-os:'C:/dab6/dist.tgz'
# extract on agent-os to C:\dab6\dist, then run the CDP driver with node:
scp /tmp/dab6-capture-win.mjs agent-os:'C:/dab6/capture.mjs'
ssh agent-os 'powershell -NoProfile -Command "cd C:\dab6; & C:\nvm4w\nodejs\node.exe capture.mjs"'
scp 'agent-os:C:/dab6/shots/*.png' <after-dir>/
```

The driver script (`/tmp/dab6-capture-win.mjs`) is also archived alongside these PNGs
as `capture-driver.mjs` for exact reproducibility.
