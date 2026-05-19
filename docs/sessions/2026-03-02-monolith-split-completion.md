# Session: Monolith Split Completion (Waves 3–5)
**Date:** 2026-03-02
**Working Directory:** `/home/jmagar/workspace/axon_rust`

---

## Session Overview

Picked up where a previous Codex session left off. The prior session had run 18 agents across 3 waves to split 21 monolith files and fix 148 PR review threads — but all changes were uncommitted, and 14 files in the working tree were still over the 500-line limit. This session:

1. Audited the prior session's work via JSONL session logs
2. Identified 7 uncommitted extracted files still over 500 lines
3. Dispatched 3 additional agent teams (waves 3, 4, 5) to complete the work
4. Committed everything: 3 commits, 480 tests passing, **0 allowlist entries** remaining

---

## Timeline

| Time | Activity |
|------|----------|
| Session start | Read prior Codex session JSONL logs to audit what was done |
| Discovery | Found all wave 1–3 work uncommitted; 7 extracted files still over 500 lines |
| Wave 3 | 6 agents split remaining oversized extractions → commit `70b95fa1` |
| Wave 4 | 6 agents split 8 allowlisted files → commit `4b879327` |
| Wave 5 | 5 agents split final 6 allowlisted files → commit `8150d61d` |
| Session end | Allowlist cleared to zero; all teams shut down and cleaned up |

---

## Key Findings

- **All prior work was uncommitted** — the original session's 18 agents left everything in the working tree/index. `git log` showed HEAD hadn't moved.
- **Monolith checker passed at HEAD** because it only checks committed files — it missed the 7 oversized untracked files.
- **Second-pass extractions became new monoliths** — agents in the prior session's wave 3 created extracted files (e.g., `neural-canvas-core.tsx` at 1568 lines) that were themselves over the 500-line limit.
- **PR review truthfulness issue** — prior session resolved all 148 GitHub threads but only fixed 113 in code; 31 were marked resolved without code changes. Noted but not re-opened this session.
- **Pre-commit hooks caught function-level violations** mid-commit in waves 3 and 4, which commit agents fixed before finalizing.

---

## Technical Decisions

- **`config/types.rs` + `config/parse.rs` handled by one agent (Mendel)** — these are tightly coupled; splitting them with separate agents would risk merge conflicts and broken imports.
- **No allowlist additions for test files** — `crates/jobs/common/tests.rs` (563 lines) was split into a `tests/` directory rather than excused, keeping the policy strict.
- **Wrapper-only moves prohibited** — agents were explicitly instructed not to create thin re-export barrels that defer the problem; every split had to extract real logic.
- **Commit agents handled allowlist cleanup** — rather than tracking stale entries manually, commit agents read and updated `.monolith-allowlist` as their first step before staging.

---

## Files Modified

### Wave 3 — Oversized Extractions (commit `70b95fa1`)

| File | Before | After | New Modules |
|------|--------|-------|-------------|
| `apps/web/components/neural-canvas-core.tsx` | 1568 | 373 | `neural-canvas/{types,color-utils,simplex-drift,dendrite,axon,neuron,synapse,particles,anim-state}.ts` |
| `apps/web/components/editor/use-chat-fake-stream-samples.ts` | 1103 | 2 (barrel) | `samples-markdown.ts`, `samples-mdx-basic.ts`, `samples-mdx-advanced.ts`, `samples-mdx-media.ts`, `samples-mdx.ts` |
| `apps/web/components/omnibox/omnibox-component-impl.tsx` | 1066 | 96 | `omnibox-hooks.ts`, `omnibox-effects.ts`, `omnibox-input-bar.tsx`, `omnibox-dropdowns.tsx`, `omnibox-types.ts` |
| `apps/web/app/settings/page-impl-content.tsx` | 853 | 206 | `settings-data.ts`, `settings-components.tsx`, `settings-sections.tsx` |
| `apps/web/components/ui/table-icons-icons-core.tsx` | 685 | 4 (barrel) | `table-icons-border-a.tsx`, `table-icons-border-b.tsx` |
| `apps/web/hooks/use-ws-messages.ts` | 506 | 330 | `ws-messages/handlers.ts` |
| `scripts/qdrant_quality_analysis.py` | 611 | 361 | `qdrant_quality_reporting.py` |

### Wave 4 — Allowlisted Files (commit `4b879327`)

| File | Before | After | New Modules |
|------|--------|-------|-------------|
| `crates/jobs/common/tests.rs` | 563 | directory | `tests/{mod,watchdog,dotenv,db_lifecycle}.rs` |
| `crates/jobs/refresh/processor.rs` | 501 | 379 | `processor_tests.rs` |
| `crates/ingest/reddit.rs` | 562 | directory | `reddit/{mod,types,comments,client}.rs` |
| `crates/cli/commands/screenshot.rs` | 558 | directory | `screenshot/{mod,cdp,util}.rs` |
| `crates/core/config/cli.rs` | 543 | directory | `cli/{mod,global_args}.rs` |
| `crates/core/content.rs` | 521 | 323 | `content/engine.rs` |
| `crates/vector/ops/commands/ask.rs` | 502 | 45 | `ask/{normalize,output,tests}.rs` |
| `apps/web/app/mcp/components.tsx` | 501 | 271 | `mcp-types.ts`, `kv-editor.tsx`, `mcp-server-card.tsx` |

### Wave 5 — Final Allowlisted Files (commit `8150d61d`)

| File | Before | After | New Modules |
|------|--------|-------|-------------|
| `crates/core/config/types.rs` | 1117 | 321 | `types/{enums,config,config_impls}.rs` |
| `crates/core/config/parse.rs` | 879 | 311 | `parse/{docker,helpers,build_config}.rs` |
| `scripts/generate_mcp_schema_doc.py` | 669 | 141 | `mcp_schema_models.py`, `mcp_schema_parser.py`, `mcp_doc_renderer.py` |
| `crates/core/http.rs` | 665 | directory | `http/{error,normalize,ssrf,client,cdp,tests}.rs` |
| `crates/web/download.rs` | 595 | directory | `download/{mod,validation,manifest,archive}.rs` |
| `crates/jobs/ingest.rs` | 578 | directory | `ingest/{types,schema,ops,process,tests}.rs` |

---

## Agent Teams

### Wave 3 Team (`monolith-finish`)
| Agent | Task |
|-------|------|
| Curie | `neural-canvas-core.tsx` |
| Lovelace | `use-chat-fake-stream-samples.ts` |
| Hopper | `omnibox-component-impl.tsx` |
| Meitner | `page-impl-content.tsx` + `table-icons-icons-core.tsx` |
| Franklin | `use-ws-messages.ts` + `qdrant_quality_analysis.py` |
| Volta | Verify + commit |

### Wave 4 Team (`monolith-wave4`)
| Agent | Task |
|-------|------|
| Darwin | `jobs/common/tests.rs` + `jobs/refresh/processor.rs` |
| Tesla | `ingest/reddit.rs` |
| Faraday | `cli/commands/screenshot.rs` |
| Planck | `core/config/cli.rs` + `core/content.rs` |
| Boyle | `vector/ops/commands/ask.rs` |
| Herschel | `apps/web/app/mcp/components.tsx` |
| Maxwell | Verify + commit |

### Wave 5 Team (`monolith-wave5`)
| Agent | Task |
|-------|------|
| Mendel | `config/types.rs` + `config/parse.rs` (coupled pair) |
| Lavoisier | `scripts/generate_mcp_schema_doc.py` |
| Celsius | `core/http.rs` |
| Ampere | `web/download.rs` |
| Ohm | `jobs/ingest.rs` |
| Watt | Verify + commit |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check` (wave 3) | 0 errors | 0 errors | ✅ |
| Pre-commit hooks (wave 3) | All pass | All pass (2 inline fixes by Volta) | ✅ |
| `cargo check` (wave 4) | 0 errors | 0 errors | ✅ |
| 480 tests (wave 4) | All pass | All pass | ✅ |
| Monolith policy (wave 4) | 0 violations | 0 violations, 3 warnings | ✅ |
| `cargo check` (wave 5) | 0 errors | 0 errors (2 unused import fixes by Watt) | ✅ |
| 480 tests (wave 5) | All pass | All pass | ✅ |
| `.monolith-allowlist` entries | 0 | 0 | ✅ |
| `pnpm exec tsc --noEmit` | 0 new errors | 0 new errors (pre-existing omnibox errors only) | ✅ |

---

## Behavior Changes (Before/After)

- **Before:** 21+ files over 500 lines, all uncommitted, sitting in working tree
- **After:** All files under 500 lines, 3 clean commits on `main`, allowlist empty
- **Module structure:** Many flat files are now directory modules with focused submodules — import paths unchanged via re-exports
- **Monolith policy:** Passes clean at HEAD with 0 violations; function-level warnings only (under hard limit)

---

## Source IDs + Collections Touched

*Axon embedding attempted post-session — see embed status below.*

---

## Risks and Rollback

- **Risk:** Re-export barrels could drift if submodule APIs change — callers import from the original path but the logic is elsewhere
- **Risk:** Some agents used `#[path]` attributes for test files (Darwin) — non-standard pattern that could confuse new contributors
- **Rollback:** `git revert 8150d61d 4b879327 70b95fa1` reverts all 3 waves cleanly; no DB or infra changes

---

## Decisions Not Taken

- **Leaving test files at 563 lines** — `crates/jobs/common/tests.rs` was noted as "acceptable?" in the allowlist; decided to split it anyway to keep policy strict
- **Reopening 31 deferred PR threads** — the prior session resolved GitHub threads without code changes; not re-opened this session (future work)
- **Splitting `config/types.rs` and `config/parse.rs` with separate agents** — rejected due to tight coupling; one agent (Mendel) handled both to avoid cross-agent conflicts

---

## Open Questions

- Were the 31 deferred PR review threads actually non-actionable, or do they need real fixes?
- `vitest` binary was missing from the workspace — frontend unit tests did not run; should be verified separately
- Biome had a pre-existing nested root config conflict — unrelated to this work but should be investigated

---

## Next Steps

- Run `pnpm vitest` to verify frontend tests pass (binary was unavailable during agent runs)
- Review the 31 PR threads that were resolved without code changes and decide which need fixes
- Consider addressing remaining large files not in allowlist (e.g., `crates/core/config/types/config.rs` at 383 lines is fine, but watch for growth)
