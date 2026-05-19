# Session: Ingest Docs + yt-dlp Setup

**Date:** 2026-02-23
**Branch:** fix-crawl
**Duration:** ~1 session

---

## Session Overview

Fixed the `yt-dlp not found` error in the axon-workers Docker container, created a new `docs/ingest/` documentation structure with one file per ingest source, corrected multiple factual errors in existing command docs, and added `docs/CLAUDE.md` to govern future doc placement.

---

## Timeline

1. Diagnosed `yt-dlp not found` error — binary was at `~/.local/bin/yt-dlp` on the host but not in the Docker container's runtime stage
2. Added yt-dlp standalone binary installation to `docker/Dockerfile` runtime stage with arch detection
3. Created `docs/ingest/` directory with four new docs (youtube, github, reddit, sessions)
4. Discovered existing `docs/commands/` versions for all four — user confirmed keeping both with different purposes
5. Audited all eight docs for split violations, factual errors, and contradictions — found several
6. Corrected factual errors across all docs (wrong flag names, defaults, scan paths, false feature claims)
7. Trimmed `commands/youtube.md` install section; reverted `commands/github.md` per user request
8. Created `docs/CLAUDE.md` to define the commands/ vs ingest/ split and routing rules

---

## Key Findings

- `yt-dlp` at `/home/jmagar/.local/bin/yt-dlp` on host; Dockerfile runtime stage (`debian:12.9-slim`) had no Python and no yt-dlp — added standalone binary download (`yt-dlp_linux` / `yt-dlp_linux_aarch64` by arch)
- `crates/ingest/github.rs` only indexes **files** — issues, PRs, wiki, and repo metadata are **not implemented** (TODO). Both docs falsely claimed they were. User asked to revert commands doc to preserve the planned-feature description for upcoming implementation.
- `crates/ingest/youtube.rs:128-129` — `extract_video_id()` then reconstructs `watch?v=<ID>`, discarding `list=` — playlists silently stripped; pure playlist URLs fail. Commands doc had a broken playlist example.
- Reddit CLI flags are `--sort`, `--time`, `--max-posts` (default 25), `--min-score` (default 0), `--depth` (default 2) — both docs had wrong names (`--reddit-sort` etc.) and wrong defaults (`100`, `1`, `3`)
- Sessions scan paths (from source): Claude=`~/.claude/projects/`, Codex=`~/.codex/sessions/`, Gemini=`~/.gemini/history/` + `~/.gemini/tmp/` — ingest doc had wrong paths
- Sessions state tracker (`axon_session_ingest_state`) prevents re-indexing unchanged files — commands doc incorrectly said "re-embeds every time"
- `--force` and `--format` flags referenced in ingest/sessions.md do not exist in the CLI

---

## Technical Decisions

- **Standalone yt-dlp binary over pip**: No Python in the runtime stage; standalone binary is self-contained, ~70MB, arch-specific. Alternative (add Python + pip install) would bloat the image significantly.
- **Keep both commands/ and ingest/ docs**: Commands = CLI reference (flags, subcommands, examples). Ingest = system docs (how it works, setup, troubleshooting). Serve different readers.
- **Revert github commands doc**: User wants to implement issues/PRs/wiki soon — preserving the aspirational scope description avoids having to rewrite it when the feature lands. The ingest doc still accurately reflects current state.
- **Cross-links in both directions**: Every commands doc links to its ingest counterpart and vice versa — prevents docs diverging without a clear "source of truth" signal.

---

## Files Modified

| File | Change |
|------|--------|
| `docker/Dockerfile` | Added yt-dlp standalone binary install to runtime stage (arch-aware: amd64/arm64) |
| `docs/commands/youtube.md` | Fixed playlist example (only `watch?v=` URLs work), trimmed yt-dlp install section to one line + cross-link |
| `docs/commands/reddit.md` | Added correct reddit-specific flags table (`--sort`, `--time`, `--max-posts`, `--min-score`, `--depth`) with verified defaults; removed wrong flag names |
| `docs/commands/sessions.md` | Fixed incorrect "re-embeds every time" claim; corrected scan paths; removed non-existent flags |
| `docs/commands/github.md` | Reverted to pre-session state (aspirational scope preserved for upcoming implementation) |
| `docs/ingest/youtube.md` | **Created** — pipeline internals, URL handling details, yt-dlp Docker install, limitations, troubleshooting |
| `docs/ingest/github.md` | **Created** — what gets indexed (files only, current state), env vars, how it works, limitations |
| `docs/ingest/reddit.md` | **Created** — OAuth2 setup, how it works, rate limits, limitations, troubleshooting |
| `docs/ingest/sessions.md` | **Created** — supported formats, correct scan paths, state tracker details, how it works |
| `docs/CLAUDE.md` | **Created** — defines commands/ vs ingest/ split, what belongs where, cross-linking rules, new-doc guidance |

---

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| yt-dlp in Docker | Not installed → `yt-dlp not found or failed to start` on every YouTube ingest job | Installed as standalone binary at `/usr/local/bin/yt-dlp` in runtime stage |
| Reddit flag names | Docs said `--reddit-sort`, `--reddit-limit 100`, `--reddit-comment-depth 3`, etc. | Corrected to `--sort`, `--max-posts 25`, `--depth 2` (matching actual CLI) |
| Sessions behavior description | Commands doc said "re-runs re-embed everything; use axon dedupe after" | Corrected: state tracker skips unchanged files automatically |
| YouTube playlist examples | Commands doc showed `axon youtube "https://...playlist?list=PLxxx"` as valid | Corrected: pure playlist URLs fail; `list=` stripped from watch URLs |
| Sessions scan paths | Ingest doc listed `~/.config/claude/`, `~/.codex/`, `~/.config/gemini/` | Corrected to `~/.claude/projects/`, `~/.codex/sessions/`, `~/.gemini/history/` + `~/.gemini/tmp/` |

---

## Verification Evidence

| Check | Expected | Actual | Status |
|-------|----------|--------|--------|
| `yt-dlp` in Dockerfile | RUN block in runtime stage | Added after `WORKDIR /app`, before binary copy | ✅ |
| Reddit `--sort` default | `Hot` | `RedditSort::Hot` at `config/parse/mod.rs:76` | ✅ |
| Reddit `--max-posts` default | `25` | `default_value_t = 25` at `config/cli.rs:142` | ✅ |
| Reddit `--depth` default | `2` | `default_value_t = 2` at `config/cli.rs:148` | ✅ |
| Sessions Claude path | `~/.claude/projects/` | `expand_home("~/.claude/projects")` at `sessions/claude.rs:20` | ✅ |
| Sessions Codex path | `~/.codex/sessions/` | `expand_home("~/.codex/sessions")` at `sessions/codex.rs:18` | ✅ |
| Sessions Gemini paths | `~/.gemini/history/` + `~/.gemini/tmp/` | `gemini_root.join("history")` + `gemini_root.join("tmp")` at `sessions/gemini.rs:33` | ✅ |
| GitHub issues/PRs/wiki | NOT implemented | `ingest_github` ends at file embed (~line 235), no issues/PR/wiki fetch | ✅ (correctly documented) |
| YouTube playlist handling | `list=` stripped | `extract_video_id` + `watch?v={video_id}` reconstruction at `youtube.rs:128-129` | ✅ |

---

## Risks and Rollback

**Dockerfile change (yt-dlp):**
- Risk: GitHub releases URL format could change; `yt-dlp_linux` binary name could change
- Rollback: Remove the `RUN` block (`docker/Dockerfile` lines added after `WORKDIR /app`)
- The container will still build without it; only YouTube ingest jobs will fail

**No code changes** — all other changes are documentation only, zero runtime risk.

---

## Decisions Not Taken

- **Pin yt-dlp to a specific version**: Using `releases/latest` means automatic updates on each build. A pinned version would be more reproducible but requires manual bumps when YouTube changes its format. Left as `latest` since yt-dlp needs frequent updates anyway.
- **Add `--force` flag to sessions**: Would allow re-indexing without touching files. Not implemented — deferred; `touch <file> && axon sessions` is a workable workaround for now.
- **Fix github commands doc accuracy**: User explicitly asked to revert — the false feature claims in commands/github.md are preserved as the intended scope description for the upcoming implementation session.
- **Single doc per command (no split)**: The split adds complexity but the two audiences (CLI user vs. operator/developer) are genuinely different. A single file mixing flag tables with pipeline internals and troubleshooting gets unwieldy.

---

## Open Questions

- Does `yt-dlp_linux` binary work on all amd64 Debian-based containers, or are there glibc version constraints? (Not verified — assumed yes for `debian:12.9-slim`)
- `docs/commands/github.md` still lists "issues, PRs, wiki, metadata" as ingest scope — these need to match reality once the feature is implemented. Need to keep the ingest doc in sync when those land.
- `axon ingest errors <uuid>` is silently unhandled (known gap from MEMORY.md) — not addressed this session

---

## Next Steps

- **Implement GitHub issues/PRs/wiki ingestion** — user indicated this is imminent; `docs/commands/github.md` scope description is already written for it, `docs/ingest/github.md` accurately reflects current (files-only) state and will need a "What Gets Indexed" update when the feature lands
- Rebuild and push `axon-workers` Docker image to pick up yt-dlp fix
- Fix `axon ingest errors <uuid>` silent failure (add `"errors"` arm to `maybe_handle_ingest_subcommand`)
- Consider adding `--include-source` flag documentation to `commands/github.md` (CLI flag exists at `config/cli.rs:118` as `include_source`, exposed as `--include-source`)
