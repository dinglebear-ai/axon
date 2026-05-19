# Session overview
- Scope: diagnose/fix embed reliability, clean duplicate project/global hooks and skills, resolve PR comments, and harden monolith enforcement behavior.
- Repo state captured on `fix-crawl` with staged changes across embed worker, hook wiring, docs, and policy files.
- Session save action executed at `2026-02-23` in `/home/jmagar/workspace/axon_rust`.

# Timeline of major activities
- Observed repeated Postgres constraint errors for `axon_crawl_jobs_status_check` and stalled embed behavior in logs (user-provided runtime output).
- Validated environment and runtime status with `axon status`; embeds showed both successful and failed jobs.
- Refactored embed worker logic into `crates/jobs/embed_jobs/worker.rs` and reduced `crates/jobs/embed_jobs.rs` size.
- Migrated monolith enforcement references from repo `scripts/enforce_monoliths.py` to global `~/.claude/hooks/enforce_monoliths.py` in CI/hook/docs wiring.
- Tightened monolith policy behavior with non-code/test/comment/docstring filtering and file-level hook invocation.

# Key findings (with references)
- Embed dedupe now reuses `running` jobs only if `updated_at` is within a freshness window derived from watchdog timeout: `crates/jobs/embed_jobs.rs:106` and `crates/jobs/embed_jobs.rs:113`.
- Worker execution was split into a dedicated module and exported from the parent module: `crates/jobs/embed_jobs.rs:23` and `crates/jobs/embed_jobs/worker.rs:234`.
- Monolith allowlist temporary exception for `crates/jobs/embed_jobs.rs` was removed: `.monolith-allowlist:8`.
- CI and local hook wiring now calls global enforcer path with fallback skip messaging: `.github/workflows/ci.yml:39`, `lefthook.yml:5`, `Justfile:41`.
- Global monolith script now filters by code extensions and excludes tests/comments/docstrings for counts: `/home/jmagar/.claude/hooks/enforce_monoliths.py:46`, `/home/jmagar/.claude/hooks/enforce_monoliths.py:64`, `/home/jmagar/.claude/hooks/enforce_monoliths.py:201`, `/home/jmagar/.claude/hooks/enforce_monoliths.py:313`.

# Technical decisions and rationale
- Kept dedupe for `pending` and only fresh `running` jobs to avoid reusing stale stuck embed jobs while preserving idempotency for active work.
- Parameterized SQL status values in embed job operations to avoid string-interpolated status SQL in hot paths.
- Moved monolith enforcer to global hook location and updated repo references to eliminate duplicate script ownership.
- Added file-targeted PostToolUse monolith checks (global settings) so enforcement runs on edited files rather than only staged diffs.
- Retained `.claude/settings.json` project hooks for s6-structural and rust-toolchain/cargo checks; monolith gate is now global.

# Files modified/created and purpose
- `crates/jobs/embed_jobs.rs`: reduced module size, added freshness-aware dedupe, exported worker module.
- `crates/jobs/embed_jobs/worker.rs`: new worker implementation file (heartbeat/cancel/progress/process loop).
- `crates/jobs/embed_jobs/tests.rs`: added tests for fresh-running dedupe reuse and stale-running new-job creation.
- `.github/workflows/ci.yml`, `lefthook.yml`, `Justfile`, `docs/monolith-policy.md`: repointed monolith enforcement to `~/.claude/hooks/enforce_monoliths.py`.
- `.monolith-allowlist`, `.env.example`: removed temporary allowlist exception; added embed safety env knobs and updated log retention comment.

# Critical commands executed and outcomes
- `git status --short` | confirmed staged modifications/additions/deletions across embed jobs, hook wiring, docs, skills/scripts.
- `python3 ~/.claude/hooks/enforce_monoliths.py --self-test` | output: `self-test passed`.
- `python3 ~/.claude/hooks/enforce_monoliths.py --base HEAD~1 --head HEAD` | output: `Monolith policy check passed.`
- `python3 ~/.claude/hooks/enforce_monoliths.py --file README.md` | output: `Monolith policy check passed.`
- `cargo check -q` | exit code `0` (no stdout).

# Behavior changes (before/after)
- Before: embed dedupe could map to any running duplicate input/config; stale running jobs could be reused.
- After: dedupe accepts `pending` and only `running` jobs with recent `updated_at` window (`make_interval(secs => ...)`).
- Before: monolith enforcement wiring referenced repo `scripts/enforce_monoliths.py`.
- After: CI/precommit/lefthook/docs reference global `~/.claude/hooks/enforce_monoliths.py` with graceful skip when missing.
- Before: temporary allowlist exemption allowed oversized `crates/jobs/embed_jobs.rs`.
- After: exemption removed and file split with dedicated worker module.

# Verification evidence
| command | expected | actual | status |
|---|---|---|---|
| `python3 ~/.claude/hooks/enforce_monoliths.py --self-test` | internal validator tests pass | `self-test passed` | pass |
| `python3 ~/.claude/hooks/enforce_monoliths.py --base HEAD~1 --head HEAD` | changed-range monolith policy passes | `Monolith policy check passed.` | pass |
| `python3 ~/.claude/hooks/enforce_monoliths.py --file README.md` | non-code docs should not trigger violation | `Monolith policy check passed.` | pass |
| `cargo check -q` | Rust workspace compiles | exit code `0` | pass |
| `axon status` | Axon runtime responds with job summary | runtime and job summaries printed; included one failed embed due to TEI 429 and multiple successful embeds | pass |

# Source IDs + collections touched (embed/retrieve)
- Embed command: `axon embed "docs/sessions/2026-02-23-pr-comments-hooks-monolith-session-v2.md" --json` returned `{"job_id":"0f79d638-c95a-48ff-aa6e-cf445276e3ca","status":"pending","source":"rust"}`.
- Embed job status: `axon embed status 0f79d638-c95a-48ff-aa6e-cf445276e3ca --json` returned `status=completed` with `result_json.collection="cortex"`, `result_json.docs_embedded=1`, `result_json.chunks_embedded=1`.
- Source ID used for retrieve (from embed status `input_text`): `docs/sessions/2026-02-23-pr-comments-hooks-monolith-session-v2.md`.
- Retrieve verification: `axon retrieve "docs/sessions/2026-02-23-pr-comments-hooks-monolith-session-v2.md" --collection "cortex"` returned `Chunks: 1` and echoed the same source ID.
- Note: `axon embed --json` output did not include a `data.url` field; source ID was taken from completed embed job `input_text`.

# Risks and rollback
- Risk: global-hook dependency means environments without `~/.claude/hooks/enforce_monoliths.py` will skip enforcement where fallback is configured.
- Risk: moving enforcement outside repo reduces repo-local reproducibility for contributors who do not mirror global hooks.
- Rollback option: restore repo `scripts/enforce_monoliths.py` and revert wiring in `lefthook.yml`, `Justfile`, and `.github/workflows/ci.yml`.
- Rollback option: re-add temporary allowlist entry for `crates/jobs/embed_jobs.rs` if split needs to be backed out.

# Decisions not taken
- Did not reintroduce project-level monolith PreToolUse hook in `.claude/settings.json`.
- Did not keep skill/script duplicates in repo once promoted to global locations.
- Did not mark embed pipeline healthy solely from queue/job state without direct embed/retrieve verification.

# Open questions
- Should CI fail hard when global monolith enforcer is missing instead of printing `[skip]`?
- Should `.env.example` also include `AXON_EMBED_STRICT_PREDELETE` in `.env` defaults for all environments (currently only observed in `.env.example`)?
- Should non-repo global hook/skill files be versioned in a dedicated dotfiles repo for team reproducibility?

# Next steps
- Run mandatory Axon embed/retrieve verification for this session markdown and record source ID/collection in this document.
- If retrieve passes, keep this document as final session artifact and proceed with PR flow.
