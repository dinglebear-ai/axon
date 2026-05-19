---
date: 2026-05-09 22:50:21 EST
repo: git@github.com:jmagar/axon.git
branch: main
head: 17467f51
agent: Codex
session id: 019e0ee9-08e9-70e0-a1a9-89737d55c56c
transcript: /home/jmagar/.codex/sessions/2026/05/09/rollout-2026-05-09T18-43-33-019e0ee9-08e9-70e0-a1a9-89737d55c56c.jsonl
working directory: /home/jmagar/workspace/axon_rust
worktree: /home/jmagar/workspace/axon_rust  17467f51 [main]
---

# Session Log: CLAUDE.md Cleanup and Env Accuracy

## User Request

The session began with `$claude-md-management:claude-md-improver`, followed by direction to correct earlier auth assumptions because OAuth exists, make all CLAUDE.md changes, ignore a later docs-generation tangent, then verify `.env.example` against code and update the real env files too.

## Session Overview

- Audited and revised Axon guidance/docs to remove stale `crates/...` paths, old full-mode/runtime language, and inaccurate MCP auth guidance.
- Verified the current MCP auth model from code before documenting it: bearer-only, OAuth, and dual static bearer plus OAuth JWT behavior.
- Audited `.env.example` against actual env readers in source, compose, and setup assets.
- Updated `.env.example`, repo `.env`, and `/home/jmagar/.axon/.env` so the real env files include all current template keys while preserving existing secret values.
- Saved this session note under `docs/sessions/`.

## Sequence of Events

- Used the `claude-md-management:claude-md-improver` workflow to audit CLAUDE.md and related guidance files before editing.
- Rechecked MCP OAuth/auth source after the user corrected the earlier static-token-only assumption.
- Updated many repository guidance and documentation files to align with the current `src/` layout, lite-only runtime, services-first contract, and MCP auth behavior.
- Per user request, ignored the docs-generation-opportunities tangent and continued finishing the CLAUDE.md/doc updates.
- Per user request, audited `.env.example` against env readers in Rust source and Docker Compose, patched it, then updated both real env files.
- Per user request, ran `vibin:save-to-md` and wrote this markdown session capture.

## Key Findings

- MCP auth is not static-token-only: `src/mcp/auth.rs:6`, `src/mcp/auth.rs:8`, and `src/mcp/auth.rs:227` show bearer-only mode, OAuth mode, and static bearer remaining enabled with OAuth.
- OAuth startup reads `AXON_MCP_AUTH_MODE`, `AXON_MCP_PUBLIC_URL`, and Google/admin OAuth env vars in `src/mcp/auth.rs:169`, `src/mcp/auth.rs:181`, and nearby setup lines.
- `AXON_MCP_TRANSPORT`, `AXON_LITE`, and `AXON_SQLITE_PATH` are real config inputs in `src/core/config/parse/helpers.rs:5`, `src/core/config/parse/build_config.rs:72`, and `src/core/config/parse/build_config.rs:78`.
- Suggest tuning env vars are real runtime inputs in `src/vector/ops/commands/suggest.rs:162`, `src/vector/ops/commands/suggest.rs:164`, and `src/vector/ops/commands/suggest.rs:166`.
- Embed/Qdrant tuning vars are real runtime inputs in `src/vector/ops/tei/tei_client.rs:97`, `src/vector/ops/tei/pipeline.rs:324`, `src/vector/ops/tei/pipeline.rs:332`, and `src/vector/ops/tei/qdrant_store.rs:397`.
- Real env additions were appended at `.env:127` and `/home/jmagar/.axon/.env:127` without writing secret values into this note.

## Technical Decisions

- Preserved existing secret values and local-only env settings instead of replacing real env files from `.env.example`.
- Added missing current-runtime keys to the real envs as a separate appended section to minimize risk and make the update auditable.
- Did not delete older ACP/web/queue env values during this pass because the request was to bring env files up to date, not perform a breaking cleanup.
- Kept `.env.example` aligned with code-backed env readers and avoided test-only or stale variables such as `AXON_TEST_*`, legacy PG/Redis/AMQP variables, and unused Neo4j helper env.
- Used `git diff --check` and key inventory comparisons rather than a full test suite because only docs/env files were edited in this specific env pass.

## Files Modified

- `.env.example` - expanded and corrected the canonical environment template.
- `.env` - added missing current-runtime keys while preserving existing local values and secrets.
- `/home/jmagar/.axon/.env` - added the same missing current-runtime keys to the canonical user env while preserving existing local values and secrets.
- `CLAUDE.md`, `src/*/CLAUDE.md`, `docs/CLAUDE.md`, and `docs/mcp/CLAUDE.md` - updated repo guidance during the earlier CLAUDE.md maintenance pass.
- `README.md`, `docs/**/*.md`, `plugins/skills/*/SKILL.md`, `src/**/README.md`, and related guidance files - updated during the broader stale-doc cleanup already present in the dirty tree.
- `docs/commands/setup.md` - new untracked docs file present in the dirty tree.
- `docs/sessions/2026-05-09-claude-md-env-accuracy.md` - this session note.

## Commands Executed

- `rg` and `sed` over source/docs to find stale paths, auth references, and env readers.
- `git status --short` to inventory the dirty tree.
- `git diff --check` and `git diff --check -- .env .env.example` to verify whitespace/diff hygiene.
- `comm` plus `rg` to compare keys in `.env.example` against `.env` and `/home/jmagar/.axon/.env`.
- `rg -o ... | sort | uniq -d` to verify no duplicate keys in either real env file.
- `gh pr view --json number,title,url` failed due network/API connectivity, so no active PR metadata was recorded.

## Errors Encountered

- `gh pr view --json number,title,url` failed with `error connecting to api.github.com`; this only affected PR metadata collection for the session note.
- The first auth summary earlier in the session under-described OAuth. The user corrected that, and the follow-up pass rechecked `src/mcp/auth.rs` before updating docs/env language.

## Behavior Changes (Before/After)

| Area | Before | After |
| --- | --- | --- |
| `.env.example` | Missing several current runtime knobs and had some stale comments. | Includes code-backed MCP, logging, suggest, job, TEI, Qdrant, Gemini, and Docker TEI variables. |
| Real env files | Missing many keys now present in `.env.example`. | `.env` and `/home/jmagar/.axon/.env` include the full template key set with existing values preserved. |
| MCP auth docs/env language | Risked implying static bearer only. | Reflects bearer-only mode, OAuth mode, and static bearer compatibility during OAuth mode. |

## Verification Evidence

| command | expected | actual | status |
| --- | --- | --- | --- |
| `git diff --check -- .env.example` | no whitespace errors | no output | pass |
| `git diff --check -- .env .env.example` | no whitespace errors | no output | pass |
| `comm -23 <template keys> <repo .env keys>` | no missing template keys | no output | pass |
| `comm -23 <template keys> </home/jmagar/.axon/.env keys>` | no missing template keys | no output | pass |
| `rg -o "^[A-Za-z_][A-Za-z0-9_]*" .env \| sort \| uniq -d` | no duplicate keys | no output | pass |
| `rg -o "^[A-Za-z_][A-Za-z0-9_]*" /home/jmagar/.axon/.env \| sort \| uniq -d` | no duplicate keys | no output | pass |

## Risks and Rollback

- Risk: appended env defaults could influence local runtime behavior if previously unset. Rollback path: remove the appended `Current Axon runtime additions` section from `.env` and `/home/jmagar/.axon/.env`.
- Risk: repo docs are broadly dirty from the CLAUDE.md/doc cleanup. Rollback path: review `git diff` by file and selectively restore only unwanted doc edits; do not use a broad reset because unrelated local work may be present.
- Risk: real env files contain secrets and were intentionally not copied into this note. Future review should inspect them locally rather than through chat/log output.

## Decisions Not Taken

- Did not remove local-only or legacy env variables from `.env` or `/home/jmagar/.axon/.env`; deletion requires a separate runtime cleanup decision.
- Did not run the Rust test suite because the completed pass edited docs/env files, not Rust behavior.
- Did not force-add ignored env files or session notes; staging/commit was not requested in this turn.

## Open Questions

- Whether older ACP/web/queue env variables in the real env files should be removed or migrated is unresolved.
- Active PR metadata was unavailable because the GitHub CLI request could not reach `api.github.com`.
- The full breadth of earlier doc changes should still be reviewed before commit because many files were already dirty.

## Next Steps

- Unfinished from this session: review the broad dirty docs diff before committing.
- Follow-on: decide whether to clean stale local-only env variables from `.env` and `/home/jmagar/.axon/.env`.
- Follow-on: run the appropriate docs/lint checks before a final push if these docs are meant to land together.
