# 2026-05-03 Alternate Ingest Full Review Remediation

## Repo Snapshot

- Repo: `/home/jmagar/workspace/axon_rust`
- Branch: `obs/p0-tracing-bundle`
- HEAD: `ab5c12a8fc8efc0f885873aeca30e624cffcc5f0`
- Scope: alternate ingest methods only: GitHub, YouTube, Reddit.
- Worktree: shared and dirty before this work started. Do not assume every modified file belongs to this session.

## User Request

The session started with:

```text
comprehensive:full-review
scope to our alternate ingest method - github, youtube, reddit
```

The user then asked to continue, create Beads for all review issues, and run `lavra-work em all`.

## Review Artifacts

Generated comprehensive review artifacts under `.full-review/`:

- `.full-review/00-scope.md`
- `.full-review/01-quality-architecture.md`
- `.full-review/02-security-performance.md`
- `.full-review/03-testing-documentation.md`
- `.full-review/04-best-practices.md`
- `.full-review/05-final-report.md`

The final report found 20 issues: no critical findings, 4 high priority findings, 11 medium priority findings, and 5 low priority findings.

## Beads Created And Closed

Created epic:

- `axon_rust-qfc` - `[EPIC] Alternate ingest full-review remediation (2026-05-03)`

Created and later closed all 20 child Beads:

- `axon_rust-qfc.1` - Implement or reject YouTube playlist and channel ingest targets
- `axon_rust-qfc.2` - Persist async ingest progress into result_json
- `axon_rust-qfc.3` - Skip unauthenticated GitHub clone retry for private/auth failures
- `axon_rust-qfc.4` - Add regression coverage for accepted YouTube playlist/channel targets
- `axon_rust-qfc.5` - Normalize and validate MCP ingest targets like CLI targets
- `axon_rust-qfc.6` - Canonicalize and validate Reddit thread targets before HTTP construction
- `axon_rust-qfc.7` - Honor Reddit Retry-After on 429 responses
- `axon_rust-qfc.8` - Add cancellation checks to Reddit source ingest
- `axon_rust-qfc.9` - Surface GitHub embed batch failures instead of silently succeeding partial ingests
- `axon_rust-qfc.10` - Add async ingest progress persistence tests
- `axon_rust-qfc.11` - Add MCP ingest normalization parity tests
- `axon_rust-qfc.12` - Refresh GitHub ingest docs for clone-based file ingestion
- `axon_rust-qfc.13` - Correct YouTube ingest docs to match playlist/channel implementation state
- `axon_rust-qfc.14` - Expose a service-level ingest progress contract
- `axon_rust-qfc.15` - Report Reddit partial comment-fetch failures in ingest results
- `axon_rust-qfc.16` - Standardize ingest progress payload keys
- `axon_rust-qfc.17` - Canonicalize YouTube playlist enumeration rows before processing
- `axon_rust-qfc.18` - Update crates/ingest README for unified ingest CLI entrypoints
- `axon_rust-qfc.19` - Document MCP ingest target formats and validation behavior
- `axon_rust-qfc.20` - Centralize provider target parsers for ingest routing and execution

Final Beads state:

- `bd show axon_rust-qfc --json` reported the epic `closed`.
- `epic_total_children`: 20
- `epic_closed_children`: 20
- `bd list --status in_progress --json` returned `[]`.

`bd dolt push` was attempted and failed because no Dolt remote is configured:

```text
fatal: remote 'origin' not found
```

`bd dolt remote list` returned:

```text
No remotes configured.
```

## Implementation Summary

### YouTube

Implemented source-side support for playlist/channel targets.

Key behavior:

- `@handle` targets normalize to `https://www.youtube.com/@handle`.
- `classify_youtube_target()` distinguishes single videos from playlist/channel targets.
- `ingest_youtube_target()` dispatches to either single-video ingest or playlist/channel ingest.
- Playlist/channel enumeration uses `yt-dlp --flat-playlist`.
- Enumerated rows are canonicalized to `https://www.youtube.com/watch?v=<id>`.
- Empty or invalid enumeration rows are skipped with warnings.
- Playlist/channel progress reports `videos_done`, `videos_total`, and `chunks_embedded`.

Primary files:

- `crates/ingest/youtube.rs`
- `crates/services/ingest.rs`
- `docs/ingest/youtube.md`
- `docs/commands/ingest.md`

### GitHub

Hardened GitHub target parsing, clone fallback, and file embed accounting.

Key behavior:

- Added normalized `GitHubTarget` parsing.
- GitHub execution uses the shared normalized parser.
- Private repositories do not retry unauthenticated clone after authenticated clone failure.
- Unknown-visibility repositories skip unauthenticated retry when the clone error is auth/permission related.
- Clone stderr redacts configured token text before logging/returning errors.
- File embedding now counts failed read/stat and embed batch failures.
- Failed embed batches fail the GitHub files subtask instead of silently succeeding with missing chunks.

Primary files:

- `crates/ingest/github.rs`
- `crates/ingest/github/files.rs`
- `crates/ingest/github/files/batch.rs`
- `docs/ingest/github.md`
- `crates/ingest/README.md`

### Reddit

Hardened Reddit target validation, retry behavior, cancellation, and partial failure reporting.

Key behavior:

- Reddit target parsing is fallible and canonicalizes thread URLs/permalinks.
- Thread URLs are restricted to `reddit.com`, `www.reddit.com`, and `old.reddit.com`.
- Non-Reddit `/comments/` URLs are rejected before HTTP construction.
- 429 handling honors `Retry-After` with a 60 second cap and falls back to exponential delay.
- Retry sleeps are cancellation-aware.
- Reddit source ingest checks cancellation between source phases.
- Comment fetch failures are counted and surfaced in `reddit_stats`.
- Service result payloads include partial comment failure counters.

Primary files:

- `crates/ingest/reddit.rs`
- `crates/ingest/reddit/client.rs`
- `crates/ingest/reddit/comments.rs`
- `crates/ingest/reddit/types.rs`
- `docs/ingest/reddit.md`

### Shared Progress And MCP

Added shared async ingest progress persistence and MCP validation parity.

Key behavior:

- Service ingest functions now have progress-capable variants for GitHub, Reddit, YouTube, and sessions.
- Lite ingest workers persist live progress payloads into `axon_ingest_jobs.result_json`.
- A DB regression test proves `update_result_json()` updates progress while preserving `running` status.
- Reddit cancellation tokens are registered by lite ingest workers and passed into the Reddit source layer.
- MCP `ingest.start` validates/normalizes GitHub, Reddit, and YouTube targets before enqueueing.
- MCP ingest docs now include accepted target formats and validation behavior.
- Common progress payload keys are documented in `PhaseReporter`.

Primary files:

- `crates/jobs/lite/ops.rs`
- `crates/jobs/lite/workers.rs`
- `crates/jobs/lite/workers/runners.rs`
- `crates/services/ingest.rs`
- `crates/mcp/server/handlers_embed_ingest.rs`
- `crates/ingest/progress.rs`
- `docs/MCP-TOOL-SCHEMA.md`

## Verification Evidence

Fresh integrated checks:

```bash
cargo fmt --check
cargo test --lib ingest
```

Observed results:

- `cargo fmt --check`: passed.
- `cargo test --lib ingest`: 195 passed, 0 failed, 0 ignored, 1191 filtered out.

Provider-worker focused checks also passed before final integration:

- GitHub worker: `cargo check --lib`, `cargo test --lib github:: -- --nocapture` passed with 38 passed, 0 failed.
- Reddit worker: `cargo check --lib`, `cargo test --lib reddit` passed with 31 passed, 0 failed.
- YouTube worker: source tests were integrated into the final `cargo test --lib ingest` pass.

## Current Worktree Caveat

The worktree remains dirty and includes many modifications that predated this scoped ingest remediation. This session did not commit or push, because staging everything would risk bundling unrelated work.

Files intentionally touched for this scoped run include:

- `crates/ingest/README.md`
- `crates/ingest/github.rs`
- `crates/ingest/github/files.rs`
- `crates/ingest/github/files/batch.rs`
- `crates/ingest/progress.rs`
- `crates/ingest/reddit.rs`
- `crates/ingest/reddit/client.rs`
- `crates/ingest/reddit/comments.rs`
- `crates/ingest/reddit/types.rs`
- `crates/ingest/youtube.rs`
- `crates/jobs/lite/ops.rs`
- `crates/jobs/lite/workers.rs`
- `crates/jobs/lite/workers/runners.rs`
- `crates/mcp/server/handlers_embed_ingest.rs`
- `crates/services/ingest.rs`
- `docs/MCP-TOOL-SCHEMA.md`
- `docs/commands/ingest.md`
- `docs/ingest/github.md`
- `docs/ingest/reddit.md`
- `docs/ingest/youtube.md`

## Follow-Up Notes

- Beads are local only until a Dolt remote is configured.
- The broad repo test suite was not run; only the focused ingest gate plus formatting were run after integration.
- If committing later, isolate this scoped ingest work from the unrelated dirty files already present in the branch.
