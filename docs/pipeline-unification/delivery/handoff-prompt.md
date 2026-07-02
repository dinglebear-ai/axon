# Handoff Prompt — Finish the Pipeline-Unification Refactor (#298)

**Status: Phase 9 (Port Source Families) is COMPLETE.** All 8 source families
(local, web, git, feed, youtube, reddit, sessions, registry) have adapters +
services bridges merged and verified on main. Remaining: **P10 surface cutover,
P11 reset/prune, P12 release.**

Copy the block below into a fresh session to continue. The opening "Use a
workflow" authorizes the Workflow tool; prepend `ultracode` or attach a `/goal`
to run continuously.

---

```
Use a workflow to finish the Axon pipeline-unification refactor (GitHub issue #298), Phases 10-12. I'm continuing from codex (out of usage). Read issue #298 and treat docs/pipeline-unification/ as source of truth — especially delivery/cutover-contract.md, delivery/surface-removal-contract.md, delivery/implementation-plan.md (§Phase 10-12), and delivery/testing-contract.md.

## Current verified state (git fetch first, confirm main SHA)
- PHASE 9 DONE: all 8 source families have adapters + services bridges merged on main. Adapters in crates/axon-adapters/src/{local,web,git,feed,youtube,reddit,sessions,registry_sources}.rs; bridges in crates/axon-services/src/{local,web,git,feed,youtube,reddit,registry,sessions}_source.rs (each: resolve→lease→discover→diff→generation→vectorize→publish, mirroring local_source; #[cfg(test)] integration tests over fake LedgerStore/EmbeddingProvider/VectorStore). The vector-payload contract (crates/axon-vectors/src/payload.rs + payload_families.rs) recognizes families code/web/package/session/graph/memory/feed/social/media + their fields.
- Bridges are #[allow(dead_code)] pub(crate) — NOTHING in production (CLI/MCP/REST) calls them yet. That wiring IS Phase 10.
- PRs this era: #319 web, #320 five adapters, #321 git adapter, #322 six bridges. Bead axon_rust-8s8uq (closed). Live tracker: https://axon.tootie.tv/pipeline-unification.

## Phase 10 — Surface Cutover (the big one)
Goal: hard-break public surfaces so `axon <source>` is the one entrypoint calling the new bridges.
- Implement `axon <source>` / `axon watch <source>` / `axon watch exec <source>` (CLI in crates/axon-cli) dispatching to the new *_source bridges via a resolver/router (crates/axon-route SourceResolver/SourceRouter/AdapterRegistry — wire the family adapters into the registry; note registry_sources adapter isn't registered with with_options for its dump path yet — see reddit bridge's note).
- The bridges currently take prepared inputs (repo_root/feed_path/*_dump_path/sessions_root). Phase 10 must add the ACQUISITION step that fetches/clones/downloads to produce those (git clone, feed fetch, reddit/youtube API, etc.) — port from crates/axon-ingest legacy, OR wire as a pre-bridge acquisition stage. Decide where clone/fetch lives.
- Keep `extract` (structured LLM extraction) and `map` first-class.
- REMOVE from CLI/MCP/REST/web/Palette/Android/Chrome: old embed, ingest, scrape, crawl, code-search-watch, purge, and legacy MCP action families. NO compatibility aliases (clean break). Update generated CLI help, MCP tool schema, REST OpenAPI, and app client contracts.
- Proof (delivery/surface-removal-contract.md + cutover-contract.md): removed surfaces absent from generated schemas + help; new surfaces map to shared DTOs; no aliases remain.

## Phase 11 — Reset/Prune/Empty-DB cutover
Reset plan/exec with receipts; prune plans + cleanup-debt execution; old stores block unified workers until reset; fresh SQLite schema + Qdrant payload/index shape. Proof: Tier-5 cutover tests pass; fresh reindex from empty DB is the supported path.

## Phase 12 — Release readiness
Prove the refactor is merge-complete per delivery/implementation-plan.md §Phase 12.

## HOW to parallelize (PROVEN this era — use it)
Fan out the Workflow tool's agent() primitive (or parallel Agent calls with isolation:"worktree") for independent units — it delivered all 8 adapters + 6 bridges here. LESSONS: (1) these implementations take 10-15 MINUTES each (250-320k tokens, 65-170 tool_uses) — be patient, wait for the PARENT's real completion report, ignore early self-delegating child no-ops. (2) ALWAYS verify files landed on disk (wc -l / run the tests) before trusting a "done" — reports occasionally lie. (3) Worktree agents may each edit the same shared file (e.g. payload.rs) in their own copy → these conflict; reconcile into ONE coherent change at integration. (4) Agent worktrees are EPHEMERAL — copy their output to a real branch + commit BEFORE the session ends or the work is lost.

## Guardrails (enforced by CI)
- VERIFY IN CODE — never mark done from a claim; grep the real symbol + RUN tests. Core discipline of #298.
- No mod.rs; sidecar *_tests.rs with #[path]; files ≤500 lines / fns ≤120 (monolith, CI-enforced); log_info/log_warn not println.
- Per PR: cargo test green for touched crates, cargo fmt, `cargo run -p xtask --no-default-features -- schemas generate` (regenerate + commit schemas when you change DTOs/enums/payload families — schema-contract-sync CI gate enforces it), cargo xtask check. Branch off main; commit -c core.hooksPath=/dev/null; push --no-verify (pre-push = slow full build); PR; wait for ci-gate pass (UNSTABLE-only-CodeRabbit is fine); gh pr merge <n> --squash --delete-branch. main protected — no --admin.
- GitGuardian: don't put literal secret-shaped strings in fixtures — assemble at runtime via format!().

## Cadence
One merged, verified PR at a time. After each merge, run tests on merged main to confirm, update issue #298 checkboxes (only what you verified in code), update the tracker artifact. Prioritize correctness over speed. Phase 10 is large — split it (CLI dispatch + acquisition wiring; then per-surface removal MCP/REST/web/apps).
```
