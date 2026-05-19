---
date: 2026-05-07 21:54:03 EST
repo: git@github.com:jmagar/axon.git
branch: bd-work/retrieval-remediation-ug6
head: 438e2a79
agent: Codex
session id: unknown
transcript: unknown
working directory: /home/jmagar/workspace/axon_rust
worktree: /home/jmagar/workspace/axon_rust 438e2a79 [bd-work/retrieval-remediation-ug6]
---

# Retrieval Remediation Session

## User Request

Run `quick-push`, then `lavra-work axon_rust-ug`, fully addressing the retrieval remediation epic and creating/closing the relevant Beads. Later, dispatch the code simplifier agent to review the implemented changes and save the session to markdown.

## Session Overview

- Quick-pushed the existing retrieval MCP contract fixes to `main`.
- Created and pushed branch `bd-work/retrieval-remediation-ug6`.
- Implemented and closed all 11 children under `axon_rust-ug6`, then closed the epic.
- Dispatched the code simplifier agent and integrated its simplification patch.
- Pushed code and Beads/Dolt state.

## Sequence of Events

1. Pushed the initial MCP retrieval fixes on `main` as `56c0fe48 fix(retrieval): address MCP retrieval contract issues`.
2. Created branch `bd-work/retrieval-remediation-ug6` from `main`.
3. Split retrieval remediation across direct retrieve/Qdrant, CLI/docs, cache safety, and query/ask diagnostics work.
4. Implemented tactical remediation in `a6a49ad8 fix(retrieval): complete ug6 remediation`.
5. Implemented the typed query/ask retrieval pipeline refactor in `64a2d670 refactor(retrieval): share query and ask pipeline`.
6. Dispatched code simplifier agent `019e04a1-de48-7f93-9ff2-151f58bf6708` and integrated its patch in `438e2a79 refactor(retrieval): simplify shared pipeline`.
7. Confirmed `bd list --parent axon_rust-ug6 --json` returned `[]` and `bd show axon_rust-ug6 --json` reported the epic closed with 11 of 11 children closed.

## Key Findings

- Direct retrieve previously returned `(usize, String)`, losing matched URL, truncation, warnings, and variant errors across the service boundary.
- `qdrant_retrieve_by_url` used a fixed scroll limit of 256 even for small `max_points`, and silently skipped malformed Qdrant points.
- Query and ask used different dispatch diagnostic paths; query diagnostics depended on `ask_diagnostics`.
- Ask full-document cache config fields existed, but the cache used a process-global default rather than the runtime capacity/TTL.
- `retrieve --limit` conflicted with the global `--limit`; the implemented CLI flag is `retrieve --max-points`.

## Technical Decisions

- Kept `retrieve --max-points` only, because a retrieve-specific `--limit` alias conflicts with the existing global `--limit` in Clap.
- Preserved legacy serialized `RetrieveResult` shape when metadata is absent by using `skip_serializing_if` defaults.
- Used safe Qdrant URL diagnostics that redact userinfo and remove query/fragment.
- Kept ask's existing dual-search batch optimization, but moved shared embedding, dispatch diagnostics, mode metadata, candidate construction, and scoring into shared retrieval helpers.
- Used `prlimit` for the ask cache core-dump guard to satisfy the crate's `-D unsafe-code` policy.

## Files Modified

- `CHANGELOG.md`: added Unreleased notes for retrieval remediation and simplification.
- `Cargo.toml`, `Cargo.lock`: bumped axon version through `1.8.4`.
- `apps/web/package.json`: bumped web package version through `1.6.6`.
- `docs/CONFIG.md`, `docs/SECURITY.md`: documented ask full-document cache config and core-dump guard behavior.
- `docs/commands/ask.md`: documented mode-aware ask relevance threshold behavior for dense/cosine versus hybrid/RRF mode.
- `docs/commands/retrieve.md`: documented `retrieve --max-points` and aligned missing-content error text with CLI behavior.
- `src/cli/commands/mcp.rs`, `src/cli/commands/serve.rs`: enforce ask cache core-dump guard for long-running processes.
- `src/cli/commands/retrieve.rs`: passes configured retrieve max points to the service.
- `src/core/config/cli.rs`, `src/core/config/parse.rs`, `src/core/config/parse/build_config/command_dispatch.rs`, `src/core/config/parse/build_config/config_literal.rs`, `src/core/config/types/config.rs`, `src/core/config/types/config_impls.rs`: parse and store `retrieve --max-points`.
- `src/services/error.rs`: added shared vector dispatch diagnostics with safe Qdrant URL handling.
- `src/services/query.rs`, `src/services/types/service.rs`: preserved typed retrieve metadata and mapping.
- `src/vector/cache.rs`, `src/vector/cache/doc_cache.rs`, `src/vector/cache/tests.rs`: runtime-configured ask doc cache registry and cache safety tests.
- `src/vector/ops/commands/ask/context/build.rs`: uses runtime-configured ask document cache.
- `src/vector/ops/commands/ask/context/retrieval.rs`: uses shared retrieval helpers and simplifier-extracted ask helper functions.
- `src/vector/ops/commands/query.rs`: uses shared retrieval embedding, dispatch, mode metadata, candidate build, and scoring helpers.
- `src/vector/ops/commands/retrieval.rs`: shared typed retrieval helpers.
- `src/vector/ops/qdrant.rs`, `src/vector/ops/qdrant/client.rs`, `src/vector/ops/qdrant/commands/retrieve.rs`, `src/vector/ops/qdrant/types.rs`: typed direct retrieve metadata, bounded canonical-first lookup, malformed point warnings, scroll limit behavior, and tracing fields.
- `src/vector/ops/tei.rs`: adjusted public crate-internal TEI re-exports after shared retrieval helper extraction.
- `tests/mcp_contract_parity.rs`: retrieve result compatibility tests.

## Commands Executed

- `git push origin main`: pushed `56c0fe48` to `main`.
- `git checkout -b bd-work/retrieval-remediation-ug6`: created the remediation branch.
- `bd list --parent axon_rust-ug6 --json`: inspected child Beads.
- `bd close ...`: closed implemented child Beads and then the epic.
- `bd dolt push`: pushed tracker state after Beads updates.
- `git push -u origin bd-work/retrieval-remediation-ug6` and later `git push`: pushed the branch and follow-up commits.
- `cargo fmt --check`, `cargo check --bin axon`, `cargo clippy --all-targets --all-features -- -D warnings`, and focused `cargo test` commands: verified the changes.

## Errors Encountered

- `cargo check` initially waited on Cargo locks from concurrent worker verification. It completed after existing build sessions drained.
- A first attempt at `retrieve --limit` caused a Clap duplicate long-option failure because global `--limit` already exists. The alias was removed and docs/tests were updated to use `--max-points`.
- Pre-commit clippy failed on a manual clamp and default-then-reassign test configs. Fixed by using `.clamp(1, 256)` and struct literals with `..Config::default()`.
- Pre-commit monolith failed when `run_qdrant_dispatch()` reached 122 lines. Split fallback and single-arm dispatch into helper functions.
- Beads auto-export reported `git add failed: exit status 1` several times, but explicit `bd dolt push` completed successfully.

## Behavior Changes (Before/After)

- Before: direct retrieve returned only chunk count and reconstructed content. After: direct retrieve carries requested URL, matched URL, truncation, warnings, and variant errors through services while preserving legacy serialization when metadata is absent.
- Before: direct retrieve tried URL variants via concurrent fanout. After: it uses bounded canonical-first lookup.
- Before: malformed Qdrant points were silently ignored. After: they are counted, logged, and surfaced as warnings where relevant.
- Before: `max_points=1` still scrolled up to 256 points per page. After: scroll limit honors `max_points.min(256).max(1)`.
- Before: query dispatch diagnostics were gated by `ask_diagnostics`. After: query and ask share safe vector dispatch diagnostics.
- Before: ask full-document cache used default singleton settings. After: it uses runtime cache capacity/TTL and enforces `RLIMIT_CORE=0` via `prlimit` for `serve` and `mcp` when enabled.
- Before: query and ask duplicated more retrieval pipeline work. After: shared helpers own embedding, dispatch diagnostics, mode metadata, candidate construction, and scoring policy plumbing.

## Verification Evidence

| command | expected | actual | status |
| --- | --- | --- | --- |
| `cargo fmt --check` | formatting clean | passed | pass |
| `cargo check --bin axon` | binary typechecks | passed on `1.8.4` | pass |
| `cargo clippy --all-targets --all-features -- -D warnings` | no clippy warnings | passed | pass |
| `cargo test --lib retrieve` | retrieve-focused tests pass | 19 passed | pass |
| `cargo test --test mcp_contract_parity retrieve_result` | MCP retrieve contract tests pass | 2 passed | pass |
| `cargo test --lib query_reports_typed_diagnostics_payload_without_ask_diagnostics` | query diagnostics test passes | passed | pass |
| `cargo test --lib vector_dispatch_failure_redacts_qdrant_url_userinfo_and_query` | diagnostics redaction test passes | passed | pass |
| `cargo test --lib ask_doc_cache_uses_runtime_cache_config` | ask cache config test passes | passed | pass |
| `cargo test --lib vector::cache` | cache tests pass | 12 passed | pass |
| `cargo test -q vector::ops::commands::ask::context` | ask context tests pass | 78 passed | pass |
| `cargo test -q vector::ops::commands::query` | query tests pass | 5 passed | pass |
| `cargo test -q vector::ops::commands::retrieval` | retrieval helper tests pass | 3 passed | pass |
| `cargo test -q qdrant_retrieve` | Qdrant retrieve tests pass | 3 passed | pass |
| pre-commit hook during `git commit` | monolith, rustfmt, clippy, tests pass | passed for commits `a6a49ad8`, `64a2d670`, `438e2a79` | pass |
| `bd list --parent axon_rust-ug6 --json` | no open child Beads | `[]` | pass |
| `git status --short --branch` | branch clean and tracking origin | `## bd-work/retrieval-remediation-ug6...origin/bd-work/retrieval-remediation-ug6` | pass |

## Risks and Rollback

- Retrieval ranking behavior is sensitive. The refactor intentionally preserved existing dense/RRF semantics and ask dual-search behavior, with focused tests covering ask context, query, retrieval helpers, and direct retrieve.
- Direct retrieve now surfaces more metadata and warnings. Public JSON compatibility is preserved when metadata is absent.
- Rollback path: revert `438e2a79`, `64a2d670`, and `a6a49ad8` from branch `bd-work/retrieval-remediation-ug6`, or reset the branch back to `56c0fe48` if discarding all epic work.

## Decisions Not Taken

- Did not keep `retrieve --limit` as an alias because it conflicts with global `--limit`.
- Did not leave `axon_rust-ug6.4` open after user correction; implemented and closed the full typed retrieval pipeline refactor.
- Did not change direct retrieve/Qdrant details during the code simplifier pass beyond the prior implementation, because the simplifier found those behavior-sensitive contracts should remain intact.

## References

- Beads: `axon_rust-ug6` and child Beads `axon_rust-ug6.1` through `axon_rust-ug6.11`.
- Related already-closed Beads: `axon_rust-wxe.2`, `axon_rust-wxe.3`, `axon_rust-2j9`, `axon_rust-dvo`.
- Review artifacts referenced by the epic: `.full-review/00-scope.md`, `.full-review/01-quality-architecture.md`, `.full-review/02-security-performance.md`.
- Branch commits: `56c0fe48`, `a6a49ad8`, `64a2d670`, `438e2a79`.

## Open Questions

- No active PR URL was observed from `gh pr view` during this save.
- Transcript path was not observed in the Codex environment.

## Next Steps

- Unfinished work from this session: none for `axon_rust-ug6`; the epic is closed and has no open child Beads.
- Follow-on task not yet started: open or update the GitHub PR for `bd-work/retrieval-remediation-ug6` if one has not already been created.
