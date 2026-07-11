# Command Contract
Last Modified: 2026-06-30

## Contract

This is the target clean-break CLI contract. The current implementation still
exposes the older command-first surface described below.

The CLI is a first-class transport over the same Axon service contracts as MCP
and REST. It must not invent alternate semantics, hidden compatibility aliases,
or CLI-only data paths for source acquisition.

```text
argv
  -> CommandParser
  -> axon-api request DTO
  -> axon-services
  -> axon-api result DTO
  -> human renderer or JSON renderer
```

The CLI is allowed to provide ergonomic syntax, color, progress bars, and
terminal affordances. It is not allowed to bypass `axon-api`, `axon-services`,
`SourceLedger`, `SourceGraph`, `EmbeddingProvider`, or `VectorStore`.

## Design Rules

- `axon <source>` is the source acquisition/indexing happy path.
- `axon embed`, `axon ingest`, `axon scrape`, `axon crawl`, `axon code-search`,
  and `axon code-search-watch` are removed user-facing commands.
- `axon map <source>` remains a top-level discovery command.
- `axon watch <source>` remains the explicit watch-management entrypoint.
- `axon extract` remains top-level for structured LLM extraction.
- `axon memory` remains top-level for durable memory lifecycle.
- Search/retrieval/RAG commands remain distinct: `search`, `query`,
  `retrieve`, and `ask` must not blur.
- Operational commands call the same service DTOs as REST/MCP.
- CLI output may be human-friendly, but `--json` output must be strict DTO
  envelopes with no prose-only fields.
- No backwards compatibility aliases are required or desired.

## Current Implementation Snapshot

Verified against the live binary (`cargo build --bin axon`, `axon --help`,
`axon <cmd> --help`) as of this snapshot:

Implemented today:

- `embed`, `ingest`, `scrape`, `crawl`, `code-search`, and
  `code-search-watch` are already **removed** as `clap` subcommands — they do
  not appear in the `Command` enum (`crates/axon-core/src/config/cli.rs`).
- A default `axon <source>` parser path already exists: `route_bare_source`
  in `crates/axon-core/src/config/source_routing.rs` rewrites argv before
  clap parsing so any leading token that isn't a recognized subcommand,
  removed-command name, or global flag is routed to the `source` subcommand
  (`axon https://x`, `axon ./dir`, `axon r/rust`, `axon pkg:npm/foo`, and
  even `axon scrape <url>` all resolve to `axon source <target>`). The
  target grammar's implicit-`<source>`-as-first-positional shape is not yet
  literal (there is still a real `source` subcommand under the hood), but the
  removed-command list and default-routing behavior this snapshot previously
  called "not implemented" are both live.
- Current subcommands (from `axon --help`) are: `map`, `endpoints`, `search`,
  `research`, `extract`, `screenshot`, `diff`, `brand`, `query`, `retrieve`,
  `ask`, `evaluate`, `train`, `summarize`, `suggest`, `memory`, `sources`,
  `domains`, `stats`, `migrate`, `status`, `source`,
  `sessions`, `watch`, `monitor`, `sync`, `refresh`, `fresh`, `debug`,
  `doctor`, `mcp`, `serve`, `setup`, `preflight`, `smoke`, `compose`,
  `completions`, `config`, `update`, `palette`, `jobs`, and `reset`.
  `dedupe` and `purge` are **not** live subcommands anywhere in the CLI (0
  grep hits in `crates/axon-cli/src/`) — the live prune surface today is
  `axon prune plan|exec` (`crates/axon-cli/src/commands/prune.rs`); the
  target-grammar `axon prune dedupe`/`axon prune purge <target>` rows further
  below in this doc are not-yet-implemented design, not current state.
- Async job subcommands are consolidated under `jobs` (`axon jobs --help`)
  rather than remaining family-specific per `crawl`/`extract`/`embed`/
  `ingest` — those parent commands no longer exist to hang job subcommands
  off of.
- `--json` output is command-specific today. For example, `search --json`,
  `query --json`, and job output do not share one strict envelope.

Remaining gap vs. this contract:

- `axon <source>` is implemented via an explicit `source` subcommand plus
  argv-rewriting, not as a literal top-level positional in the clap grammar
  itself. The contract's grammar block (`axon [global-options] <source>
  [source-options]`) is satisfied behaviorally but not structurally.
- CLI JSON output is not yet rendered from one shared `axon-api` envelope
  across all commands — it remains command-specific.
- Job, watch, artifact, prune, graph, provider, and collection operations are
  partially grouped (`jobs`, `watch`, `sync`, `fresh`, `config`) but not all
  under one canonical grouped-command taxonomy this contract describes.

## Top-Level Grammar

```text
axon [global-options] <source> [source-options]
axon [global-options] map <source> [map-options]
axon [global-options] watch <source> [watch-options]
axon [global-options] watch <subcommand> [watch-options]
axon [global-options] extract <source> --schema <schema> [extract-options]
axon [global-options] ask <question> [ask-options]
axon [global-options] query <query> [query-options]
axon [global-options] retrieve <source-or-url> [retrieve-options]
axon [global-options] chat <message> [chat-options]
axon [global-options] evaluate <question> [evaluate-options]
axon [global-options] suggest [focus] [suggest-options]
axon [global-options] search <query> [search-options]
axon [global-options] research <query> [research-options]
axon [global-options] summarize <source> [summarize-options]
axon [global-options] endpoints <source> [endpoint-options]
axon [global-options] brand <source> [brand-options]
axon [global-options] diff <source-a> <source-b> [diff-options]
axon [global-options] screenshot <source> [screenshot-options]
axon [global-options] memory <subcommand> [memory-options]
axon [global-options] jobs <subcommand> [job-options]
axon [global-options] artifacts <subcommand> [artifact-options]
axon [global-options] uploads <subcommand> [upload-options]
axon [global-options] prune <subcommand> [prune-options]
axon [global-options] collections <subcommand> [collection-options]
axon [global-options] graph <subcommand> [graph-options]
axon [global-options] providers <subcommand> [provider-options]
axon [global-options] config <subcommand> [config-options]
axon [global-options] setup <subcommand> [setup-options]
axon [global-options] reset [reset-options]
axon [global-options] preflight [preflight-options]
axon [global-options] smoke [smoke-options]
axon [global-options] serve [serve-options]
axon [global-options] mcp [mcp-options]
axon [global-options] palette <subcommand> [palette-options]
axon [global-options] capabilities [options]
axon [global-options] status [options]
axon [global-options] doctor [options]
axon help [topic]
axon --help
axon --version
```

Parser rule (resolved, U1-25, 2026-07-09 audit): if the first positional
token is not a canonical (registered) command or a global flag, treat it as
`<source>` and route to `SourceRequest`. Removed-command tokens are **not**
special-cased or excluded — since they are not registered `clap` subcommands,
they fall through to the same bare-source routing as any other unrecognized
token, per the live implementation in `route_bare_source`
(`crates/axon-core/src/config/source_routing.rs`): `axon embed <path>`
resolves to `axon source embed <path>` (literal token `embed` becomes the
source target string), not exit code 8 ("removed command invoked"). This is
the current, intentional behavior — the "excluded from routing" phrasing
this section previously used contradicted it and has been removed.

## Canonical Command Registry

| Command | DTO Request | DTO Result | Mutates | Async | Purpose |
|---|---|---|---:|---:|---|
| `axon <source>` | `SourceRequest` | `SourceResult` | yes | yes | Acquire, normalize, embed, refresh, and optionally watch a source. |
| `axon map <source>` | `SourceRequest` | `SourceResult` | no | maybe | Discover source items/URLs with `scope=map`, `embed=false`. |
| `axon watch <source>` | `WatchRequest` | `WatchResult` | yes | no | Create or ensure a watch for a source. |
| `axon watch <sub>` | `Watch*Request` | `Watch*Result` | yes | maybe | Manage watch lifecycle. |
| `axon extract <source>` | `ExtractRequest` | `ExtractResult` | artifact/graph optional | yes | Structured LLM extraction. |
| `axon search <query>` | `SearchRequest` | `SearchResult` | optional | no | External web discovery. |
| `axon query <query>` | `QueryRequest` | `QueryResult` | no | no | Indexed vector/graph retrieval. |
| `axon retrieve <source-or-url>` | `RetrievalRequest` | `RetrievalResult` | no | no | Stored content lookup by known identity. |
| `axon ask <question>` | `AskRequest` | `AskResult` | trace only | maybe | RAG answer from indexed context. |
| `axon chat <message>` | `ChatRequest` | `ChatResult` | trace only | maybe | Direct LLM chat without retrieval. |
| `axon evaluate <question>` | `EvaluationRequest` | `EvaluationResult` | trace only | yes | Evaluate RAG answer and baseline. |
| `axon suggest [focus]` | `SuggestRequest` | `SuggestResult` | no | maybe | Suggest source acquisition targets. |
| `axon research <query>` | `ResearchRequest` | `ResearchResult` | optional | yes | Web search/fetch/synthesis. |
| `axon summarize <source>` | `SummarizeRequest` | `SummarizeResult` | artifact only | maybe | Fetch and summarize without indexing by default. |
| `axon endpoints <source>` | `EndpointDiscoveryRequest` | `EndpointDiscoveryResult` | artifact only | maybe | Discover network/API endpoints. |
| `axon brand <source>` | `BrandRequest` | `BrandResult` | artifact only | maybe | Extract brand identity assets. |
| `axon diff <a> <b>` | `DiffRequest` | `DiffResult` | artifact only | maybe | Compare two sources. |
| `axon screenshot <source>` | `ScreenshotRequest` | `ScreenshotResult` | artifact | maybe | Capture screenshot artifact. |
| `axon memory <sub>` | `Memory*Request` | `Memory*Result` | yes | maybe | Durable memory lifecycle. |
| `axon jobs <sub>` | `Job*Request` | `Job*Result` | yes | no | Job status/control. |
| `axon artifacts <sub>` | `Artifact*Request` | `Artifact*Result` | no | no | Artifact listing/detail/content. |
| `axon uploads <sub>` | `Upload*Request` | `Upload*Result` | yes | no | Staged uploads. |
| `axon prune <sub>` | `Prune*Request` | `Prune*Result` | yes | yes | Cleanup, purge, dedupe. |
| `axon collections <sub>` | `Collection*Request` | `Collection*Result` | maybe | no | Collection listing/detail. |
| `axon graph <sub>` | `Graph*Request` | `Graph*Result` | no | no | SourceGraph query/resolve/detail. |
| `axon providers <sub>` | `Provider*Request` | `Provider*Result` | no | no | Provider capabilities/health. |
| `axon config <sub>` | `Config*Request` | `Config*Result` | maybe | no | Inspect, validate, and rewrite `.env`/`config.toml`. |
| `axon setup <sub>` | `Setup*Request` | `Setup*Result` | maybe | maybe | Bootstrap, compose helpers, update/sync, smoke/preflight helpers. |
| `axon reset` | `Reset*Request` | `Reset*Result` | yes | yes | Explicit destructive clean-slate reset of local stores. |
| `axon preflight` | `PreflightRequest` | `PreflightReport` | no | no | Check host/config/provider readiness before starting work. |
| `axon smoke` | `SmokeRequest` | `SmokeReport` | optional test data | yes | Run explicit smoke checks against configured providers. |
| `axon serve` | `ServeRequest` | `ServeResult` | process | no | Start REST/MCP/web/workers. |
| `axon mcp` | `McpServerRequest` | `McpServerResult` | process | no | Start stdio/HTTP MCP server mode. |
| `axon palette <sub>` | `Palette*Request` | `Palette*Result` | maybe | maybe | Desktop Palette app launch/status/export helper. |
| `axon capabilities` | `CapabilityRequest` | `CapabilityDocument` | no | no | Machine-readable server capability contract. |
| `axon status` | `StatusRequest` | `StatusReport` | no | no | Runtime status. |
| `axon doctor` | `DoctorRequest` | `DoctorReport` | no | maybe | Diagnostic checks. |

## Source Command

`axon <source>` is the only normal way to acquire/index a source.

Examples:

```bash
axon shadcn.com
axon shadcn.com --scope docs --refresh
axon /home/jmagar/workspace/axon --watch
axon github.com/jmagar/axon --scope repo
axon crates:serde
axon npm:@modelcontextprotocol/sdk
```

Normalized request:

```json
{
  "source": "shadcn.com",
  "scope": "docs",
  "embed": true,
  "refresh": "if_stale",
  "watch": "disabled",
  "wait": false,
  "options": {}
}
```

Rules:

- `source` is required.
- `embed` defaults to true.
- `scope` defaults through adapter capability rules.
- `--watch` creates or ensures a durable watch in addition to the source run.
- Plain local file/directory sources should become watched when source policy
  says local mutable sources stay fresh by default.
- `--refresh` forces refresh even when the ledger says current.
- `--no-embed` acquires/normalizes without vector storage.
- `--wait` blocks until the current job reaches terminal state.
- Without `--wait`, async work returns immediately with a job descriptor.

## Target Resolution

Command parsing must preserve the target string and call shared resolution.
Parser-level heuristics are limited to flags and subcommand routing.

Resolution precedence:

1. Explicit scheme or prefix: `https://`, `file://`, `git:`, `rss:`, `npm:`.
2. Existing local path for path-collidable strings.
3. Adapter-declared shorthand: `owner/repo`, `r/rust`, `@handle`.
4. Scheme-less host normalization.
5. Unknown source error with suggested prefixes.

The CLI must not bake in source-specific URL hacks that bypass
`SourceResolver`/`SourceRouter`.

## Source Flags

| Flag | Type | Default | Meaning |
|---|---|---|---|
| `--scope <scope>` | string | adapter default | Adapter-declared acquisition strategy. |
| `--watch` | bool | false unless source policy says otherwise | Create/ensure freshness lifecycle. |
| `--no-embed` | bool | false | Acquire and normalize without vector storage. |
| `--refresh` | bool | false | Force refresh. |
| `--wait` | bool | false | Block until terminal state. |
| `--json` | bool | false | Emit JSON envelope/events. |
| `--adapter <name>` | string | resolved | Force adapter when supported. |
| `--collection <name>` | string | config | Vector collection override. |
| `--limit <n>` | integer | adapter default | Source-specific item/page/file cap. |
| `--render-mode <mode>` | enum | adapter default | Web rendering strategy. |
| `--header <header>` | repeatable | none | Fetch header; redacted in logs/status. |
| `--output <path>` | path | none | Write result/artifact to explicit path when supported. |
| `--response-mode <mode>` | enum | `auto` | `inline`, `summary`, `artifact`, `path`, `auto`. |

## Map Command

`axon map <source>` discovers source items without embedding.

```bash
axon map shadcn.com
axon map github.com/jmagar/axon --scope repo
axon map mcp://labby/server --scope schema
```

Rules:

- `map` is a projection over `SourceRequest`.
- `scope` defaults to `map`.
- `embed` defaults to false and must not be enabled unless explicitly requested.
- `map` may fetch sitemaps, package indexes, repo trees, tool schemas, or MCP
  server capabilities.
- `map` must not publish vectors as a side effect.
- Human output is a discovered item list; JSON output is `SourceResult`.

## Watch Commands

Watch commands manage freshness lifecycle.

```bash
axon watch <source>
axon watch exec <source>
axon watch list
axon watch get <watch_id>
axon watch status <watch_id>
axon watch pause <watch_id>
axon watch resume <watch_id>
axon watch delete <watch_id>
axon watch history <watch_id>
```

Subcommand matrix:

| Command | Required | Optional | Result |
|---|---|---|---|
| `axon watch <source>` | `source` | `--scope`, `--every`, `--embed`, `--refresh` | watch descriptor |
| `axon watch exec <source>` | `source` or watch id | `--wait`, `--refresh` | job descriptor/result |
| `axon watch list` | none | `--status`, `--source-id`, `--limit`, `--cursor` | paged watches |
| `axon watch get <watch_id>` | `watch_id` | `--include-history` | watch detail |
| `axon watch status <watch_id>` | `watch_id` | none | heartbeat/progress |
| `axon watch pause <watch_id>` | `watch_id` | `--reason` | watch detail |
| `axon watch resume <watch_id>` | `watch_id` | none | watch detail |
| `axon watch delete <watch_id>` | `watch_id` | `--delete-state`, `--reason` | deletion result |
| `axon watch history <watch_id>` | `watch_id` | `--limit`, `--cursor` | run history |

`exec` is the contract spelling. Do not reintroduce `run-now`.

## Retrieval and Search Command Boundaries

These commands must stay sharply distinct. They all answer "find something",
but they touch different systems and have different side effects.

| Command | Primary Question | Input Interpreted As | Reads | Writes | Calls Web Search | Calls LLM | Output |
|---|---|---|---|---|---:|---:|---|
| `axon search <query>` | "What does the outside web say exists for this query?" | Search-engine query text | `SearchProvider` | optional source jobs only when explicitly enabled | yes | no | web result list, source hints, optional queued jobs |
| `axon query <query>` | "Which indexed chunks match this text?" | Semantic/vector query text | `VectorStore`, optional `SourceGraph`, optional `DocumentCache` | no | no | no | ranked chunks/documents with scores and metadata |
| `axon retrieve <source-or-url>` | "Show me the stored content for this known source/document/url." | Source id, document id, chunk id, URL, or canonical source URI | `SourceLedger`, `DocumentCache`, `ArtifactStore`, `VectorStore` metadata | no | no | no | stored documents/chunks/content in source order |
| `axon ask <question>` | "Answer my question from indexed knowledge." | Natural-language question | retrieval stack: `VectorStore`, `SourceGraph`, `DocumentCache`, `MemoryStore` when requested | optional trace/job/event rows | no | yes | synthesized answer with citations and retrieval trace |

Decision rules:

- Use `search` when the target may not be indexed yet or the user wants current
  web discovery. It returns search results, not Axon's stored knowledge.
- Use `query` when the user wants raw retrieval results from what Axon has
  already embedded. It must not synthesize an answer.
- Use `retrieve` when the caller already knows the source, URL, document, or
  chunk and wants stored content back. It is lookup by identity.
- Use `ask` when the user wants an answer. It performs retrieval and calls
  `LlmProvider` to synthesize from cited indexed context.

Validation:

- `search` requires query text.
- `query` requires query text.
- `retrieve` requires a source, URL, source id, document id, or chunk id.
- `ask` requires a question.
- `ask` with only a URL/source id should fail with a suggestion to use
  `retrieve` or `axon <source>`.

## Retrieval Command Schemas

### search

```bash
axon search "latest qdrant payload indexing" --limit 10
```

DTO:

```json
{
  "query": "latest qdrant payload indexing",
  "limit": 10,
  "time_range": null,
  "auto_source": false
}
```

### query

```bash
axon query "source ledger generation cleanup" --content-kind code --limit 10
axon query "where is provider cooling implemented" \
  --source /home/jmagar/workspace/axon \
  --content-kind code \
  --path-prefix crates/ \
  --freshness committed
```

DTO:

```json
{
  "query": "source ledger generation cleanup",
  "filters": {
    "content_kind": "code",
    "source": "/home/jmagar/workspace/axon",
    "path_prefix": "crates/"
  },
  "generation": "committed",
  "freshness": "committed",
  "limit": 10,
  "include_graph": false
}
```

Local code query rules:

- `axon query` is the canonical replacement for the old code-search command
- `--content-kind code` enables code-aware filters and result rendering
- `--source <path|source_id|canonical_uri>` restricts results to a repo,
  checkout, package, or other indexed source
- `--path-prefix`, `--language`, `--symbol`, and `--repo` are filters over
  canonical vector payload fields, not ad hoc path scans
- default freshness for code is `committed`; foreground query may trigger or
  report a refresh job, but it must not search an uncommitted generation
- if a refresh is running, output includes the current `job_id`, phase, and
  stale/committed generation warning
- no command named `code-search` or `code-search-watch` may dispatch

### retrieve

```bash
axon retrieve github.com/jmagar/axon --include-content --limit 50
```

DTO:

```json
{
  "source": "github.com/jmagar/axon",
  "include_content": true,
  "limit": 50
}
```

### ask

```bash
axon ask "How should source generations be published?" --include-trace
```

DTO:

```json
{
  "question": "How should source generations be published?",
  "filters": {},
  "include_trace": true
}
```

## Analysis and Inspection Commands

These commands may fetch, render, call providers, and write artifacts, but they
do not index by default.

| Command | Required | Optional | Result |
|---|---|---|---|
| `axon chat <message>` | message | `--system`, `--model`, `--temperature`, `--stream` | `ChatResult` |
| `axon evaluate <question>` | question | `--expected`, `--judge`, `--limit` | `EvaluationResult` |
| `axon suggest [focus]` | none | `--source-id`, `--limit`, `--constraints` | `SuggestResult` |
| `axon research <query>` | query | `--limit`, `--depth`, `--full-content`, `--auto-source` | `ResearchResult` |
| `axon summarize <source>` | source | `--instructions`, `--format`, `--header` | `SummarizeResult` |
| `axon endpoints <source>` | source | `--render-mode`, `--capture`, `--limit` | `EndpointDiscoveryResult` |
| `axon brand <source>` | source | `--render-mode`, `--include-screenshot` | `BrandResult` |
| `axon diff <source-a> <source-b>` | two sources | `--mode`, `--header` | `DiffResult` |
| `axon screenshot <source>` | source | `--viewport`, `--full-page`, `--wait-for` | `ScreenshotResult` |
| `axon extract <source>` | source, schema | `--instructions`, `--persist-artifact`, `--trusted-graph-write` | `ExtractResult` |

Rules:

- `research` may create source jobs only when explicitly requested.
- `summarize` fetches and summarizes without indexing by default.
- `extract` is structured LLM extraction, not indexing.
- `chat` has no retrieval by default. Use `ask` for RAG.

## Memory Commands

```bash
axon memory remember <text>
axon memory search <query>
axon memory context <prompt-or-source>
axon memory show <memory_id>
axon memory link <memory_id> <target>
axon memory supersede <old_memory_id> <new_memory_id>
axon memory reinforce <memory_id>
axon memory contradict <memory_id> <other_memory_id>
axon memory pin <memory_id>
axon memory archive <memory_id>
axon memory forget <memory_id>
axon memory review
axon memory compact <memory_id>...
```

**CLI wiring gap (not a missing feature):** `crates/axon-cli/src/commands/memory.rs`
today only implements `remember`, `list`, `search`, `show`, `link`, `supersede`,
and `context` as CLI subcommands. `reinforce`, `contradict`, `pin`, `archive`,
`forget`, `review`, and `compact` are not yet wired into the CLI's `clap` tree —
running any of them fails as an unrecognized subcommand. The underlying
lifecycle **is** implemented in `axon-memory` (see
`crates/axon-memory/src/CLAUDE.md` and the reinforcement/decay/review/pin/
archive/forget/compact operations there); this is purely a CLI transport gap.
It is tracked as **Task 9 ("CLI, MCP, And REST Memory Contract")** in
`docs/pipeline-unification/plans/2026-07-04-phase-3b-security-error-memory-completion.md`,
which requires CLI/MCP/REST to expose the full lifecycle as one contract.

Subcommand matrix:

| Command | Required | Optional | Mutates |
|---|---|---|---:|
| `remember` | text | `--type`, `--scope`, `--no-embed`, `--graph-link` | yes |
| `search` | query | `--scope`, `--limit`, `--include-archived` | no |
| `context` | prompt/source | `--budget-tokens`, `--scope`, `--include-working` | no |
| `show` | memory id | `--include-graph`, `--include-events` | no |
| `link` | memory id, target | `--edge-kind`, `--confidence` | yes |
| `supersede` | old id, new id | `--reason` | yes |
| `reinforce` | memory id | `--signal`, `--amount`, `--context` | yes |
| `contradict` | memory id, other id | `--reason` | yes |
| `pin` | memory id | `--reason` | yes |
| `archive` | memory id | `--reason` | yes |
| `forget` | memory id | `--reason`, `--hard-delete` | yes |
| `review` | none | `--reason`, `--limit`, `--cursor` | maybe |
| `compact` | memory ids | `--instructions`, `--target-scope` | yes |

Memory is not a source adapter.

## Operational Commands

### jobs

| Command | Required | Optional | Result |
|---|---|---|---|
| `axon jobs list` | none | `--status`, `--kind`, `--limit`, `--cursor` | paged job summaries |
| `axon jobs get <job_id>` | job id | `--include`, `--include-events` | job detail |
| `axon jobs events <job_id>` | job id | `--after-sequence`, `--limit`, `--cursor` | event page |
| `axon jobs cancel <job_id>` | job id | `--reason` | cancellation result |
| `axon jobs retry <job_id>` | job id | `--from-phase`, `--idempotency-key` | new job descriptor |
| `axon jobs recover` | none | `--kind`, `--older-than` | recovery summary |
| `axon jobs cleanup` | none | `--older-than`, `--dry-run` | cleanup summary |
| `axon jobs clear` | none | `--status`, `--older-than`, `--confirm` | clear summary |

### artifacts and uploads

| Command | Required | Optional | Result |
|---|---|---|---|
| `axon artifacts list` | none | `--kind`, `--source-id`, `--job-id`, `--limit`, `--cursor` | artifact page |
| `axon artifacts get <artifact_id>` | artifact id | `--include-content-url` | artifact metadata |
| `axon artifacts content <artifact_id>` | artifact id | `--download`, `--range`, `--output` | content pointer/file |
| `axon uploads create <path>` | path | `--purpose`, `--source-hint` | upload descriptor |
| `axon uploads complete <upload_id>` | upload id | `--sha256`, `--source-options` | artifact/source ref |
| `axon uploads abort <upload_id>` | upload id | `--reason` | abort result |

### prune, collections, graph, providers

| Command | Required | Optional | Result |
|---|---|---|---|
| `axon prune plan <target>` | target | `--include`, `--retention`, `--filter` | prune plan |
| `axon prune exec <plan_id>` | plan id | `--confirm` | job descriptor |
| `axon prune dedupe` | none | `--collection`, `--threshold`, `--source-id`, `--dry-run` | summary/job |
| `axon prune purge <target>` | target | `--prefix`, `--dry-run`, `--confirm` | summary/job |
| `axon collections list` | none | none | collection summaries |
| `axon collections get <collection>` | collection | `--include-schema`, `--include-indexes` | collection detail |
| `axon graph kinds` | none | none | kind catalog |
| `axon graph resolve <identifier>` | identifier | `--kind`, `--limit` | graph matches |
| `axon graph query <query>` | query | `--limit`, `--cursor` | graph query result |
| `axon graph node <node_id>` | node id | `--include-edges`, `--include-evidence` | node detail |
| `axon graph edge <edge_id>` | edge id | `--include-evidence` | edge detail |
| `axon providers list` | none | `--kind`, `--status` | provider summaries |
| `axon providers get <provider>` | provider id | `--include-health`, `--include-limits` | provider detail |

### config, setup, reset, serve, mcp, palette

| Command | Required | Optional | Result |
|---|---|---|---|
| `axon config list` | none | `--source`, `--reveal` | effective config summary |
| `axon config get <key>` | key | `--source`, `--reveal` | config value |
| `axon config set <key> <value>` | key/value | `--env`, `--toml`, `--dry-run` | config edit plan/result |
| `axon config unset <key>` | key | `--env`, `--toml`, `--dry-run` | config edit plan/result |
| `axon config validate` | none | `--strict`, `--json` | config validation report |
| `axon setup config rewrite` | none | `--dry-run`, `--yes` | desired `.env`/`config.toml` rewrite plan/result |
| `axon setup compose` | none | `--profile`, `--dry-run` | compose command/plan |
| `axon setup sync` | none | `--dry-run` | local setup sync result |
| `axon setup update` | none | `--dry-run` | local setup update result |
| `axon preflight` | none | `--config`, `--providers`, `--json` | preflight report |
| `axon smoke` | none | `--live`, `--json` | smoke report |
| `axon reset` | none | `--stores`, `--dry-run`, `--yes`, `--receipt` | destructive reset plan/result |
| `axon serve` | none | `--bind`, `--port`, `--workers` | long-running server |
| `axon mcp` | none | `--transport`, `--bind`, `--port` | long-running MCP server |
| `axon palette status` | none | `--json` | desktop app status |
| `axon palette open` | none | `--target` | app launch result |

Rules:

- `migrate` is not part of the clean-slate target surface.
- setup/config commands may edit local files only after explicit command input.
- reset is required clean-slate tooling, not old-data migration.
- `axon doctor` remains the diagnostic command. `preflight` and `smoke` are
  explicit top-level operational checks, not `doctor` subcommands.
- `serve` and `mcp` are process entrypoints; their startup health is reported
  through status/doctor/provider/job contracts.

## Global Output Modes

Human mode is default. JSON mode is selected with `--json`.

Human mode:

- uses Aurora CLI tokens/colors where supported
- shows resolved source, adapter, scope, and reason
- shows whether embedding is enabled
- shows job id and watch id when backgrounded
- shows progress in foreground or with `--wait`
- shows warnings/degraded state clearly
- prints next commands for polling only when useful
- never hides fatal errors behind success-looking prose

JSON mode:

- emits one strict envelope for immediate results
- emits newline-delimited `SourceProgressEvent` objects for progress streams
- never emits human prose outside JSON
- includes `job_id`, `source_id`, `status`, `phase`, warnings, and errors
- uses the same result DTOs as MCP/REST

## CLI Response Envelope

JSON success:

```json
{
  "ok": true,
  "command": "source",
  "request_id": "req_...",
  "contract_version": "2026-06-30",
  "data": {},
  "job": null,
  "watch": null,
  "artifacts": [],
  "warnings": [],
  "pagination": null,
  "trace": {
    "job_id": "job_...",
    "trace_id": "trace_..."
  }
}
```

JSON failure:

```json
{
  "ok": false,
  "command": "source",
  "request_id": "req_...",
  "contract_version": "2026-06-30",
  "error": {
    "code": "source.resolve.unsupported",
    "message": "No adapter can resolve this source.",
    "stage": "resolving",
    "retryable": false,
    "severity": "failed",
    "details": {}
  },
  "warnings": [],
  "trace": {
    "job_id": null,
    "trace_id": "trace_..."
  }
}
```

## Background, Foreground, and Progress

Default behavior:

- fast read-only commands run foreground and return results
- mutating/long source jobs may enqueue and return immediately
- `--wait` blocks and renders progress until terminal state
- `--json --wait` streams progress events then final envelope
- `--json` without `--wait` returns a job descriptor immediately

Job descriptor:

```json
{
  "job_id": "job_...",
  "kind": "source",
  "status": "running",
  "phase": "embedding",
  "poll_after_ms": 1000,
  "poll": {
    "command": "axon jobs get job_..."
  },
  "events": {
    "command": "axon jobs events job_..."
  }
}
```

Progress event shape:

```json
{
  "event_id": "evt_...",
  "sequence": 42,
  "job_id": "job_...",
  "source_id": "src_...",
  "phase": "embedding",
  "status": "running",
  "severity": "info",
  "visibility": "public",
  "message": "embedding changed files",
  "timestamp": "2026-06-30T20:20:00Z",
  "counts": {
    "items_total": 1200,
    "items_done": 431,
    "chunks_total": 5200,
    "chunks_done": 1800,
    "bytes_total": 1234567,
    "bytes_done": 456789
  },
  "current": {
    "source_item_key": "src/lib.rs",
    "adapter": "github"
  }
}
```

## Pagination

CLI list commands use cursor pagination in JSON mode.

Flags:

```text
--limit <n>
--cursor <opaque_cursor>
```

Envelope:

```json
{
  "pagination": {
    "limit": 50,
    "next_cursor": "opaque_cursor_or_null",
    "has_more": true
  }
}
```

Human mode may show "next: ..." with the next command using the cursor.

## Response Modes and Artifacts

| Mode | Behavior |
|---|---|
| `inline` | Print full content when below size and visibility limits. |
| `summary` | Print concise summary plus ids/cursors/artifacts. |
| `artifact` | Write full output to ArtifactStore and print artifact refs. |
| `path` | Print local path/content pointer when safe. |
| `auto` | Choose inline for small output and artifact/path for large output. |

Large `retrieve`, `research`, `summarize`, `endpoints`, `screenshot`, and
`extract` outputs must use artifacts or files rather than silent truncation.

## Errors and Exit Codes

Exit codes:

| Code | Meaning |
|---:|---|
| 0 | success |
| 1 | generic failure |
| 2 | CLI parse or validation error |
| 3 | provider unavailable/degraded beyond allowed policy |
| 4 | auth/permission denied |
| 5 | source resolution/acquisition failure |
| 6 | job failed or canceled |
| 7 | output/write/artifact failure |
| 8 | removed command invoked |

Structured error fields:

| Field | Required | Meaning |
|---|---:|---|
| `code` | yes | Stable machine code. |
| `message` | yes | Redacted human message. |
| `stage` | yes | parse/resolve/acquire/prepare/embed/publish/etc. |
| `retryable` | yes | Whether retry may succeed. |
| `severity` | yes | warning/degraded/failed/fatal. |
| `details` | no | Redacted structured details. |

## Removed Commands

These must not remain user-facing commands, parser variants, help entries, or
completion entries:

```text
axon embed <source>
axon ingest <source>
axon scrape <url>
axon crawl <url>
axon code-search <query>
axon code-search-watch <path>
axon purge <target>
axon dedupe
axon refresh [filter]
axon fresh <sub>
```

`refresh` and `fresh` were missing from this enumeration even though
`delivery/surface-removal-contract.md`, `foundation/source-pipeline.md`, and
`plans/finish-unification-metaplan.md` all already treat them as removed
(`axon refresh ...` → `axon <source> --refresh` or a source operation;
`axon fresh ...` → `axon watch ...` or source freshness config). This was a
doc-sync gap in this file, not a scope decision — the canonical list above now
matches those contracts.

The final parser should treat removed commands like any other unknown command.
Documentation can explain the new command model, but runtime aliases/remap
handlers are not part of the contract.

## Help and Completions

`axon --help`, `axon help`, and `axon help <topic>` are contract surfaces.

Help must include:

- canonical commands
- source syntax examples
- global flags
- source flags
- search/query/retrieve/ask differences
- watch commands
- memory commands
- operational commands
- examples
- pointer to machine-readable capabilities

Shell completions must be generated from the same command registry, not a
separate hand-maintained list.

## Auth and Visibility

CLI auth uses the same policy as REST/MCP.

| Operation Class | Required Scope |
|---|---|
| status/help/capabilities | read |
| query/retrieve/ask/search/research/summarize | read |
| source/watch/upload/prune/memory mutation | write |
| destructive prune/purge/forget/hard delete/reset | write plus admin policy and confirmation |
| diagnostics revealing local config | local/admin policy |

Visibility rules:

- redact secrets in stdout, stderr, logs, progress, and JSON
- hide local absolute paths unless local policy permits them
- include artifact ids instead of raw large/sensitive content
- never print raw auth headers, cookies, tokens, signed URLs, or env values

## Crosswalk to MCP and REST

| CLI | MCP | REST | API DTO |
|---|---|---|---|
| `axon <source>` | `action=source` | `POST /v1/sources` | `SourceRequest` |
| `axon map <source>` | `action=map` | `POST /v1/map` | `SourceRequest` |
| `axon search <query>` | `action=search` | `POST /v1/search` | `SearchRequest` |
| `axon query <query>` | `action=query` | `POST /v1/query` | `QueryRequest` |
| `axon retrieve <source-or-url>` | `action=retrieve` | `POST /v1/retrieve` | `RetrievalRequest` |
| `axon ask <question>` | `action=ask` | `POST /v1/ask` | `AskRequest` |
| `axon chat <message>` | `action=chat` | `POST /v1/chat` | `ChatRequest` |
| `axon evaluate <question>` | `action=evaluate` | `POST /v1/evaluate` | `EvaluationRequest` |
| `axon suggest [focus]` | `action=suggest` | `POST /v1/suggest` | `SuggestRequest` |
| `axon research <query>` | `action=research` | `POST /v1/research` | `ResearchRequest` |
| `axon summarize <source>` | `action=summarize` | `POST /v1/summarize` | `SummarizeRequest` |
| `axon endpoints <source>` | `action=endpoints` | `POST /v1/endpoints` | `EndpointDiscoveryRequest` |
| `axon brand <source>` | `action=brand` | `POST /v1/brand` | `BrandRequest` |
| `axon diff <a> <b>` | `action=diff` | `POST /v1/diff` | `DiffRequest` |
| `axon screenshot <source>` | `action=screenshot` | `POST /v1/screenshot` | `ScreenshotRequest` |
| `axon extract <source>` | `action=extract` | `POST /v1/extract` | `ExtractRequest` |
| `axon memory <sub>` | `action=memory` | `/v1/memories/*` | `Memory*` |
| `axon jobs <sub>` | `action=jobs` | `/v1/jobs/*` | `Job*` |
| `axon watch <sub>` | `action=watches` | `/v1/watches/*` | `Watch*` |
| `axon artifacts <sub>` | `action=artifacts` | `/v1/artifacts/*` | `Artifact*` |
| `axon uploads <sub>` | `action=uploads` | `/v1/uploads/*` | `Upload*` |
| `axon prune <sub>` | `action=prune` | `/v1/prune/*` | `Prune*` |
| `axon collections <sub>` | `action=collections` | `/v1/collections/*` | `Collection*` |
| `axon graph <sub>` | `action=graph` | `/v1/graph/*` | `Graph*` |
| `axon providers <sub>` | `action=providers` | `/v1/providers/*` | `Provider*` |
| `axon reset` | `action=reset` | `/v1/reset/*` | `Reset*` |
| `axon capabilities` | `action=capabilities` | `GET /v1/capabilities` | `CapabilityDocument` |
| `axon status` | `action=status` | `GET /v1/status` | `StatusReport` |
| `axon doctor` | `action=doctor` | `GET /v1/doctor` | `DoctorReport` |

## Validation Checklist

Implementation is incomplete until all of these pass:

- `axon --help` matches `axon-help.md`.
- The command registry drives help and shell completions.
- Every command maps to an `axon-api` DTO.
- Removed commands are absent from help/completions and cannot dispatch.
- `search`, `query`, `retrieve`, and `ask` obey their boundary rules.
- `axon <source>` is the only source acquisition happy path.
- `axon map` uses `scope=map` and `embed=false`.
- `watch exec` is the only immediate-run watch spelling.
- `--json` emits strict JSON with no human prose.
- `--wait` streams progress and returns final status.
- Background work always returns a job/watch descriptor.
- Large output uses artifacts, files, cursors, or explicit truncation warnings.
- Public output follows `metadata-payload.md` redaction rules.
- CLI/MCP/REST crosswalk stays aligned.
