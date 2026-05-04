# Repository Structure -- Axon

## Directory tree

```
axon_rust/
├── apps/
│   └── web/                         # Next.js dashboard (Pulse workspace, omnibox)
│       ├── app/                     # Next.js app router pages
│       ├── components/              # React components
│       ├── hooks/                   # React hooks (WebSocket, API)
│       ├── lib/                     # Shared utilities
│       ├── proxy.ts                 # API proxy with token auth
│       ├── shell-server.mjs         # Shell WebSocket server
│       ├── biome.json               # Linter/formatter config
│       ├── package.json             # Node dependencies
│       └── CLAUDE.md                # Web-specific development instructions
│
├── crates/                          # Rust workspace crates (module roots)
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
│   ├── jobs/                        # Async job framework
│   │   ├── common/                  # Shared job infrastructure
│   │   ├── crawl/                   # Crawl job processor, worker, runtime
│   │   ├── extract/                 # Extract job processor
│   │   ├── embed/                   # Embed job processor
│   │   └── ingest.rs               # Ingest job processor
│   ├── mcp.rs                       # MCP module root
│   ├── mcp/                         # MCP schema and server
│   │   ├── schema.rs               # Tool input schema, action enums
│   │   └── server.rs               # Handler dispatch, transport setup
│   ├── services.rs                  # Services module root
│   ├── services/                    # Typed service layer
│   │   ├── context.rs              # ServiceContext and capabilities
│   │   ├── types/                  # Result structs
│   │   ├── acp/                    # ACP session lifecycle
│   │   └── acp_llm/               # ACP-backed LLM completions
│   ├── vector.rs                    # Vector module root
│   ├── vector/                      # Qdrant ops, TEI, hybrid search
│   │   └── ops/                    # TEI embed, Qdrant upsert/search, ask
│   ├── web.rs                       # WebSocket execution bridge
│   └── web/                         # Web-specific handlers
│
├── docker/                          # Container builds and s6 supervision
│   ├── Dockerfile                   # Multi-stage: cargo-chef -> build -> runtime
│   ├── chrome/                      # Headless Chrome + CDP proxy
│   ├── web/                         # Next.js + s6-overlay
│   │   ├── Dockerfile
│   │   ├── cont-init.d/            # Container init scripts
│   │   └── s6-rc.d/                # s6 service definitions
│   ├── rabbitmq/                    # (legacy — no longer used)
│   ├── s6/                          # Worker s6 service definitions
│   │   ├── cont-init.d/
│   │   └── s6-rc.d/
│   └── CLAUDE.md                    # Docker build instructions
│
├── docs/                            # Documentation (this directory)
├── migrations/                      # SQL migrations
├── scripts/                         # Maintenance, hooks, testing scripts
├── tests/                           # Integration tests
├── config/                          # Additional config files
├── specs/                           # Specifications
│
├── main.rs                          # Binary entry point
├── lib.rs                           # Library root (run/run_once, command dispatch)
├── crates.rs                        # Workspace crate re-exports
├── Cargo.toml                       # Rust package manifest
├── Cargo.lock                       # Dependency lock file
├── Justfile                         # Task runner recipes
├── config.example.toml              # Annotated template — copy to ~/.axon/config.toml
├── lefthook.yml                     # Git hooks
├── deny.toml                        # cargo-deny config
├── renovate.json                    # Dependency update bot
├── rust-toolchain.toml              # Rust 1.94.0 pinned toolchain
│
├── docker-compose.yaml              # App containers (workers + web)
├── docker-compose.services.yaml     # Infrastructure services
├── docker-compose.gpu.yaml          # GPU override for TEI/Ollama
├── docker-compose.test.yaml         # Test infrastructure
├── .env.example                     # Environment variable template
├── services.env                     # Infrastructure container credentials
│
├── CLAUDE.md                        # Project instructions for Claude Code
├── AGENTS.md -> CLAUDE.md           # Codex agent alias
├── GEMINI.md -> CLAUDE.md           # Gemini agent alias
├── README.md                        # User-facing documentation
└── CHANGELOG.md                     # Version history
```

## Workspace crates

Axon uses a flat module layout rooted at `crates/`. Each crate has a module root file (`crates/<name>.rs`) and a subdirectory (`crates/<name>/`).

| Crate | Module root | Purpose |
|-------|------------|---------|
| cli | `crates/cli.rs` | CLI command handlers -- one file per subcommand |
| core | `crates/core.rs` | Config parsing, HTTP client, content/markdown processing |
| crawl | `crates/crawl.rs` | Spider-based crawl engine, render mode switching |
| ingest | `crates/ingest.rs` | Source adapters (GitHub, Reddit, YouTube) |
| jobs | `crates/jobs.rs` | Async job framework with AMQP and SQLite backends |
| mcp | `crates/mcp.rs` | MCP server schema definition and handler dispatch |
| services | `crates/services.rs` | Typed service layer consumed by CLI, MCP, and web |
| vector | `crates/vector.rs` | Qdrant operations, TEI embedding, hybrid search |
| web | `crates/web.rs` | WebSocket execution bridge for web UI |

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

Enforced by `scripts/check_no_mod_rs.sh`.

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
| `docker-compose.services.yaml` | Qdrant, TEI, Chrome | `axon` bridge |
| `docker-compose.gpu.yaml` | NVIDIA GPU reservations overlay | -- |
