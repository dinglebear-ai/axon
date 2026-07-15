---
name: website-design-clone
description: Use when the user wants an agent-ready DESIGN.md derived from a site's observable visual language using Axon brand, scrape, and screenshot evidence.
---

# Axon Website Design Clone

Use this when the user wants one URL turned into a practical design system file agents can use immediately.

Default outcome: extract a practical design-system reference and format it as
`DESIGN.md`.

The skill should feel like a thin workflow around Axon page tools: gather brand identity, page content, structure, metadata, links, and visual evidence, then synthesize those findings into a clean design-system markdown file.

## Onboarding Interview

Infer the source URL, target stack, and whether implementation is requested from context. If the user gives a URL and asks for a design system, proceed immediately.

Ask at most 1-3 concise questions only if blocked, such as the website URL, whether to output only `DESIGN.md` or also implement, or a required target stack.

Use the host agent's normal prompt or modal UI. Do not name a harness-specific question function.

## Axon Collection Plan

Use Axon through the CLI or equivalent tool surface. Start with brand extraction,
page content, and a screenshot of the supplied URL:

1. `brand` for structured colors, fonts, logos, favicon, and voice/tone clues.
2. `scrape` for headings, copy hierarchy, links, metadata, and page structure.
3. `screenshot` for visual layout, density, imagery, and responsive evidence.

Example:

```bash
mkdir -p .axon
URL="https://example.com"
slug="example"
axon brand "$URL" --json > ".axon/${slug}-brand.json"
axon scrape "$URL" --json --skip-embed > ".axon/${slug}-scrape.json"
axon --output-dir .axon screenshot "$URL" --json > ".axon/${slug}-screenshot.json"
```

Use `brand` as the primary source for color, typography, logo/favicon, and voice
signals. Use `scrape` for content hierarchy and link structure. Use the
screenshot artifact path returned by Axon as the primary visual reference for
layout, hierarchy, imagery, and overall feel. Add supplemental HTML or related
pages only when those three artifacts are insufficient for the final design
system.

Collect:

- brand data for colors, typography, logos, favicon, personality, and confidence
- a full-page screenshot saved locally in `.axon/` so it can be embedded in `DESIGN.md`
- page markdown for headings, copy hierarchy, CTAs, navigation, and section order when needed
- metadata and links for brand, product, and page-purpose clues when needed
- HTML only when brand output, markdown, and screenshot are insufficient to infer classes, font names, CSS variables, or component structure
- related pages only when the user asks for a broader site system

Do not over-crawl by default. The first version should be useful from a single representative page.

## What To Extract

Infer and document the site's design language:

- colors: primary, secondary, accents, backgrounds, borders, text, states
- typography: font families if detectable, type scale, weights, line heights, heading/body treatment
- spacing: container widths, section rhythm, grid gaps, padding scale, density
- layout: page structure, hero patterns, cards, grids, nav, footer, responsive assumptions
- components: buttons, inputs, cards, badges, nav items, pricing blocks, testimonials, feature rows, forms
- imagery and icons: style, shape language, illustration/photo treatment, logo constraints, and representative hero/product/feature imagery visible in the screenshot or page links
- motion and interaction: hover states, transitions, animation style when observable or inferable
- voice and content patterns: CTA wording, heading style, product copy rhythm

When a value cannot be measured exactly from scrape output, label it as inferred and give a practical approximation.

## Parallel Work

If appropriate, use sub-agents or equivalent parallel task runners. Natural splits include one page per researcher for multi-page sites, or one reviewer each for colors, typography, spacing, and components.

Each parallel researcher should return source URLs, extracted evidence, inferred design tokens, and confidence notes.

## Final Deliverable

Create or return a `DESIGN.md` with this structure. Embed the full-page screenshot near the top so a coding agent gets visual context alongside the tokens.

```markdown
# DESIGN.md: [Source Site]

## Source
- URL: [source URL]
- Capture date: [date]
- Evidence: [scrape/screenshot/html/links used]

## Reference Screenshot
Screenshot: path returned by `axon screenshot --json`

Use this screenshot as the visual source of truth for layout, hierarchy, density, and feel. Tokens below describe the same page in machine-readable form.

## Design Summary
[Short description of the visual language and what an agent should recreate]

## Design Tokens

### Colors
[Named color roles with hex values when known; mark inferred values clearly]

### Typography
[Fonts, fallback recommendations, scale, weights, heading/body rules]

### Spacing And Layout
[Spacing scale, containers, grids, radius, shadows, borders]

## Components
[Buttons, cards, nav, forms, hero, feature sections, pricing, footer, etc.]

## Page Patterns
[Section order, common layouts, responsive behavior]

## Content Style
[Voice, CTA style, heading patterns, copy density]

## Agent Build Instructions
[Concrete instructions an AI coding agent can follow to create a new site in this style]

## Rerun Inputs
workflow: website-design-clone
source_url: [url]
target_stack: [stack]
output: DESIGN.md
```

If the user asks to implement, first produce or update `DESIGN.md`, then use it as the source of truth for the build.

## Quality Bar

- Do not copy proprietary logos, trademarks, images, distinctive trade dress, or
  copy unless the user owns the source. Create compatible or inspired tokens and
  layout guidance from observable patterns.
- Prefer reusable design tokens over one-off observations.
- Distinguish observed facts from inferred approximations.
- Keep the output compact enough that another agent can paste it into context and build from it.
- Preserve source URLs and scrape artifacts for review.
- For Axon/browser routing, see [capture-recipes.md](../../references/capture-recipes.md).
