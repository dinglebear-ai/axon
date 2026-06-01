# docs/ — Documentation Structure
Last Modified: 2026-06-01

All project documentation lives here. This file defines the layout and the rules for what goes where.

## Directory Layout

Living/reference docs are grouped by intent; dated point-in-time records live in the history
directories at the bottom. See [`README.md`](README.md) for the full annotated map.

```
docs/
├── guides/                   # Getting started + task-oriented how-to
│   ├── getting-started.md    #   setup for local dev and Docker
│   ├── configuration.md      #   config.toml + environment variables
│   ├── ask-rag.md            #   the ask RAG pipeline
│   ├── reindexing.md         #   re-indexing + payload schema upgrades
│   ├── context-injection.md  #   context-injection mechanics
│   └── ingest/               #   ingest pipeline + per-source deep-dives
├── reference/                # Factual reference
│   ├── commands/             #   CLI reference — one file per command
│   ├── mcp/                  #   MCP overview, tool-schema, transport, connect, deploy, env, tools, patterns
│   ├── http-api.md           #   HTTP API surface
│   ├── api-parity.md         #   CLI/MCP/HTTP action parity
│   ├── endpoints.md          #   endpoint discovery
│   ├── shell-completions.md  #   shell completion generation
│   ├── cargo-features.md     #   Cargo feature matrix
│   ├── spider-feature-flags.md
│   ├── job-lifecycle.md      #   async job state machine (SQLite-backed)
│   ├── inventory.md          #   component + command inventory
│   ├── qdrant-payload-schema.md
│   └── env-matrix.md/.toml   #   env var migration matrix
├── architecture/             # System design
│   ├── overview.md           #   architecture diagrams + data flow
│   ├── stack/                #   trimodal arch, tech choices, prerequisites
│   └── specs/                #   feature specifications
├── operations/               # Running it in production
│   ├── deployment.md
│   ├── operations.md
│   ├── performance.md
│   ├── security.md
│   └── auth/                 #   MCP auth + static API token
├── contributing/             # Development + repo conventions
│   ├── rust.md
│   ├── testing.md
│   ├── monolith-policy.md
│   ├── guardrails.md
│   ├── checklist.md
│   ├── feature-delivery-framework.md
│   ├── desktop-palette-testing.md
│   └── repo/                 #   repo tree, rules, recipes, scripts, memory
│
│   # History — dated records, NOT kept up to date:
├── sessions/                 # Session logs: YYYY-MM-DD-HH-MM-description.md
├── reports/                  # Code reviews, audits, analysis
├── plans/                    # Implementation plans (plans/complete/ = archived)
├── superpowers/              # Superpowers plans/specs
├── perf/                     # Dated performance snapshots
├── eval/                     # Evaluation datasets and fixtures
├── palette-demo/             # Desktop palette testing demo assets
└── archive/                  # Historical removed-runtime docs (do not edit)
```

---

## The Split: commands/ vs ingest/

These two directories cover different readers and different questions. `docs/reference/commands/` is the CLI reference for every command. `docs/guides/ingest/` is only for the ingest pipeline and real source deep-dives (`ingest`, `GitHub`, `reddit`, `sessions`, `YouTube`).

### `docs/reference/commands/` — "How do I use this command?"

The CLI reference. Written for someone at a terminal who needs to know what flags exist, what subcommands are available, and how to run common tasks.

**Belongs here:**
- Synopsis / usage line
- Arguments table
- All flags and their defaults (including command-specific flags)
- Job subcommands (`status`, `cancel`, `list`, `cleanup`, `clear`, `recover`, `worker`)
- Concrete usage examples
- Required environment variables (brief — what to set, not why)
- One-line install instructions for external dependencies, linking to an ingest deep-dive only when one exists

**Does not belong here:**
- Step-by-step pipeline internals ("first it calls X, then Y…")
- Troubleshooting sections for ingest sources (→ ingest/)
- Known limitations tables for ingest sources (→ ingest/)
- Implementation details (function names, data structures)

### `docs/guides/ingest/` — "How does this work / how do I set it up?"

The implementation and operations reference for the ingest pipeline and supported ingest sources. Written for someone debugging a failure, setting up a new environment, or contributing to the ingest code.

**Belongs here:**
- Prerequisites with full installation instructions (Docker + local dev)
- What actually gets indexed (detailed, with conditions and exclusions)
- Step-by-step pipeline walkthrough with function names and code references
- Known limitations table (with root causes)
- Troubleshooting section (error messages → solutions)
- Environment variables (full list including optional infra vars like `TEI_URL`, `AXON_COLLECTION`)
- Developer guide (e.g. "Adding a new session format")

**Does not belong here:**
- CLI flags table (→ commands/)
- Job subcommand reference (→ commands/)
- Usage examples with `axon <cmd> <args>` (→ commands/)
- Async behavior / `--wait` explanation (→ commands/)

### Cross-Linking Rule

Only commands with real ingest deep-dives link to `docs/guides/ingest/`:
```markdown
> For implementation details and troubleshooting see [`docs/guides/ingest/<name>.md`](../ingest/<name>.md).
```

Every `ingest/` file opens with a back-link to its `commands/` counterpart:
```markdown
> CLI reference (flags, subcommands, examples): [`docs/reference/commands/<name>.md`](../commands/<name>.md)
```

Do not create tiny command stubs in `docs/guides/ingest/` for commands such as `ask`, `doctor`, `domains`, `embed`, `evaluate`, `query`, `retrieve`, `setup`, `sources`, `stats`, or `suggest`. Keep their operational notes in `docs/reference/commands/<name>.md` unless they grow into a true deep-dive.

---

## Other Directories

### `docs/reference/commands/` — all commands

Each command gets one file. For commands that don't have a paired ingest doc (e.g. `ask.md`, `search.md`, `research.md`), use the same structure: synopsis → flags → subcommands → examples → notes.

### `docs/plans/`

Implementation plans generated during development. Format: free-form markdown, named by feature (e.g. `crawl-performance.md`). Move to `docs/plans/complete/` when the plan is fully executed. Never delete — plans are the written record of why things are the way they are.

### `docs/sessions/`

Session logs: `YYYY-MM-DD-HH-MM-description.md`. Generated by `save-to-md` skill at session end. These capture decisions, root causes, and context for future sessions and agents.

### `docs/reports/`

Code reviews, audits, security analysis. Named by date and scope: `2026-02-22-full-review.md`.

### Database schema

The SQLite schema is auto-created by migrations under `src/jobs/migrations/` plus store helpers in `src/jobs/store.rs`. There is no separate hand-maintained schema doc.

---

## Writing New Docs

- **New command?** → Add `docs/reference/commands/<name>.md` following the template above.
- **New ingest source?** → Add both `docs/reference/commands/<name>.md` and `docs/guides/ingest/<name>.md` with cross-links.
- **Implementation plan?** → `docs/plans/<feature>.md`.
- **Session summary?** → `docs/sessions/YYYY-MM-DD-HH-MM-<description>.md` via `save-to-md`.

Keep docs accurate to the code. If you change a flag name, default value, or scan path — update the doc in the same commit.
