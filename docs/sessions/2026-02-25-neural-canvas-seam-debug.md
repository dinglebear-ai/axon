# Session Log — 2026-02-25 — Neural Canvas Seam Debug

## 1. Session overview
- Goal: systematically debug red/seam visual artifacts in the Next.js frontend.
- Scope executed: reproduce artifact, isolate rendering layer, patch canvas compositor behavior.
- Result: seam artifact removed in local verification after patching `neural-canvas.tsx`.

## 2. Timeline of major activities
- User reported visible red graphical glitches and a horizontal seam (“two hemispheres”) below the omnibox.
- Reproduced the issue via browser tooling and screenshots from `http://dookie:3000/`.
- Isolated layer behavior by toggling canvas/main visibility and sampling layout metrics.
- Applied fix to force opaque canvas base fill in frame and cached background layer.
- Reloaded and verified seam was no longer visible.

## 3. Key findings with path:line references when relevant
- Canvas viewport matched window size during reproduction (`w=780`, `h=503`, `innerW=780`, `innerH=503`), but document/body height was larger (`721`), confirming underlying page area existed below the viewport in the captured state.
- Main results container can remain mounted with hidden/collapsed states (`max-h-0` + `opacity-0`), which can influence document flow and expose base page background when canvas content is partially transparent: `/home/jmagar/workspace/axon_rust/apps/web/components/results-panel.tsx:124`.
- Canvas render loop cleared the frame but did not paint a guaranteed opaque base before compositing background layers: `/home/jmagar/workspace/axon_rust/apps/web/components/neural-canvas.tsx:1390`.
- Background layer compositing used density + ambient gradient pass without an explicit opaque base fill beforehand: `/home/jmagar/workspace/axon_rust/apps/web/components/neural-canvas.tsx:1402`.
- Global body background includes pink/blue radial gradients, which can bleed through transparent canvas regions: `/home/jmagar/workspace/axon_rust/apps/web/app/globals.css:192`.

## 4. Technical decisions and rationale
- Decision: patch canvas compositor instead of changing global page gradient.
- Rationale: artifact appeared as a rendering seam tied to transparency/layer composition; fixing at canvas layer removes seam while preserving existing brand background styling.
- Decision: add opaque fill in both active frame buffer and cached background buffer.
- Rationale: prevents any transparency gap during frame clear/refresh cadence and cached layer redraw cycles.

## 5. Files modified/created and purpose
- Modified: `/home/jmagar/workspace/axon_rust/apps/web/components/neural-canvas.tsx`
- Purpose: prevent body/background bleed-through by enforcing opaque base fill in render loop and background cache rendering.
- Created: `/home/jmagar/workspace/axon_rust/docs/sessions/2026-02-25-neural-canvas-seam-debug.md`
- Purpose: structured session record with verification and outcomes.

## 6. Critical commands executed and outcomes
- `git log --oneline -- apps/web/components/neural-canvas.tsx | head -n 8` | listed recent edits touching neural canvas.
- Browser script probe (`evaluate_script`) | confirmed canvas/window dimensions and larger body/doc height in repro state.
- Browser screenshots (`take_screenshot`) | captured before/after and isolation states (`/tmp/axon-live.png`, `/tmp/no-canvas.png`, `/tmp/canvas-only.png`, `/tmp/after-fix.png`).
- `git status --short -- apps/web/components/neural-canvas.tsx` | confirmed modified file: `M apps/web/components/neural-canvas.tsx`.

## 7. Behavior changes (before/after)
- Before: visible horizontal seam below omnibox and red/pink banding consistent with layered bleed-through.
- After: seam not visible in post-patch viewport verification screenshot (`/tmp/after-fix.png`).

## 8. Verification evidence (`command | expected | actual | status`)
- `evaluate_script (canvas metrics)` | canvas should map viewport; investigate overflow clues | `w=780,h=503,innerW=780,innerH=503,bodyH=721,docH=721` | PASS (evidence captured)
- `take_screenshot /tmp/no-canvas.png` | should reveal underlying body gradient | visible banded gradient without neural layer | PASS
- `take_screenshot /tmp/canvas-only.png` | canvas-only should show neural texture without UI influence | full-field neural texture shown | PASS
- Reload + `take_screenshot /tmp/after-fix.png` | seam should be absent | seam not observed | PASS

## 9. Source IDs + collections touched (embed/retrieve source IDs, collections, outcomes)
- Preflight: `axon status` executed successfully; system reported mixed historical job states.
- Embed attempt 1: `axon embed "docs/sessions/2026-02-25-neural-canvas-seam-debug.md" --json` returned async payload (`job_id=ae00e1f8-d0da-4ae9-83ac-fae5e691d230`, `status=pending`, `source=rust`).
- Embed attempt 2 (synchronous): `axon embed "docs/sessions/2026-02-25-neural-canvas-seam-debug.md" --wait true --json` returned `chunks_embedded=3`, `collection=cortex`.
- Output limitation: embed JSON in this run did not expose `data.url`; source ID field required by workflow was not present.
- Retrieve verification attempt: `axon retrieve "docs/sessions/2026-02-25-neural-canvas-seam-debug.md" --collection "cortex"` succeeded with `Chunks: 3` and returned session content.

## 10. Risks and rollback
- Risk: opaque base fill may slightly reduce perceived depth from body gradient bleed-through.
- Mitigation: ambient/density layers remain active; only unintended transparency path is removed.
- Rollback: revert the two inserted fill operations in `neural-canvas.tsx`.

## 11. Decisions not taken
- Did not change `body` gradient definitions in `globals.css`.
- Did not remove/collapse results panel mount strategy.
- Did not alter neural preset palette values in this fix.

## 12. Open questions
- Does the seam reproduce on other viewport sizes/devices after this patch?
- Should document height behavior be tightened when results panel is collapsed to reduce layout variability?

## 13. Next steps
- If user confirms edge cases still exist, add targeted resize/viewport stress test coverage around canvas compositing.
- Optionally add an automated visual regression check around seam-free continuous canvas rendering.
