# axon sessions
Last Modified: 2026-06-11

<!-- BEGIN GENERATED ACTION SURFACES -->
## Surfaces

| Surface | Entry point |
|---|---|
| CLI | `axon sessions ...` |
| REST | Not inventoried |
| MCP | Not exposed as a dedicated MCP action. |
| Service | `Not inventoried` |

Parity notes: This action page is missing from docs/reference/api-parity.md.
<!-- END GENERATED ACTION SURFACES -->


Ingest local AI session history (Claude, Codex, Gemini) into Qdrant.

> For implementation details and troubleshooting see [`docs/guides/ingest/sessions.md`](../../guides/ingest/sessions.md).

## Synopsis

```bash
axon sessions [FLAGS]
axon sessions <SUBCOMMAND> [ARGS]
```

## Provider Sources

| Provider | Path |
|----------|------|
| Claude | `~/.claude/projects/` |
| Codex | `~/.codex/sessions/` |
| Gemini | `~/.gemini/history/`, `~/.gemini/tmp/` |

## Flags

All global flags apply. Sessions-specific and key flags:

| Flag | Default | Description |
|------|---------|-------------|
| `--claude` | off | Include Claude sessions. Presence flag — include to enable. |
| `--codex` | off | Include Codex sessions. Presence flag — include to enable. |
| `--gemini` | off | Include Gemini sessions. Presence flag — include to enable. |
| `--project <name>` | — | Case-insensitive substring filter on project name. |
| `--wait <bool>` | `false` | Block until ingestion completes; otherwise enqueue async ingest job. |
| `--collection <name>` | `axon` | Target Qdrant collection. |
| `--json` | `false` | Machine-readable output. |

Provider selection rule:
- If none of `--claude/--codex/--gemini` is set, all providers are ingested.

## Job Subcommands

```bash
axon sessions status <job_id>
axon sessions cancel <job_id>
axon sessions errors <job_id>
axon sessions list
axon sessions cleanup
axon sessions clear
axon sessions recover
axon sessions worker
```

These subcommands operate on the shared ingest queue across source types.

## Auto-Ingest Watcher

`axon sessions watch` is the host-local long-running auto-ingest process. It watches local AI session export roots, waits for file writes to settle, prepares changed files with the same local parsers used by `axon sessions`, and ingests through the existing prepared-session path.

```bash
axon sessions watch --json
axon sessions watch --codex --project axon --json
axon sessions watch --path ~/.claude/projects --debounce-ms 750 --settle-ms 500 --max-retries 5
axon sessions watch --no-initial-scan --json
axon sessions watch-status --json
axon sessions smoke-watch --timeout-secs 30 --json
```

Default watched roots:

| Provider | Root | File shape |
|----------|------|------------|
| Claude | `~/.claude/projects/` | `.jsonl` |
| Codex | `~/.codex/sessions/` | `.jsonl` |
| Gemini | `~/.gemini/history/`, `~/.gemini/tmp/` | `.json` |

The watcher uses non-recursive directory watches and registers newly-created directories as they appear. File events are debounced, then a file is ingested only after size and mtime stay unchanged for the settle window. Overflow or backend rescan signals trigger a full root rescan. Parse/upload/storage failures are retried up to `--max-retries` and recorded in the session watch checkpoint tables.

`watch-status` summarizes checkpoint and recent error rows. `smoke-watch` writes a valid Codex probe transcript and waits for concrete checkpoint evidence from the running watcher; it fails if ingestion is not observed before the timeout.

`sessions watch` accepts the same provider/project filters as one-shot session ingest (`--claude`, `--codex`, `--gemini`, `--project <name>`), scoped to the watch subcommand.

When `--upload-to-server` is explicitly enabled and `AXON_SERVER_URL` is set, the watcher still parses and redacts local files on the client, then uploads prepared docs to `POST /v1/ingest/sessions/prepared` with bearer auth and request timeouts. It never asks the server to scan server-local transcript roots. Remote upload requires `202 Accepted` plus a returned `job_id`; that durable queue acceptance is logged as `accepted_remote` and stored as a distinct local `remote_accepted` checkpoint state. It is not treated as a completed local ingest and does not satisfy `smoke-watch` success. Without that opt-in, v0 uses the local prepared-session service path and checkpoints only after local prepared-session ingest succeeds.

## Examples

```bash
# Async enqueue (default) for all providers
axon sessions

# Sync run for Codex only
axon sessions --codex --wait true

# Claude + Gemini filtered to project name match
axon sessions --claude --gemini --project axon --wait true

# Check job status
axon sessions status 550e8400-e29b-41d4-a716-446655440000

# Install host-local automatic capture
axon setup session-watch-service install

# Watch locally but upload changed prepared docs to a running server
AXON_SERVER_URL=https://axon.example.com AXON_HTTP_TOKEN=... \
  axon sessions watch --upload-to-server --json
```

## Environment Variables

| Variable | Default | Purpose |
|----------|---------|---------|
| `AXON_SERVER_URL` | — | Server endpoint used by `axon sessions watch --upload-to-server` for prepared-session uploads. Plain `axon sessions` remains host-local. |
| `AXON_HTTP_TOKEN` | — | Bearer token sent by `axon sessions watch --upload-to-server` to authenticated Axon HTTP servers. |
| `AXON_SESSION_INGEST_MAX_BYTES` | `20971520` | Maximum bytes read from one session file before skipping it. |
| `AXON_SESSION_INGEST_MAX_TOTAL_TEXT_BYTES` | `104857600` | Maximum total prepared text accepted in one server-mode sessions request. |
| `AXON_COLLECTION` | `axon` | Default target collection when `--collection` is not supplied. |
| `QDRANT_URL` | `http://127.0.0.1:53333` | Vector store endpoint used by local mode and by the server process. |
| `TEI_URL` | — | Embedding endpoint used by local mode and by the server process. |

## External Dependency Install Instructions

Local `axon sessions` parsing/redaction uses the Axon binary only. Embedding still requires the normal Axon runtime services:

```bash
# Install/build axon from this repository
cargo build --release --bin axon
./target/release/axon --version

# Start required local services
just services-up
./scripts/axon doctor
```

For watcher remote upload, start a server built from the same revision before running the client:

```bash
AXON_HTTP_HOST=127.0.0.1 AXON_HTTP_PORT=8001 axon serve mcp --transport http
AXON_SERVER_URL=http://127.0.0.1:8001 AXON_HTTP_TOKEN=... \
  axon sessions watch --upload-to-server --codex --json
```

## Notes

- Local session text is decoded and redacted before embedding.
- Job records for local queued runs are Source jobs whose request metadata
  identifies the sessions source family.
- `AXON_SERVER_URL` does not route plain `axon sessions` through HTTP in this release. Only `axon sessions watch --upload-to-server` uses it.
- Legacy remote `source_type="sessions"` ingest is rejected so an Axon server cannot scan its own `~/.claude`, `~/.codex`, or `~/.gemini` paths by accident.
