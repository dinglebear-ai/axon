# Session Log — 2026-02-26 — Ask Retrieval Hardening

## 1. Session overview
- Investigated incorrect `axon ask` answers and validated that non-grounded fallback behavior existed.
- Hardened `ask` retrieval and context injection to reduce irrelevant sources and weak matches.
- Corrected an accidentally named session file, re-embedded it, and removed the bad Qdrant source entry.
- Updated `ask` system prompt to disallow uncited training-knowledge answers.

## 2. Timeline of major activities
- Confirmed full-doc retrieval path and runtime diagnostics showed `full_docs_selected=4` in a live run.
- Found and fixed accidental file name `and then update any and all other relevant docs`; moved to `docs/sessions/2026-02-26-context-injection-cleanup.md`.
- Re-embedded moved file and deleted bad source URL from Qdrant (`operation_id=21964`, status `completed`).
- Added supplemental gating + low-signal filtering + topical-overlap checks in `ask` retrieval code.
- Rebuilt binaries and re-ran failing query with diagnostics to verify retrieval pool narrowing.

## 3. Key findings with path:line references when relevant
- `ask` previously allowed uncited fallback answers from training knowledge via system prompt branch in [`crates/vector/ops/commands/streaming.rs:7`](/home/jmagar/workspace/axon_rust/crates/vector/ops/commands/streaming.rs:7).
- `ask` retrieval now includes low-signal source filtering in [`crates/vector/ops/commands/ask/context.rs:62`](/home/jmagar/workspace/axon_rust/crates/vector/ops/commands/ask/context.rs:62) and application point [`context.rs:180`](/home/jmagar/workspace/axon_rust/crates/vector/ops/commands/ask/context.rs:180).
- Topical-overlap gate is enforced in [`context.rs:89`](/home/jmagar/workspace/axon_rust/crates/vector/ops/commands/ask/context.rs:89) and used in rerank filtering at [`context.rs:201`](/home/jmagar/workspace/axon_rust/crates/vector/ops/commands/ask/context.rs:201).
- Supplemental context now requires tighter score threshold (`+0.05`) via [`context.rs:32`](/home/jmagar/workspace/axon_rust/crates/vector/ops/commands/ask/context.rs:32) and [`context.rs:274`](/home/jmagar/workspace/axon_rust/crates/vector/ops/commands/ask/context.rs:274).
- Top chunk diversity tightened to max 1 per URL in [`context.rs:211`](/home/jmagar/workspace/axon_rust/crates/vector/ops/commands/ask/context.rs:211).

## 4. Technical decisions and rationale
- Enforced source-grounded output for `ask` to prevent uncited fallback content.
- Added low-signal source filtering (`docs/sessions`, `.cache`, logs) to reduce retrieval contamination from session artifacts.
- Preserved opt-in access to low-signal sources when query explicitly requests session/log/history content.
- Added topical-overlap gating to reduce keyword-adjacent but non-answering candidates.
- Kept supplemental context optional and budget-aware to avoid bloated prompts when coverage is already strong.

## 5. Files modified/created and purpose
- [`crates/vector/ops/commands/ask/context.rs`](/home/jmagar/workspace/axon_rust/crates/vector/ops/commands/ask/context.rs): retrieval gates (low-signal filter, topical overlap, supplemental threshold, diversity) + tests.
- [`crates/vector/ops/commands/streaming.rs`](/home/jmagar/workspace/axon_rust/crates/vector/ops/commands/streaming.rs): hardened `ASK_RAG_SYSTEM_PROMPT` to disallow uncited fallback answers.
- [`docs/sessions/2026-02-26-context-injection-cleanup.md`](/home/jmagar/workspace/axon_rust/docs/sessions/2026-02-26-context-injection-cleanup.md): corrected stale references from accidental filename incident.
- [`docs/sessions/2026-02-26-ask-retrieval-hardening.md`](/home/jmagar/workspace/axon_rust/docs/sessions/2026-02-26-ask-retrieval-hardening.md): this session log.

## 6. Critical commands executed and outcomes
- `timeout 45s ./scripts/axon ask --query "What is Axon ask?" --limit 3 --diagnostics --json` -> showed `full_docs_selected=4`, `chunks_selected=10`, `supplemental_selected=3` (pre-tightening baseline).
- `./scripts/axon doctor --json` -> `all_ok: true`; services reported healthy.
- `./scripts/axon embed "docs/sessions/2026-02-26-context-injection-cleanup.md" --wait true --json` -> success (`chunks_embedded=5`, `collection=cortex`).
- `curl ... /collections/cortex/points/delete?wait=true` with URL filter for bad path -> success (`operation_id=21964`, `status=completed`).
- `timeout 45s ./scripts/axon ask "how do you create claude code custom slash commands?" --diagnostics --json` -> post-tightening run: `candidate_pool=47`, `reranked_pool=8`, `full_docs_selected=4`, `supplemental_selected=0`.
- `cargo test -q supplemental_ -- --nocapture` and `cargo test -q low_signal_ -- --nocapture` and `cargo test -q topical_overlap_ -- --nocapture` -> tests passed.
- `cargo build --bin axon --bin axon-mcp` -> build passed.

## 7. Behavior changes (before/after)
- Before: `ask` prompt explicitly permitted a `## Answer (from training knowledge)` fallback.
- After: `ask` prompt disallows uncited fallback answers and requires source-grounded response behavior.
- Before: retrieval could include session/cache/log sources in normal queries.
- After: low-signal sources are filtered unless query explicitly requests session/log/history data.
- Before: supplemental chunks could include weaker candidates if budget allowed.
- After: supplemental chunks require stricter score and only run under budget/coverage gates.
- Before: top chunks allowed up to 2 entries per URL.
- After: top chunks allow max 1 entry per URL for stronger diversity.

## 8. Verification evidence (`command | expected | actual | status`)
| command | expected | actual | status |
|---|---|---|---|
| `./scripts/axon doctor --json` | Services reachable | `all_ok: true` | ✅ |
| `cargo check -q` | Compile succeeds | No compile errors | ✅ |
| `cargo test -q supplemental_ -- --nocapture` | Supplemental tests pass | `4 passed; 0 failed` | ✅ |
| `cargo test -q low_signal_ -- --nocapture` | Low-signal tests pass | `2 passed; 0 failed` | ✅ |
| `cargo test -q topical_overlap_ -- --nocapture` | Topical-overlap tests pass | `1 passed; 0 failed` | ✅ |
| `cargo build --bin axon --bin axon-mcp` | Binaries build | Finished dev profile successfully | ✅ |
| `timeout 45s ./scripts/axon ask "how do you create claude code custom slash commands?" --diagnostics --json` | Narrower, grounded retrieval set | `reranked_pool=8`, `supplemental_selected=0` | ✅ |

## 9. Source IDs + collections touched (embed/retrieve source IDs, collections, outcomes)
- Prior bad source entry observed in index: source ID `/home/jmagar/workspace/axon_rust/and then update any and all other relevant docs`, collection `cortex`, async embed job `f856a1da-f2eb-476f-9c64-d29a80e9f7bc`.
- Deletion of bad source URL in Qdrant executed via filter delete: `operation_id=21964`, `status=completed`.
- Corrected file embed run observed: `docs/sessions/2026-02-26-context-injection-cleanup.md` with `chunks_embedded=5`, collection `cortex`.
- Current session embed command: `./scripts/axon embed "docs/sessions/2026-02-26-ask-retrieval-hardening.md" --json` -> async response `{job_id:"52931766-eed5-48a0-bdbb-ab0cecb53be5", status:"pending"}`.
- Embed job status (`axon embed status 52931766-eed5-48a0-bdbb-ab0cecb53be5 --json`): `status=completed`, source ID `docs/sessions/2026-02-26-ask-retrieval-hardening.md`, collection `cortex`, `chunks_embedded=1`.
- Retrieve verification (`axon retrieve "docs/sessions/2026-02-26-ask-retrieval-hardening.md" --collection "cortex"`): success (`Chunks: 1`).

## 10. Risks and rollback
- Risk: stricter topical-overlap may filter useful but sparsely token-overlapping docs in edge cases.
- Risk: low-signal source filtering can hide relevant session docs unless query requests them explicitly.
- Rollback: revert `ask/context.rs` and `streaming.rs` changes, then rebuild (`cargo build --bin axon --bin axon-mcp`).
- Rollback for bad-source cleanup: deletion already applied to Qdrant; re-embed old source path is possible if needed.

## 11. Decisions not taken
- Did not implement BM25 retrieval integration.
- Did not implement RRF fusion across separate dense/lexical retrievers.
- Did not add domain allowlist override for deprecated-command queries in this session.
- Did not implement citation deduplication formatting cleanup.

## 12. Open questions
- Should `ask` support query-time domain allowlists for known-authoritative docs (for example `docs.claude.com`) in fringe/deprecated topics?
- Should topical-overlap thresholds be configurable via env vars?
- Should low-signal filters be exposed as config toggles per command mode?
- `git status --short` returned empty during report capture; uncertain whether changes were already staged/committed externally.

## 13. Next steps
- Test `ask` on additional known-failing queries and tune overlap thresholds with diagnostics.
- Add optional authoritative-domain boost/allowlist for targeted query classes.
- Add citation source dedup in final answer rendering.
- If needed, add BM25 + RRF path and benchmark against current dense+rerank pipeline.
