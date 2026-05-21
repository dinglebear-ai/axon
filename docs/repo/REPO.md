# Repository Structure -- Axon

## Directory tree

```
axon_rust/
├── apps/
│   └── web/                         # Static setup/config panel source
│       ├── app/                     # Next static app files
│       ├── package.json             # Node dependencies
│       └── CLAUDE.md                # Web-specific development instructions
│
├── src/                             # Rust module roots and submodules
│   ├── cli.rs                       # CLI module root
│   ├── cli/                         # Command handlers
│   │   └── commands/                # Per-command handlers (scrape, crawl, ask, etc.)
│   ├── core.rs                      # Core module root
│   ├── core/                        # Config, HTTP client, content processing
│   │   └── config/                  # CLI flags, env parsing, runtime config
│   ├── crawl.rs                     # Crawl module root
│   ├── crawl/                       # Spider-based crawl engine
│   ├── ingest.rs                    # Ingest module root
│   ├── ingest/                      # GitHub, Reddit, YouTube adapters
│   ├── jobs.rs                      # Jobs module root
│   ├── jobs/                        # SQLite job runtime and workers
│   │   ├── runtime.rs               # SQLite runtime module root
│   │   ├── workers.rs               # In-process worker loops
│   │   ├── store.rs                 # SQLite schema and lifecycle helpers
│   │   ├── ops.rs                   # Job state transition helpers
│   │   ├── query.rs                 # Job query helpers
│   │   └── watch.rs                 # Recurring watch scheduler
│   ├── mcp.rs                       # MCP module root
│   ├── mcp/                         # MCP schema and server
│   │   ├── schema.rs               # Tool input schema, action enums
│   │   └── server.rs               # Handler dispatch, transport setup
│   ├── services.rs                  # Services module root
│   ├── services/                    # Typed service layer
│   │   ├── context.rs              # ServiceContext
│   │   ├── types/                  # Result structs
│   │   ├── llm_backend/            # Gemini headless completions
│   ├── vector.rs                    # Vector module root
│   ├── vector/                      # Qdrant ops, TEI, hybrid search
│   │   └── ops/                    # TEI embed, Qdrant upsert/search, ask
│   ├── web.rs                       # Unified HTTP server module root
│   └── web/                         # Static panel, /v1/ask, /v1/actions
│
├── docs/                            # Documentation (this directory)
├── migrations/                      # SQL migrations
├── scripts/                         # Maintenance, hooks, testing scripts
├── tests/                           # Integration tests
├── config/                          # Compose, Chrome, Qdrant, and MCP config files
├── specs/                           # Specifications
│
├── main.rs                          # Binary entry point
├── lib.rs                           # Library root (run/run_once, command dispatch)
├── Cargo.toml                       # Rust package manifest
├── Cargo.lock                       # Dependency lock file
├── Justfile                         # Task runner recipes
├── config.example.toml              # Annotated template — copy to ~/.axon/config.toml
├── lefthook.yml                     # Git hooks
├── deny.toml                        # cargo-deny config
├── renovate.json                    # Dependency update bot
├── rust-toolchain.toml              # Rust 1.94.0 pinned toolchain
│
├── docker-compose.prod.yaml         # Axon server + infrastructure services
├── docker-compose.yaml              # Local development stack
├── .env.example                     # Environment variable template
│
├── CLAUDE.md                        # Project instructions for Claude Code
├── AGENTS.md -> CLAUDE.md           # Codex agent alias
├── GEMINI.md -> CLAUDE.md           # Gemini agent alias
├── README.md                        # User-facing documentation
└── CHANGELOG.md                     # Version history
```

## Runtime modules

Axon uses a flat module layout rooted at `src/`. Each module has a module root file (`src/<name>.rs`) and a subdirectory (`src/<name>/`).

| Module | Module root | Purpose |
|-------|------------|---------|
| cli | `src/cli.rs` | CLI command handlers -- one file per subcommand |
| core | `src/core.rs` | Config parsing, HTTP client, content/markdown processing |
| crawl | `src/crawl.rs` | Spider-based crawl engine, render mode switching |
| ingest | `src/ingest.rs` | Source adapters (GitHub, Reddit, YouTube, sessions) |
| jobs | `src/jobs.rs` | SQLite-backed async job framework with in-process workers |
| mcp | `src/mcp.rs` | MCP server schema definition and handler dispatch |
| services | `src/services.rs` | Typed service layer consumed by CLI, MCP, and HTTP routes |
| vector | `src/vector.rs` | Qdrant operations, TEI embedding, hybrid search |
| web | `src/web.rs` | Unified HTTP server for panel, `/v1/ask`, and `/v1/actions` |

### Module layout convention (enforced)

Rust 2018+ file-per-module layout. `mod.rs` is forbidden:

```
# Correct
foo.rs          <- module root
foo/
  bar.rs        <- submodule

# Wrong
foo/
  mod.rs        <- forbidden
```

Enforced by `cargo xtask check-no-mod-rs`.

Do not use `#[path = "..."]` to route around this layout in production modules.
If a temporary path attribute is needed during a file split, remove it before
landing the change and add the new module under the standard `foo.rs` plus
`foo/bar.rs` structure. A current-tree check should return no production
`#[path]` attributes outside historical docs.

## Root files

| File | Required | Purpose |
|------|----------|---------|
| `CLAUDE.md` | Yes | Project instructions (37K, comprehensive) |
| `README.md` | Yes | User-facing documentation (55K) |
| `CHANGELOG.md` | Yes | Version history |
| `.env.example` | Yes | Environment template (150+ variables) |
| `Justfile` | Yes | Task runner (30+ recipes) |
| `config.example.toml` | Yes | Annotated config template (copy to `~/.axon/config.toml`) |
| `Cargo.toml` | Yes | Rust package manifest |
| `main.rs` | Yes | Binary entry point |
| `lib.rs` | Yes | Library root with command dispatch |

## Docker compose files

| File | Contents | Network |
|------|----------|---------|
| `docker-compose.prod.yaml` | Axon server, Qdrant, TEI, Chrome | `axon` bridge |
| `docker-compose.yaml` | Local development stack with bind-mounted Axon debug binary | `axon` bridge |
