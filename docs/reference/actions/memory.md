# axon memory
Last Modified: 2026-06-13

<!-- BEGIN GENERATED ACTION SURFACES -->
## Surfaces

| Surface | Entry point |
|---|---|
| CLI | `axon memory ...` |
| REST | `POST /v1/memory` (Implemented) |
| MCP | `{ "action": "memory", "subaction": "..." }` (`memory.remember`, `memory.list`, `memory.search`, `memory.show`, `memory.link`, `memory.supersede`, `memory.context`) |
| Service | `services::memory::{dispatch,remember,list,search,show,link,supersede,context}` |

Parity notes: Single direct REST endpoint accepts the memory subaction envelope and uses write scope because some subactions mutate persistent memory.
<!-- END GENERATED ACTION SURFACES -->


Persistent agent memory. Content and embeddings are stored in the dedicated Qdrant memory collection (`axon_memory` by default, or `AXON_MEMORY_COLLECTION`), while SQLite stores the metadata/graph mirror. New memories are normalized through `SourceDocument::new_memory(...)` before embedding, so the Qdrant point uses the same deterministic UUID as SQLite and carries the shared planner fields (`chunk_content_kind`, `chunk_locator`, `source_range`).

## Commands

```bash
axon memory remember "Memory content lives in Qdrant." --project axon
axon memory list --project axon --repo jmagar/axon --type decision --status active --limit 20
axon memory search "where is memory stored" --project axon --limit 10
axon memory show <memory-id>
axon memory link <source-memory-id> <target-memory-id> --type relates_to
axon memory supersede <replacement-memory-id> <old-memory-id>
axon memory context --project axon --query "memory storage architecture" --token-budget 2000
```

## Subcommands

| Subcommand | Purpose |
|------------|---------|
| `remember` | Store one memory. `body` is required; `title` derives from the first body line when omitted. |
| `list` | Browse memory metadata from SQLite without a text query or Qdrant round-trip. Defaults to active memories. |
| `search` | Search active memories by semantic query with optional `project`, `repo`, and `file` filters. |
| `show` | Show a memory by server-generated id. |
| `link` | Create or refresh an idempotent SQLite graph edge between two memories. `--type` defaults to `relates_to`; `supersedes` is also accepted. |
| `supersede` | Mark the old memory `superseded` in SQLite and Qdrant, then create a `supersedes` edge from the replacement memory to the old memory. Superseded memories are excluded from `search`. |
| `context` | Build an inline, XML-wrapped memory context block from project/repo/file/query seeds plus one-hop graph neighbors. Output is defanged and budget-truncated. |

`remember` accepts `--type decision|fact|preference|task|bug`, `--title`, `--project`, `--repo`, `--file`, and `--confidence`.
`list` accepts `--project`, `--repo`, `--file`, `--type decision|fact|preference|task|bug`, `--status active|superseded|archived`, and `--limit`. Results are metadata-only: `body` is omitted/null; use `show` when body hydration is needed.
`link` accepts `--type relates_to|supersedes`.
`context` accepts `--query`, `--project`, `--repo`, `--file`, `--limit`, and `--token-budget`.

## Claude Plugin SessionStart Recall

The Axon Claude plugin includes a best-effort SessionStart hook that runs:

```bash
axon memory context --project <git-root-name> --repo <owner/name> --token-budget 1200 --limit 6
```

The hook infers the current git root from Claude hook environment variables, hook stdin when present, or `PWD`. It silently skips recall when `axon`, `git`, `timeout`, or a git repository are unavailable, and it suppresses command errors so session startup is not blocked by memory outages. Successful recall is printed inside an evidence-only `<axon_session_memory_context>` block.

Environment controls:

| Variable | Default | Purpose |
|----------|---------|---------|
| `AXON_SESSION_MEMORY_CONTEXT` | `1` | Set `0`, `false`, `no`, or `off` to disable the hook. |
| `AXON_SESSION_MEMORY_TIMEOUT_SECS` | `4` | Maximum time allowed for the memory recall CLI call. |
| `AXON_SESSION_MEMORY_TOKEN_BUDGET` | `1200` | Token budget passed to `memory context`. |
| `AXON_SESSION_MEMORY_LIMIT` | `6` | Maximum memory nodes requested. |
| `AXON_SESSION_MEMORY_QUERY` | unset | Optional query seed for the context request. |
