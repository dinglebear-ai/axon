# axon sessions
Last Modified: 2026-07-15

<!-- BEGIN GENERATED ACTION SURFACES -->
## Surfaces

| Surface | Entry point |
|---|---|
| CLI | <code>axon sessions</code> |
| REST | <code>GET /v1/sources</code><br><code>GET /v1/sources/{source_id}</code><br><code>POST /v1/sources</code> |
| MCP atomic tools | <code>source</code> |
| Shared service ownership | [axon-services](../../../crates/axon-services/src/lib.rs) and the owning domain crate; see [crate ownership](../../architecture/crate-ownership.md) |

MCP names describe the atomic projection. The legacy `axon` tool uses the corresponding action/subaction selectors; `both` exposes both projections. Discover the running server before calling. [MCP contract](../mcp/tool-schema.md) owns exact schemas and selectors.

Family-level navigation does not imply identical suboperations or request shapes across transports. Transcript ingestion uses session:<provider>:<path> through source acquisition; mobile chat sessions are a different resource.
<!-- END GENERATED ACTION SURFACES -->


Index local AI session history (Claude, Codex, Gemini) into Qdrant.

> For implementation details and troubleshooting see [`docs/guides/ingest/sessions.md`](../../guides/ingest/sessions.md).

## Synopsis

```bash
axon sessions [FLAGS]
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
| `--project <name>` | — | Case-insensitive substring filter on export path or transcript content. Reliable for Claude project directories and Codex exports with `cwd`; Gemini only matches when the export path/content contains the token. |
| `--wait <bool>` | `false` | Block until indexing completes; otherwise enqueue an async source job. |
| `--collection <name>` | `axon` | Target Qdrant collection. |
| `--json` | `false` | Machine-readable output. |

Provider selection rule:
- If none of `--claude/--codex/--gemini` is set, all providers are indexed.

## Job Lifecycle

`axon sessions` enqueues unified source jobs when `--wait false` (the default).
Use the canonical job commands for lifecycle operations:

```bash
axon jobs list --kind source
axon jobs get <job_id>
axon jobs cancel <job_id>
```

## Unified Source Path

`axon sessions` remains the local convenience command for indexing Claude,
Codex, and Gemini transcript roots. The transport-neutral path is a
`SourceRequest` whose `source` is an explicit session selector:

```bash
axon 'session:codex:/home/me/.codex/sessions/2026/07/15/session.jsonl' --wait true
```

Session selector shape:

| Provider | Root | File shape |
|----------|------|------------|
| Claude | `~/.claude/projects/` | `.jsonl` |
| Codex | `~/.codex/sessions/` | `.jsonl` |
| Gemini | `~/.gemini/history/`, `~/.gemini/tmp/` | `.json` |

A selector has the form `session:<provider>:<path>`, where provider is
`claude`, `codex`, or `gemini`, and path is a session export file or directory.
The selector forces the session adapter path, so session parsing/redaction is
used instead of generic local-file indexing.

The previous session-specific watch service, status/smoke helpers, and setup
service are intentionally rejected. Durable watching for sessions belongs in the
unified source/watch pipeline.

## Examples

```bash
# Async enqueue (default) for all providers
axon sessions

# Sync run for Codex only
axon sessions --codex --wait true

# Claude + Codex filtered to project path/content match
axon sessions --claude --codex --project axon --wait true

# Transport-neutral source path for one Codex export
axon 'session:codex:/home/me/.codex/sessions/2026/07/15/session.jsonl' --wait true
```

## Environment Variables

| Variable | Default | Purpose |
|----------|---------|---------|
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

## Notes

- Local session text is decoded and redacted before embedding.
- Job records for local queued runs are Source jobs whose request metadata
  identifies the sessions source family.
- The removed prepared-session REST route must not be used or reintroduced.
- Legacy remote `source_type="sessions"` ingest is rejected so an Axon server
  cannot scan its own `~/.claude`, `~/.codex`, or `~/.gemini` paths by accident.
