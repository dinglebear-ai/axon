// Presentation-token consumption point for Palette (docs/pipeline-unification/
// surfaces/palette-contract.md § "Presentation and Tokens" — required module
// `src/tokens/`, "generated desktop presentation tokens").
//
// STATUS: STUB. The cross-cutting presentation-token generator
// (`cargo xtask presentation generate`, contract item U3-14 — see
// docs/reports/2026-07-09-pipeline-unification-alignment-audit.md, Workstream
// I) does not exist yet, so there is no generated token artifact anywhere in
// the repo to import (`find . -iname 'axon-tokens*'` → no results). Building
// that generator is out of scope for this slice — it is a shared, cross-app
// deliverable (Palette/Android/Chrome/CLI all consume the same generated
// tokens) that belongs in Workstream F alongside the other schema generators,
// not hand-rolled per app.
//
// In the meantime Palette already renders against the hand-written Aurora
// design tokens wired into Tailwind's `@theme` in `src/styles.css` (imported
// via `src/components/aurora.css`, `--aurora-*` custom properties). This
// module exists so callers have a single, stable import path to switch over
// to once the generator lands, instead of that switch requiring a
// repo-wide grep/replace across every component.
//
// TODO(axon_rust-ruzox.9 / U3-14): once `cargo xtask presentation generate`
// emits a CSS token artifact, import and re-export it here and delete this
// stub. Until then this module intentionally has no exports — importing it
// is a documented no-op, not a functioning token surface.
export {};
