# Palette Beautification — Crystalline Design
Date: 2026-05-28  
Status: Approved  
Scope: `apps/palette-tauri`

## Summary

Beautify the Axon desktop palette using the "Crystalline" direction: Option C's sharp dark precision shell with Option B's cyan-blue accent system. Mode pill becomes a ghost chip with a × dismiss button.

## Design Decisions

| Decision | Value |
|---|---|
| Base aesthetic | C — sharp dark, near-black surfaces, high contrast text |
| Accent system | B — `#29b6f6` cyan primary throughout |
| Active row treatment | 3px inset cyan left border + dark tinted background |
| Send button | `linear-gradient(135deg, #29b6f6, #1565c0)` with cyan glow |
| Mode pill | Ghost chip — transparent bg, cyan border, × dismiss |
| Status dot | Glowing cyan (`box-shadow: 0 0 6px rgba(41,182,246,0.6)`) |
| Footer | Near-black `#060e17`, ultra-muted text |

## Token Changes

All changes are overrides in `src/components/aurora.css` and direct values in `styles.css`. Aurora's existing `--aurora-accent-primary: #29b6f6` is already correct — no change needed there.

| Token / property | Old (approx) | New |
|---|---|---|
| `--aurora-shell-bg` | `#0d1f2d` | `#0b1622` |
| `--aurora-panel-medium` (cmd bar bg) | varies | `#0f1d2b` |
| `--aurora-page-bg` (output pane) | `#07131c` | `#070f18` |
| `--aurora-control-surface` (input bg) | `#0f1d28` | `#071018` |
| `.output-body` background | panel variant | `#040b12` |
| `.footer` / `.palette-footer` bg | nav-bg | `#060e17` |
| Submit button gradient | rose gradient | `#29b6f6 → #1565c0` |
| Submit button shadow | rose glow | `0 2px 10px rgba(41,182,246,0.38)` |
| Active action row bg | accent tint | `#0d1f30` |
| Active action row border | accent border | `rgba(41,182,246,0.22)` |
| Active action row shadow | none | `inset 3px 0 0 #29b6f6` |

## Component Changes

### 1. Command bar (`styles.css` — `.command-bar`, `.command-submit`)

- Background: `#0f1d2b`
- Border-bottom: `1px solid rgba(255,255,255,0.07)`
- Input (`.command-input`): bg `#071018`, border `rgba(255,255,255,0.09)`, `inset 0 1px 3px rgba(0,0,0,0.35)` shadow
- Submit button: replace rose gradient with cyan→deep-blue gradient + cyan glow shadow
- Disabled submit: bg `#0d1e2d`, border `rgba(255,255,255,0.07)`, no shadow

### 2. Mode pill → Ghost chip (`App.tsx` + `styles.css`)

Replace `.command-mode-pill` with a ghost chip that has an explicit × dismiss button:

```tsx
<button className="command-mode-pill" type="button" onClick={() => setModeAction(null)}>
  {modeAction.subcommand}
  <span className="mode-pill-dismiss" aria-hidden>×</span>
</button>
```

CSS for `.command-mode-pill`:
```css
.command-mode-pill {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  height: 27px;
  padding: 0 7px 0 10px;
  background: transparent;
  border: 1px solid rgba(41, 182, 246, 0.28);
  border-radius: 6px;
  color: #29b6f6;
  font-family: var(--aurora-font-mono);
  font-size: 11px;
  font-weight: 600;
  cursor: pointer;
  white-space: nowrap;
  flex-shrink: 0;
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
}
```

### 3. Action rows (`styles.css` — `.action-row`)

Active state:
```css
.action-row:hover,
.action-row-selected {
  background: #0d1f30;
  border-color: rgba(41, 182, 246, 0.22);
  box-shadow: inset 3px 0 0 #29b6f6, 0 1px 5px rgba(0, 0, 0, 0.28);
}
```

Non-active rows: dimmer at ~50% opacity when another row is active (optional enhancement).

### 4. Output panel (`styles.css` — `.output-panel`, `.output-body`)

- Panel bg: `#070f18`
- Body bg: `#040b12`, border `rgba(255,255,255,0.06)`
- Output heading border-bottom: `rgba(255,255,255,0.04)`

### 5. Footer (`styles.css` — `.palette-footer`)

```css
.palette-footer {
  background: #060e17;
  border-top: 1px solid rgba(255,255,255,0.05);
  color: #1e3448;   /* ultra-muted hints */
}
```

Status dot: replace `--aurora-accent-pink` brand dot with:
```css
.brand-dot {
  background: #29b6f6;
  box-shadow: 0 0 6px rgba(41, 182, 246, 0.6);
}
```

### 6. Panel heading (`styles.css` — `.panel-heading`)

```css
.panel-heading {
  color: #1e3448;
  font-weight: 700;
  letter-spacing: 0.1em;
}
```

## Files to Change

| File | Changes |
|---|---|
| `apps/palette-tauri/src/styles.css` | Token overrides, all palette-* classes, ghost chip CSS |
| `apps/palette-tauri/src/App.tsx` | Add `.mode-pill-dismiss` span inside mode pill button |
| `apps/palette-tauri/src/components/aurora.css` | Shell/panel/page bg token overrides for palette context |

## Out of Scope

- No layout changes — grid, sizing, and responsive breakpoints unchanged
- No animation/transition changes
- No new components
- Settings panel: only inherits token changes, no structural changes
- Chrome extension and desktop app: not touched
