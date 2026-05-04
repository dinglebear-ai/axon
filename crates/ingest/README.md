# crates/ingest
Last Modified: 2026-05-03

Source-specific ingestion pipelines for non-crawl content.

## Purpose
- Fetch and normalize external source content into a form suitable for embedding.
- Support ingestion from GitHub, Reddit, YouTube, and AI session exports.

## Responsibilities
- GitHub ingest for repository files/issues/wiki.
- Reddit ingest for subreddit posts/comments.
- YouTube transcript ingest.
- Session ingest parsers for Claude, Codex, and Gemini exports.

## Key Files
- `github.rs` + `github/files.rs` + `github/issues.rs` + `github/wiki.rs`: GitHub source ingest.
- `reddit.rs`: Reddit source ingest.
- `youtube.rs`: YouTube transcript ingest.
- `sessions.rs` + `sessions/claude.rs` + `sessions/codex.rs` + `sessions/gemini.rs`: session export parsers.

## Integration Points
- Async ingest job orchestration lives in `crates/jobs/ingest.rs`.
- Embedded output ultimately flows into `crates/vector/ops` and Qdrant.
- Unified source ingest CLI entrypoints live in `crates/cli/commands/ingest.rs` and shared helpers in `crates/cli/commands/ingest_common.rs`.
- AI session export ingestion is exposed through the `sessions` command path, with parsers in this crate and command handling outside the provider-specific GitHub/Reddit/YouTube adapters.

## Notes
- Keep source adapters isolated by provider to avoid cross-provider coupling.
- Changes to extracted metadata shape should be coordinated with downstream vector payload consumers.

## Related Docs
- [Repository README](../../README.md)
- [Architecture](../../docs/ARCHITECTURE.md)
- [Docs Index](../../docs/README.md)
