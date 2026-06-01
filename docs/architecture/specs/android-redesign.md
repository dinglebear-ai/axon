# Axon Android — UI Redesign Spec
_Last updated: 2026-05-28 · Status: **DESIGN LOCKED — ready for implementation plan**_

---

## Overview

Full visual redesign of the Axon Android app (`apps/android/`) aligned with the Aurora design system. The app already uses `AuroraTheme` and `AuroraPromptInput`; this redesign replaces all remaining raw Material3 primitives with Aurora components and restructures navigation around **Ask as the primary surface**.

---

## Decisions Locked

### 1. Navigation: Side Rail (5 sections) + Overlay Drawer

**Ask is the permanent content area.** The app opens into Ask. There is no rail icon for Ask — it is always visible behind any overlay.

**The rail has exactly 5 sections.** Each opens a full-height overlay drawer showing sub-items. All sections are treated identically — same visual pattern, same interaction model. Sessions is not special.

| Rail icon (Material Symbols Rounded) | Section | Drawer sub-items |
|--------------------------------------|---------|-----------------|
| `history` | Sessions | New Session · [4 most recent sessions] |
| `checklist` | Jobs | Crawls · Embeddings · Ingestions · Extractions · Watches |
| `hub` | Knowledge | Suggest · Sources · Domains · Stats |
| `settings` | Management | Dedupe · Monitor · Sync · Stack · Config |
| `construction` | Setup | Preflight · Setup · Smoke · Doctor · Debug |

**Icons:** Material Symbols Rounded, `FILL=1`, size 20sp throughout the rail and all drawers. No emoji anywhere in the UI.

**Rail item anatomy:** 46×42dp touch target, 13dp radius, `navBg` background. Active item: `rgba(41,182,246,0.12)` fill + `accentPrimary` left-edge indicator bar (3×22dp, 0 2 2 0 radius, glow). Icon 20sp + 7sp uppercase label below.

**Drawer behavior:** Slides in from the left over dimmed + blurred content (`rgba(4,10,14,0.68)` + `blur(2px)`). Tap outside or press back to dismiss. Each drawer: 232dp wide, `panelStrong` bg, `borderStrong` right edge, deep drop shadow. Header: section icon + section title (Manrope 700 14sp). Sub-items in 8dp padded list.

**Drawer sub-item anatomy (all sections except Jobs drill-down):**
- 11dp radius, `transparent` border normally; active: `rgba(41,182,246,0.08)` bg + `rgba(41,182,246,0.18)` border
- Row: 17sp Material icon (muted normally, `accentPrimary` when active) · label (Inter 600 11.5sp) · optional status badge (right-aligned)
- Optional 2nd line: 9.5sp detail text in `textMuted`, `JetBrains Mono` for URLs/counts, `accentStrong` for URLs, `successBase` for counts, `errorBase` for errors
- Optional progress bar below detail text (see §5)

**Sessions drawer specifics (same pattern, standard sub-items):**
- "New Session" is the first sub-item (starts a fresh Ask conversation)
- 4 most recent sessions listed below, each with:
  - Auto-generated title (from first message, like Claude.ai)
  - Relative timestamp
  - First-message preview line
  - Turn count + injected ops (e.g. "4 turns · 1 crawl")
- Long-press any session → rename / pin (pinned sessions show a pin icon prefix)

**Jobs drawer — two-level navigation:**
- Level 1 (overview): 5 sub-items (Crawls / Embeddings / Ingestions / Extractions / Watches), each showing aggregate status badge + live detail line + progress bar if active
- Level 2 (drill-down): tapping a sub-item replaces drawer content with a list of individual jobs for that type; back arrow returns to overview
- Each individual job row: status dot · monospace URL/target · progress bar · counts + elapsed time

---

### 2. Ask — Primary Screen

Always visible. Opens on launch. No rail icon — it is home.

**Conversation style: Chat bubbles + Axon avatar**

- **User messages:** right-aligned, `accentPrimary` tint (`rgba(41,182,246,0.1)` bg, `rgba(41,182,246,0.25)` border, `border-radius: 16 16 4 16dp`)
- **Axon responses:** left-aligned, 24dp ✦ avatar (cyan→violet gradient, `accentPrimary` border), `AuroraThinking` dots while streaming, uppercase "AXON" label in `accentPrimary` above each response
- **Injection cards:** distinct tinted card (`rgba(41,182,246,0.05)` bg, `rgba(41,182,246,0.18)` border) for crawl/ingest completions — inline between conversation turns
- `AuroraPromptInput` at bottom (already implemented)
- FAB sits above prompt input, bottom-end anchored

---

### 3. FAB — Full-Circle Operation Launcher

**Resting:** 42×42dp rounded-square (13dp radius), `panelStrong` bg, `accentPrimary` icon, `borderStrong` border.

**On tap:** Background dims (`rgba(4,10,14,0.82)` + `blur(3px)`). FAB transforms to center of a full 360° ring showing all 10 operations. Center FAB shows × to dismiss.

**Tap outside or tap center ×:** Ring collapses, no operation selected.

**10 operations in ring** (36° apart, r=96dp, starting 12 o'clock clockwise):

| Clock pos | Operation | Tile style |
|-----------|-----------|-----------|
| 12 | Scrape | default |
| 1 | Research | default |
| 3 | Extract | default |
| 4 | Query | default |
| 5 | Search | default |
| 6 | Map | default |
| 7 | Retrieve | default |
| 8 | Summarize | default |
| 9 | Crawl | `warnBase #C6A36B` tint — async |
| 10 | Ingest | `warnBase #C6A36B` tint — async |

**After selecting an operation:**
Ring collapses → **floating input card appears center-screen** (where the ring was — thumb stays in place).

Input card (`panelStrong` bg, `accentPrimary` border + glow, 20dp radius):
- Header: operation icon tile + operation name (Manrope 700) + subtitle
- Input row: URL/query field (focused, `accentPrimary` border + cursor glow) + clipboard paste button + send button (`accentButton`)
- Hint: "enter to send · tap outside to cancel"

**Inject behavior:**

| Operations | Input | What lands in conversation |
|-----------|-------|--------------------------|
| Scrape, Research, Extract, Query, Search, Map, Retrieve, Summarize | URL or query text | Full result as Axon bubble |
| Crawl | URL | Compact injection card (see below) |
| Ingest | URL / target | Compact injection card (see below) |

**Compact injection card text (Crawl / Ingest):**
```
axon mobile just crawled https://code.claude.com and indexed 120 docs (12,049 chunks)
into your knowledge base — use `axon query` + `axon retrieve` + `axon ask` via MCP
or CLI to semantically search your knowledge base.
```
This exact phrase triggers the **Axon Mobile Gemini skill** (§7).

---

### 4. Aurora Token Reference

| Element | Token | Value |
|---------|-------|-------|
| App background | `pageBg` | `#07131C` |
| Rail / nav background | `navBg` | `#07111A` |
| Card default | `panelMedium` | `#102330` |
| Card elevated | `panelStrong` | `#13293A` |
| Input surface | `controlSurface` | `#0C1A24` |
| Primary accent | `accentPrimary` | `#29B6F6` |
| Accent bright | `accentStrong` | `#67CBFA` |
| Send button | `accentButton` | `#1DA8E6` |
| Text on accent | `accentFg` | `#051520` |
| Async op tint | `warnBase` | `#C6A36B` |
| Success / indexed | `successBase` | `#7DD3C7` |
| Border default | `borderDefault` | `#1D3D4E` |
| Border strong | `borderStrong` | `#24536C` |
| Primary text | `textPrimary` | `#E6F4FB` |
| Secondary text | `textMuted` | `#A7BCC9` |
| Typography | Manrope 700/800 (headings) · Inter 400/500/600 (body) · JetBrains Mono (code) | — |
| Radii | `radius1=14dp` `radius2=18dp` `radius3=22dp` | — |

**Aurora component mapping (replace current raw Material3):**

| Current | Replace with |
|---------|-------------|
| Ask history `Column` + `Text` | `AuroraMessage` |
| Loading spinner | `AuroraThinking` / `AuroraAiShimmer` |
| Error column | `AuroraErrorPage` / `AuroraCallout` |
| Empty state column | `AuroraEmptyState` |
| Job rows in `LazyColumn` | `AuroraCard` + `AuroraStatusIndicator` + custom `AuroraProgressBar` |
| Source/domain rows | `AuroraSourcesList` / `AuroraDescriptionList` |
| Mode picker | `AuroraSheet` wrapping circle ring composable |
| Floating op input | `AuroraTextField` in floating `AuroraCard` |
| Progress indicators | Custom `AuroraProgressBar` composable (see §5) — `LinearProgressIndicator` is not Aurora-spec |
| Status dots | Custom `AuroraStatusDot` composable (7dp, glow via `Modifier.shadow` or `drawBehind`) |
| Suggest cards | `AuroraCard` with `AuroraChip` action button |

---

### 5. Progress Bars (Aurora Spec)

Used in Job rows (drawer overview + drill-down) wherever a job is running, completed, or failed. Never shown for idle items.

**Track:** `controlSurface` (`#0C1A24`) background · `1px solid borderDefault` border · border-radius = height ÷ 2 (fully rounded) · `overflow: hidden`

**Fill — 4 variants:**

| Variant | When | Gradient | Glow |
|---------|------|----------|------|
| `cyan` (default) | Running | `#1DA8E6 → #4DC8FA → #67CBFA` | `0 0 8px rgba(41,182,246,0.55), 0 0 2px #29B6F6` |
| `success` | Done | `#3A7A74 → #7DD3C7` | `0 0 6px rgba(125,211,199,0.4)` |
| `error` | Failed | `#7A3040 → #C78490` | `0 0 6px rgba(199,132,144,0.4)` |
| `warn` | Stalled / slow | `#7A5E2E → #C6A36B` | `0 0 8px rgba(198,163,107,0.4)` |

**Shimmer overlay (running bars only):** absolute `inset:0` span, `linear-gradient(90deg, transparent, rgba(230,244,251,0.32), transparent)`, `translateX(-100%) → translateX(400%)`, `2.2s ease-in-out infinite`.

**Sizes:**
- `sm` (4dp) — used in drawer overview sub-item rows
- `default` (6dp) — used in drill-down individual job rows

**Indeterminate mode:** fill width fixed at 35%, `translateX(-150%) → translateX(350%)` animation `1.5s ease-in-out infinite`. Used when total page count is unknown (new crawl with no estimate yet).

**Determinate mode:** fill width = `(completed / estimated_total) * 100%`, `transition: width 600ms cubic-bezier(0.4,0,0.2,1)`.

**Status dots (drill-down rows):** 7dp circle, glow matching variant color, pulse animation on `running`.

| State | Color | Glow | Animation |
|-------|-------|------|-----------|
| running | `accentPrimary #29B6F6` | `0 0 6px #29B6F6` | pulse 1.8s |
| done | `successBase #7DD3C7` | `0 0 5px rgba(125,211,199,.7)` | none |
| failed | `errorBase #C78490` | `0 0 5px rgba(199,132,144,.7)` | none |
| idle | `textMuted #A7BCC9` | none | none |
| warn | `warnBase #C6A36B` | `0 0 5px rgba(198,163,107,.6)` | none |

**Job data per type:**

| Type | Drawer overview detail | Drill-down per-job detail |
|------|----------------------|--------------------------|
| Crawls | active URLs (up to 2) · page counts | domain · progress bar (pages/est) · chunk count · depth · elapsed |
| Embeddings | chunk count · avg ms/chunk | source URL · chunk count · throughput |
| Ingestions | last target (source_type + truncated name) · file count | type icon · target · file/chunk count · elapsed |
| Extractions | last URL or error | URL · partial progress or error message |
| Watches | next run countdown | schedule · last run result · next trigger |

---

### 6. Knowledge — Suggest Screen

Tapping "Suggest" from the Knowledge drawer opens a **full-screen list** (not a sub-drawer). The drawer closes; the screen replaces the Ask content behind the rail.

Each suggest card:
- `panelMedium` bg, `borderDefault` border, 11dp radius
- Left: 17sp Material icon (`article` / `inventory_2` / `code` / `filter_drama` etc.) in `accentPrimary`
- Body: domain name (Inter 600 11sp) · monospace URL (8.5sp, `textMuted`) · reason text (9sp italic, `textMuted`)
- Right: action chip — **Crawl** (cyan tint) or **Ingest** (amber `warnBase` tint, signals async)
- Tapping the chip fires the operation directly (same flow as FAB → operation → send, but URL is pre-filled)

Reason text examples: "Queried 3× this week, 0 hits" · "New version, indexed copy is older" · "Dependency — not yet indexed" · "N queries · no indexed result returned".

---

### 7. Axon Mobile Gemini Skill

**Trigger phrases** (injected by the app into the Ask conversation):
- `"axon mobile just crawled"`
- `"axon mobile just ingested"`

**On trigger:** Skill instructs Gemini to use Axon MCP tools rather than hallucinating:
1. `axon query "<topic>"` — semantic search over indexed content
2. `axon retrieve "<url>"` — fetch specific chunks from a known URL
3. `axon ask "<question>"` — full RAG synthesis

---

### 8. Gemini Extension (Isolated Config)

Standalone Gemini extension, completely isolated from `~/.gemini/`. No bloat, no shared state.

**Delivers:**
- `.mcp.json` — connects Gemini to the Axon MCP server
- `skills/axon-synthesis.md` — existing RAG synthesis skill
- `skills/axon-mobile.md` — new mobile trigger skill (§7 above)

---

## Pending Decisions

_All design decisions are now locked. No open items remain._

---

## Out of Scope

Light theme · Tablet layouts · Push notifications · Offline mode
