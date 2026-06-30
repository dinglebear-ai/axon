# Presentation Contract
Last Modified: 2026-06-30

## Contract

Presentation is a shared product surface, not an implementation detail.

CLI, web, Palette desktop, Android, and Chrome extension must use the same
semantic design contract for color, typography, density, status language,
icons, accessibility, and generated token artifacts. Individual platforms may
project the tokens into platform-native formats, but they must not invent
conflicting meanings.

This contract is especially important for `apps/palette-tauri/`, because the
desktop app is expected to expose dense operational state without becoming
visually inconsistent with the rest of Axon.

## Source of Truth

Aurora is the visual source of truth for web/app tokens. Axon owns an Axon
presentation projection that maps Aurora token intent into:

- web CSS variables
- Tauri/Palette CSS variables
- Android Compose color/typography/spacing tokens
- Chrome extension CSS variables
- CLI truecolor/ANSI token constants
- generated schema/reference docs

Generated platform token files must include a contract version and source hash.

## Semantic Token Registry

### Color Tokens

| Token | Meaning | Required Platforms |
|---|---|---|
| `color.background` | App/page background. | all |
| `color.surface` | Primary panels and surfaces. | web, palette, android, extension |
| `color.surface_raised` | Elevated panels, popovers, modals. | web, palette, android, extension |
| `color.border` | Standard borders/dividers. | all |
| `color.divider` | Low-emphasis separators. | all |
| `color.text_primary` | Primary text. | all |
| `color.text_secondary` | Secondary text. | all |
| `color.text_muted` | Muted/caption text. | all |
| `color.text_inverse` | Text on strong/accent surfaces. | web, palette, android, extension |
| `color.accent` | Primary Axon action/accent. | all |
| `color.accent_strong` | Strong active/focus accent. | all |
| `color.service_name` | Secondary product/service identity accent. | all |
| `color.automation` | Agent/automation identity. | all |
| `color.success` | Completed/success state. | all |
| `color.warning` | Warning/retryable/degraded caution. | all |
| `color.error` | Failed/destructive/error state. | all |
| `color.info` | Informational state. | all |
| `color.neutral` | Idle/unknown/neutral state. | all |
| `color.waiting` | Waiting/backpressure/provider cooling. | all |
| `color.degraded` | Completed degraded/partial result. | all |
| `color.source` | Source objects. | web, palette, android |
| `color.job` | Job objects. | web, palette, android |
| `color.graph` | Graph objects. | web, palette |
| `color.memory` | Memory objects. | web, palette, android |
| `color.artifact` | Artifact objects. | web, palette, extension |
| `color.provider` | Provider objects. | web, palette |
| `color.focus_ring` | Keyboard focus outline. | web, palette, android, extension |
| `color.hover` | Hover background. | web, palette, extension |
| `color.selected` | Selected row/item background. | web, palette, android, extension |
| `color.disabled` | Disabled foreground/background treatment. | all |

Status mappings are fixed:

| Status | Token |
|---|---|
| `completed` | `color.success` |
| `completed_degraded` | `color.degraded` |
| `failed` | `color.error` |
| `canceled` | `color.neutral` |
| `waiting` | `color.waiting` |
| `blocked` | `color.warning` |
| `running` | `color.info` or `color.accent` depending on component role |
| `queued`/`pending` | `color.neutral` |

No platform may use red for degraded or warning states unless the state is
terminal failure.

### Typography Tokens

| Token | Meaning |
|---|---|
| `font.family.sans` | App UI text. |
| `font.family.mono` | Code, logs, ids, terminal output. |
| `font.family.display` | Sparse product headings only. |
| `font.size.xs` | Captions, metadata. |
| `font.size.sm` | Dense table/cell text. |
| `font.size.md` | Default body/control text. |
| `font.size.lg` | Section heading. |
| `font.size.xl` | Page heading. |
| `font.weight.regular` | Default text. |
| `font.weight.medium` | Controls, labels. |
| `font.weight.semibold` | Headings, selected labels. |
| `line_height.tight` | Dense tables/logs. |
| `line_height.normal` | Body text. |
| `line_height.relaxed` | Long-form markdown. |

Rules:

- operational surfaces default to compact readable type
- hero-scale type is not used inside app panels, tables, or dashboards
- CLI uses terminal-appropriate token projections, not CSS font sizes
- Android Compose maps tokens to `TextStyle` values

### Spacing, Radius, and Density

| Token | Meaning |
|---|---|
| `space.1` through `space.8` | Shared spacing scale. |
| `radius.xs` | Tables, chips, small controls. |
| `radius.sm` | Buttons, inputs, panels. |
| `radius.md` | Modals or larger containers. |
| `density.compact` | Palette/web operational default. |
| `density.comfortable` | Android touch-first default. |
| `density.cozy` | Read-heavy content. |

Rules:

- cards/panels use 8px radius or less unless platform style requires otherwise
- nested card-on-card layouts are forbidden for operational views
- fixed-format UI elements have stable dimensions to avoid layout shift
- Android touch targets follow platform minimums even when density is compact

## Icon and Symbol Contract

Every common operation has a semantic icon slot.

| Intent | Icon Slot | CLI Symbol |
|---|---|---|
| success | `icon.success` | `ok`/check |
| warning | `icon.warning` | `!`/warning |
| error | `icon.error` | `x`/cross |
| source | `icon.source` | `src` |
| job | `icon.job` | `job` |
| graph | `icon.graph` | `graph` |
| memory | `icon.memory` | `mem` |
| artifact | `icon.artifact` | `file` |
| provider | `icon.provider` | `svc` |
| refresh | `icon.refresh` | `refresh` |
| watch | `icon.watch` | `watch` |
| prune | `icon.prune` | `prune` |
| search | `icon.search` | `search` |
| ask | `icon.ask` | `ask` |

Web/Palette/extension should use the shared icon library chosen by the app
stack. Android uses vector drawables or Compose image vectors with the same
semantic names. CLI uses symbol fallbacks and must respect `NO_COLOR`.

## Generated Artifacts

The presentation generator emits:

```text
docs/reference/presentation/tokens.md
docs/reference/presentation/tokens.schema.json
apps/palette-tauri/src/styles/axon-tokens.css
apps/chrome-extension/src/styles/axon-tokens.css
apps/android/core/design/src/main/kotlin/.../AxonTokens.kt
crates/axon-cli/src/ui/tokens.rs
apps/web/src/styles/axon-tokens.css
```

Generator:

```bash
cargo xtask presentation generate
cargo xtask presentation generate --check
```

Generated files include:

```text
/* generated by cargo xtask presentation generate; do not edit directly */
```

## Platform Projections

| Platform | Projection |
|---|---|
| CLI | Rust constants for truecolor and ANSI-256 fallback. |
| Web | CSS variables and utility class mapping. |
| Palette | CSS variables plus Tauri-safe theme metadata. |
| Android | Compose `ColorScheme`, typography, spacing, and semantic status helpers. |
| Chrome extension | CSS variables with reduced bundle size and CSP-safe usage. |

Projection rules:

- token names are stable across platforms
- platform files include all required tokens even if some project to the same
  value
- dark and light modes are explicit
- high-contrast mode has overrides for status colors and focus rings
- CLI fallback colors are tested on 24-bit and ANSI-256 terminals

## Accessibility Contract

Required checks:

- text contrast meets WCAG AA for normal text and controls
- focus ring is visible on every interactive element
- status is not color-only; include text/icon/shape
- reduced motion disables non-essential animation
- keyboard navigation is complete for web, Palette, and extension
- Android content descriptions exist for non-decorative icons
- terminal output is meaningful with `NO_COLOR`

## Testing Contract

Required tests:

- token schema validates generated token source
- every required semantic token exists in every platform projection
- status-to-color mapping snapshot tests
- dark/light/high-contrast snapshots
- CLI truecolor and ANSI fallback snapshots
- Android Compose token unit tests
- web/Palette/extension visual smoke for status components
- no platform-specific token drift without contract update

## Acceptance Criteria

- all app surfaces use generated tokens, not local ad hoc palettes
- Palette has a complete desktop token projection
- Android, web, extension, and CLI token projections are generated from the same
  source
- status colors and labels mean the same thing everywhere
- generated token files are reproducible in check mode
- accessibility checks pass for all primary status/control components
