# docs/ — Documentation Structure
Last Modified: 2026-03-03

All project documentation lives here. This file defines the layout and the rules for what goes where.

## Directory Layout

```
docs/
├── commands/                 # CLI command reference — one file per command
├── ingest/                   # Ingest system docs — one file per ingest source
├── plans/                    # Active implementation plans (move to plans/complete/ when done)
│   └── complete/             # Archived plans
├── reports/                  # Code reviews, audits, analysis
├── screenshots/              # UI screenshots and visual references
├── sessions/                 # Session logs: YYYY-MM-DD-HH-MM-description.md
│
├── ACP.md                    # ACP protocol: one-shot vs persistent, wire format, session cache, prewarming
├── ARCHITECTURE.md           # System architecture diagrams and data-flow
├── API.md                    # HTTP API reference for axon serve
├── CLAUDE-HOT-RELOAD.md      # Claude config hot-reload in axon-web (watched paths, verification)
├── DEPLOYMENT.md             # Production deployment guide
├── FEATURE-DELIVERY-FRAMEWORK.md  # Feature development process
├── HEADLESS_OPTIONS.md       # Chrome/headless rendering configuration options
├── JOB-LIFECYCLE.md          # Job state machine and lifecycle diagrams (SQLite-backed)
├── LIVE-TEST-SCRIPTS.md      # Live integration test scripts reference
├── MCP.md                    # MCP server runtime and design guide
├── MCP-TOOL-SCHEMA.md        # MCP wire contract — source of truth for the axon tool schema
├── OPERATIONS.md             # Runbook: common operational tasks and recovery procedures
├── PERFORMANCE.md            # Performance tuning guide and benchmark results
├── SCHEMA.md                 # Full database schema reference (auto-created tables)
├── SECURITY.md               # Security model, SSRF guards, ALLOWED_MODES/ALLOWED_FLAGS
├── SERVE.md                  # axon serve web UI and WebSocket bridge reference
├── TESTING.md                # Test strategy, how to run, coverage targets
└── UI-DESIGN-SYSTEM.md       # apps/web design system tokens and component conventions
```

---

## The Split: commands/ vs ingest/

These two directories cover the four ingest commands (`github`, `reddit`, `sessions`, `youtube`). They serve different readers and different questions.

### `docs/commands/` — "How do I use this command?"

The CLI reference. Written for someone at a terminal who needs to know what flags exist, what subcommands are available, and how to run common tasks.

**Belongs here:**
- Synopsis / usage line
- Arguments table
- All flags and their defaults (including command-specific flags)
- Job subcommands (`status`, `cancel`, `list`, `cleanup`, `clear`, `recover`, `worker`)
- Concrete usage examples
- Required environment variables (brief — what to set, not why)
- One-line install instructions for external dependencies (link to ingest/ for details)

**Does not belong here:**
- Step-by-step pipeline internals ("first it calls X, then Y…")
- Troubleshooting sections (→ ingest/)
- Known limitations tables (→ ingest/)
- Implementation details (function names, data structures)

### `docs/ingest/` — "How does this work / how do I set it up?"

The implementation and operations reference. Written for someone debugging a failure, setting up a new environment, or contributing to the ingest code.

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

Every `commands/` file links to its `ingest/` counterpart:
```markdown
> For implementation details and troubleshooting see [`docs/ingest/<name>.md`](../ingest/<name>.md).
```

Every `ingest/` file opens with a back-link to its `commands/` counterpart:
```markdown
> CLI reference (flags, subcommands, examples): [`docs/commands/<name>.md`](../commands/<name>.md)
```

---

## Other Directories

### `docs/commands/` — all commands

Each command gets one file. For commands that don't have a paired ingest doc (e.g. `ask.md`, `search.md`, `research.md`), use the same structure: synopsis → flags → subcommands → examples → notes.

### `docs/plans/`

Implementation plans generated during development. Format: free-form markdown, named by feature (e.g. `crawl-performance.md`). Move to `docs/plans/complete/` when the plan is fully executed. Never delete — plans are the written record of why things are the way they are.

### `docs/sessions/`

Session logs: `YYYY-MM-DD-HH-MM-description.md`. Generated by `save-to-md` skill at session end. These capture decisions, root causes, and context for future sessions and agents.

### `docs/reports/`

Code reviews, audits, security analysis. Named by date and scope: `2026-02-22-full-review.md`.

### `docs/SCHEMA.md`

Database schema reference. Updated when tables are added or columns change. See `crates/jobs/*_jobs.rs` for the source of truth (`ensure_schema()` in each file).

---

## Writing New Docs

- **New command?** → Add `docs/commands/<name>.md` following the template above.
- **New ingest source?** → Add both `docs/commands/<name>.md` and `docs/ingest/<name>.md` with cross-links.
- **Implementation plan?** → `docs/plans/<feature>.md`.
- **Session summary?** → `docs/sessions/YYYY-MM-DD-HH-MM-<description>.md` via `save-to-md`.

Keep docs accurate to the code. If you change a flag name, default value, or scan path — update the doc in the same commit.
