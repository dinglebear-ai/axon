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
│   ├── actions/              #   action reference — one file per CLI/API/MCP action
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

## The Split: actions/ vs ingest/

These two directories cover different readers and different questions. `docs/reference/actions/` is the reference for every action across CLI, REST, and MCP surfaces. `docs/guides/ingest/` is only for the ingest pipeline and real source deep-dives (`ingest`, `GitHub`, `reddit`, `sessions`, `YouTube`).

### `docs/reference/actions/` — "How do I invoke this action?"

The action reference. Written for someone who needs to know how the same operation maps across the terminal, direct `/v1` REST routes, and the MCP `axon` tool.

**Belongs here:**
- Generated `Surfaces` block from `scripts/generate_action_docs.py`
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
- Deep implementation details beyond the generated service entry point (function internals, data structures)

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
- CLI flags table (→ actions/)
- Job subcommand reference (→ actions/)
- Usage examples with `axon <cmd> <args>` (→ actions/)
- Async behavior / `--wait` explanation (→ actions/)

### Cross-Linking Rule

Only actions with real ingest deep-dives link to `docs/guides/ingest/`. Use a
real file path when one exists; do not leave placeholder markdown links in
checked-in docs.

Every `ingest/` file opens with a back-link to its real `actions/`
counterpart.

Do not create tiny action stubs in `docs/guides/ingest/` for actions such as `ask`, `doctor`, `domains`, `embed`, `evaluate`, `query`, `retrieve`, `setup`, `sources`, `stats`, or `suggest`. Keep their operational notes in `docs/reference/actions/<name>.md` unless they grow into a true deep-dive.

---

## Other Directories

### `docs/reference/actions/` — all actions

Each action gets one file. For actions that don't have a paired ingest doc (e.g. `ask.md`, `search.md`, `research.md`), use the same structure: generated surfaces → synopsis → flags → subcommands → examples → notes.

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

- **New action?** → Add `docs/reference/actions/<name>.md`, update `docs/reference/api-parity.md`, then run `python3 scripts/generate_action_docs.py`.
- **New ingest source?** → Add both `docs/reference/actions/<name>.md` and `docs/guides/ingest/<name>.md` with cross-links.
- **Implementation plan?** → `docs/plans/<feature>.md`.
- **Session summary?** → `docs/sessions/YYYY-MM-DD-HH-MM-<description>.md` via `save-to-md`.

Keep docs accurate to the code. If you change a flag name, default value, or scan path — update the doc in the same commit.
