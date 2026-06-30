# Target `axon --help` Contract
Last Modified: 2026-06-30

This is the desired end-state help text, hand-written as a contract before
implementation.

## Current Implementation Snapshot

Implemented today:

- The current CLI is still subcommand-first and requires a command.
- Current help still includes implemented commands such as `scrape`, `crawl`,
  `embed`, `ingest`, `code-search`, `code-search-watch`, `purge`, `dedupe`,
  `fresh`, and `refresh`.
- The target `axon <source>` default path, grouped `jobs`, grouped `artifacts`,
  grouped `uploads`, grouped `prune`, grouped `graph`, and grouped `providers`
  surfaces are not the current parser contract.

Planned by this contract:

- Once the clean break lands, `axon --help` and `axon help` should render the
  end-state command model below and omit removed legacy commands.

```text
Axon

Acquire, normalize, embed, refresh, and search source knowledge.

Usage:
  axon <source> [options]
  axon map <source> [options]
  axon watch <source> [options]
  axon extract <source> --schema <schema> [options]
  axon ask <question> [options]
  axon chat <message> [options]
  axon query <query> [options]
  axon retrieve <source-or-url> [options]
  axon search <query> [options]
  axon research <query> [options]
  axon summarize <source> [options]
  axon evaluate <question> [options]
  axon suggest [focus] [options]
  axon endpoints <source> [options]
  axon brand <source> [options]
  axon diff <source-a> <source-b> [options]
  axon screenshot <source> [options]
  axon memory <command> [options]
  axon jobs <command> [options]
  axon artifacts <command> [options]
  axon uploads <command> [options]
  axon prune <command> [options]
  axon collections <command> [options]
  axon graph <command> [options]
  axon providers <command> [options]
  axon preflight [options]
  axon smoke [options]
  axon capabilities [options]
  axon status [options]
  axon doctor [options]
  axon help [topic]

Source examples:
  axon https://ui.shadcn.com/docs
  axon shadcn.com --refresh
  axon github.com/shadcn-ui/ui
  axon shadcn-ui/ui
  axon /home/jmagar/workspace/axon --watch
  axon crates:serde
  axon npm:@modelcontextprotocol/sdk
  axon pypi:fastapi
  axon r/rust
  axon https://youtube.com/watch?v=...
  axon https://example.com/feed.xml

Source options:
  --scope <scope>       Adapter-declared acquisition scope
  --watch               Keep this source fresh
  --no-embed            Acquire and normalize without vector storage
  --refresh             Force refresh even if the source appears current
  --wait                Wait for the current run to complete
  --json                Emit machine-readable output
  --collection <name>   Vector collection
  --adapter <name>      Force a source adapter when supported

Discovery:
  axon map <source>     Discover source items/URLs without embedding

Common scopes:
  page                  One web page
  site                  Crawl a site or docs subtree
  docs                  Resolve and crawl official docs
  repo                  Source repository
  org                   Repository/package organization
  package               Registry package
  subreddit             Reddit subreddit
  video                 YouTube video
  playlist              YouTube playlist
  channel               YouTube channel

Watch commands:
  axon watch <source>             Create or ensure a watch
  axon watch exec <source>        Run a watch target now
  axon watch list                 List watches
  axon watch get <watch_id>       Show watch configuration
  axon watch status <watch_id>    Show watch heartbeat/progress
  axon watch pause <watch_id>     Pause a watch
  axon watch resume <watch_id>    Resume a watch
  axon watch delete <watch_id>    Delete a watch
  axon watch history <watch_id>   Show recent watch runs

Search commands:
  axon ask <question>             Retrieve indexed context, then synthesize an answer
  axon chat <message>             Direct LLM chat without retrieval
  axon query <query>              Search indexed vectors and return ranked chunks
  axon retrieve <source-or-url>   Return stored chunks/docs for a known source
  axon search <query>             Search external web results, not indexed chunks
  axon research <query>           Search, fetch, and synthesize with citations
  axon summarize <source>         Fetch and summarize without indexing
  axon evaluate <question>        Compare RAG output against a baseline/judge
  axon suggest [focus]            Suggest source targets or next acquisition work

Search command differences:
  search      External web discovery. Calls SearchProvider. Does not read vectors.
  query       Indexed semantic retrieval. Reads VectorStore. Does not call an LLM.
  retrieve    Identity lookup. Reads stored docs/chunks for a known source/url/id.
  ask         RAG answer. Reads indexed context, then calls LlmProvider.
  chat        Direct LLM call. Does not retrieve or index context.
  research    Search/fetch/synthesis over external sources. Optional indexing only
              when requested.
  summarize   Fetch/scrape plus synthesis. Does not index unless routed through
              the source pipeline.
  evaluate    Retrieval and LLM judge workflow for measuring answer quality.
  suggest     Recommendation workflow for source discovery and follow-up.

Inspection commands:
  axon endpoints <source>         Discover API/network endpoints
  axon brand <source>             Extract brand colors/fonts/assets
  axon diff <source-a> <source-b> Compare two sources
  axon screenshot <source>        Capture a screenshot artifact

Memory commands:
  axon memory remember <text>                     Store a durable memory
  axon memory search <query>                      Search durable memories
  axon memory context <prompt-or-source>          Build bounded memory context
  axon memory show <memory_id>                    Show one memory
  axon memory link <memory_id> <target>           Link memory to a source/graph node
  axon memory supersede <old_id> <new_id>         Replace an old memory
  axon memory reinforce <memory_id>               Record successful memory use
  axon memory contradict <id> <other_id>          Mark conflicting memories
  axon memory pin <memory_id>                     Keep memory highly recallable
  axon memory archive <memory_id>                 Hide memory without forgetting
  axon memory forget <memory_id>                  Remove memory from recall
  axon memory review                              Show memory review queue
  axon memory compact <memory_id>...              Distill memories into one

Operations:
  axon jobs list                  List source/prune/extract/retrieval jobs
  axon jobs get <job_id>          Show job status
  axon jobs events <job_id>       Show job progress events
  axon jobs cancel <job_id>       Cancel a job
  axon jobs retry <job_id>        Retry from submitted config snapshot
  axon jobs recover               Recover stale jobs
  axon jobs cleanup               Cleanup old terminal jobs/events
  axon jobs clear                 Clear terminal jobs after confirmation
  axon artifacts list             List artifacts
  axon artifacts get <artifact_id> Show artifact metadata/content pointer
  axon uploads create <path>      Stage a file/session/source upload
  axon prune plan <target>        Create a dry-run prune plan
  axon prune exec <plan_id>       Execute a prune plan
  axon prune dedupe               Deduplicate vector chunks
  axon prune purge <target>       Purge indexed content through prune
  axon collections list           List vector collections
  axon collections get <name>     Show collection detail
  axon graph kinds                List graph node/edge/evidence kinds
  axon graph resolve <identifier> Resolve a source/package/repo/session id
  axon graph query <query>        Query SourceGraph relationships
  axon graph node <node_id>       Show a graph node
  axon graph edge <edge_id>       Show a graph edge
  axon providers list             List provider capabilities/health
  axon providers get <provider>   Show provider capability/health
  axon config list                Show effective config
  axon config get <key>           Show one config/env value
  axon config set <key> <value>   Write a config/env value after validation
  axon config unset <key>         Remove a config/env value after validation
  axon config validate            Validate .env/config.toml placement and values
  axon setup config rewrite       Rewrite desired config shape after confirmation
  axon setup compose              Print/run compose helper plan
  axon setup sync                 Sync local setup-owned files
  axon setup update               Update local setup-owned runtime helpers
  axon preflight                  Check host/config/provider readiness
  axon smoke                      Run explicit provider/source smoke checks
  axon reset                      Destructively reset selected local stores
  axon serve                      Start REST/MCP/web/workers
  axon mcp                        Start MCP server mode
  axon palette status             Show Palette desktop app status
  axon palette open               Open Palette desktop app
  axon capabilities               Show complete capability document

Extraction:
  axon extract <source> --schema <schema>
      Run structured LLM extraction. This is not the indexing pipeline.

Status:
  axon status                     Show source jobs, watches, and services
  axon doctor                     Check local services and configuration

`axon doctor` is diagnostics only. `axon preflight` and `axon smoke` are
separate top-level operational checks, not doctor subcommands.

Run `axon help sources` for adapter scopes and source forms.
Run `axon help mcp` for the MCP tool contract.
Run `axon help json` for machine-readable response shapes.
```

## `axon help sources`

```text
Sources

Axon resolves source strings through adapter capabilities.

Resolution order:
  1. explicit scheme or prefix
  2. existing local path
  3. adapter shorthand
  4. scheme-less web host
  5. ambiguous/unknown source error

Adapters:
  web          page, site, docs, map
  local        file, directory, workspace, repo, map
  github       repo, branch, commit, issues, prs, wiki, org, map
  gitlab       repo, branch, commit, issues, mrs, wiki, group, map
  gitea        repo, branch, commit, issues, prs, wiki, org, map
  git          repo, branch, commit, map
  crates       package, version, owner, docs, dependencies, map
  npm          package, version, scope, docs, dependencies, map
  pypi         package, version, docs, dependencies, map
  docker       image, tag, namespace, manifest, map
  reddit       subreddit, thread, user, search, map
  youtube      video, playlist, channel, captions, map
  feed         feed, entry, site, map
  sessions     project, provider, file, upload, map
  cli_tool     tool, script, command, run, help, schema, map
  mcp_tool     server, tool, resource, prompt, schema, call, map

Every acquired source embeds by default unless --no-embed is set.
Memory is not a source adapter; use `axon memory ...` for durable memory
lifecycle.
```
