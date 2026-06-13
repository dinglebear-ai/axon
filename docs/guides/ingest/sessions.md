# Sessions Ingest
Last Modified: 2026-06-13

Version: 1.0.0
Last Updated: 01:26:53 | 02/25/2026 EST

> CLI reference (flags, subcommands, examples): [`docs/reference/actions/sessions.md`](../../reference/actions/sessions.md)

Ingests exported AI conversation files (Claude, Codex, Gemini) into Qdrant. Session ingest scans local history paths, redacts secret-like tokens, normalizes each session chunk through the source-doc planner, and embeds planner-created `PreparedDoc` values through the shared TEI/Qdrant pipeline.

## Supported Formats

| Provider | Scan path | File format |
|----------|-----------|-------------|
| **Claude** | `~/.claude/projects/` | `.jsonl` per conversation |
| **Codex** | `~/.codex/sessions/` | `.jsonl` per session |
| **Gemini** | `~/.gemini/history/`, `~/.gemini/tmp/` | `.json` per conversation |

Each parser (`claude.rs`, `codex.rs`, `gemini.rs`) extracts message pairs (human + assistant turns) into flat text chunks, stripping internal metadata to keep only the conversational content.

## What Gets Indexed

- All human and assistant message turns
- Session metadata embedded as Qdrant point payload: source path, provider, project name
- Each changed session file produces one or more Qdrant points

## Size and Safety Limits

Each session file is bounded by `AXON_SESSION_INGEST_MAX_BYTES` (default: 20 MiB). Files above that limit fail closed. Secret-like tokens such as `sk-*`, `ghp_*`, `github_pat_*`, `atk_*`, and long mixed alphanumeric tokens are redacted before embedding.

## How It Works

1. Discovers all session files under each provider's scan path
2. Dispatches to the matching parser based on path/provider
3. Parser extracts message turns and formats each session document as redacted plain text
4. Session text is normalized with the shared source-doc helpers, then embedded via `embed_prepared_docs()` → TEI → Qdrant

Sessions defaults to **async queued execution** when `--wait false` (default): it enqueues an ingest job and returns a job ID.

Use `--wait true` for synchronous execution.

## Auto-Capture vs SessionStart Recall

The Claude plugin SessionStart hook is recall-only: it calls `axon memory context` for the current git project and must stay fast and best-effort. It does not scan or ingest session files.

Automatic capture is handled by the separate host-local watcher:

```bash
axon setup session-watch-service install
```

That service runs `axon sessions watch --no-initial-scan --json`, watches Claude/Codex/Gemini transcript roots, and reuses prepared-session ingest. Full-file reingest is the v0 behavior; deterministic point IDs and stale-tail cleanup make it correct when a transcript changes. Append-offset optimization can be added later using the checkpoint table fields once the simpler full-file path has proven stable.

Provider and project filters are available on the watcher itself, for example `axon sessions watch --codex --project axon --json`.

## Remote Watch Upload

When `AXON_SERVER_URL` is set together with `axon sessions watch --upload-to-server`, the watcher still reads session files from the client machine. The client parses and redacts Claude, Codex, and Gemini transcripts locally, sends prepared documents to `POST /v1/ingest/sessions/prepared`, and the server persists that upload beside a SQLite ingest job before waking ingest workers. Plain `axon sessions` remains host-local in this release.

The remote server must return `202 Accepted` with a `job_id`. The watcher emits an accepted-remote event and writes a local `remote_accepted` checkpoint for that durable upload acceptance. That checkpoint means the file was queued remotely, not that the remote embedding job reached terminal success, and it is intentionally separate from local success checkpoints.

Prepared-session uploads are bounded by semantic limits: max document count, per-document text size, total text size, metadata size, supported platform names, and collection-name validation. The uploaded payload is deleted after successful worker completion and is included in ingest cleanup/clear behavior.

The generic remote ingest shape `source_type="sessions"` is intentionally rejected over REST/MCP in this phase. That prevents remote callers from causing the server to scan server-local AI history directories.

## Git Enrichment (repo and branch fields)

Session metadata includes project and repository context where it can be resolved. Claude project directories are decoded back to filesystem paths and the git `origin` remote is read once per project directory. The result is shared across all sessions within that project.

### How enrichment works

1. The session scanner decodes the Claude CLI project folder name (e.g. `-home-jmagar-workspace-axon-rust`) back to a filesystem path via `decodeProjectPath()`.
2. `enrichWithGit(projectPath)` walks up the directory tree looking for a `.git` directory.
3. If a git root is found, `git remote get-url origin` is read and normalized to a GitHub slug when possible.

### Fallback for hyphenated directory names

Because the Claude CLI encodes path separators and literal hyphens identically (both become `-`), a single lossless decode is not always possible. When the naively decoded path does not exist on disk, `enrichWithGit` iterates over candidate paths generated by `decodedProjectPathCandidates()` (up to 16 candidates) and uses the first one that exists. This handles projects in directories with real hyphens in their names.

### Field shapes

| Field | Type | Value |
|-------|------|-------|
| `project` | `string` | Project/display name derived from the session path |
| `project_path` | `string \| undefined` | Decoded local project path when resolvable |
| `gh_repo` | `string \| undefined` | `owner/repo` string parsed from the `origin` remote URL; absent if no remote or parse fails |

## Adding a New Session Format

1. Create `src/ingest/sessions/<provider>.rs` (or add provider parser logic under `src/ingest/sessions.rs` if keeping a single module)
2. Implement `ingest_<provider>_sessions(cfg, state, multi)` following the pattern in `claude.rs`
3. Register it in sessions dispatch (`src/ingest/sessions.rs`) with a `cfg.sessions_<provider>` flag check
4. Add the `--<provider>` flag (e.g. `--claude`, `--codex`, `--gemini`) to `SessionsArgs` in `src/core/config/cli.rs` and wire it through `src/core/config/parse/build_config/`
5. Add a unit test with a minimal sample file in `#[cfg(test)]`

## Troubleshooting

**No files processed / `0 chunks indexed`**

Session export files don't exist at the scanned paths. Export conversations from the respective app:
- Claude: Settings → Export Data
- Codex: `codex export` or check `~/.codex/sessions/` after running sessions
- Gemini: Check `~/.gemini/history/` after using Gemini CLI

**Parse errors on a `.jsonl` / `.json` file**

The export schema may have changed. Open the file and verify the structure matches what the parser expects, or check `src/ingest/sessions/<provider>.rs` for the expected fields.

**`gh_repo` missing**

The decoded project directory either does not exist on disk, is not inside a git repository, or has no `origin` remote configured. Run `git remote -v` in the project directory to verify the remote is set up correctly.
