# docs/ — Documentation Instructions

Last reviewed: 2026-09-27

This scoped guide extends the root [AGENTS.md](../AGENTS.md). This file is
canonical; the sibling CLAUDE.md and GEMINI.md files are direct symlinks.
The [documentation index](README.md) is the navigation entry point and
[documentation maintenance](development/documentation.md) defines the audit
and verification workflow.

## Authority

Describe the implementation on the branch being edited, not an unmerged PR.
Generated references describe the public contracts; source manifests own
versions, dependencies, and configuration. The unified SourceRequest pipeline
is implemented. Dated pipeline-unification plans explain its design, not a
future migration that users still need to perform.

Do not hand-edit generated schema or Markdown projections. When their owning
inputs change, run the aggregate generated-contract refresh/check in the
order documented by the root guide. For prose-only changes, use scoped
structural checks instead of rebuilding the product.

## Directory layout

| Directory | What belongs here |
|---|---|
| guides/ | Getting started, configuration, and task-oriented how-to |
| guides/ingest/ | Source acquisition setup, behavior, limitations, and troubleshooting |
| reference/ | Current CLI, REST, MCP, config, runtime, and wire contracts |
| reference/actions/ | Operation usage and generated surface mappings |
| architecture/ | Current crate ownership, dependencies, and runtime design |
| development/ | Contribution, testing, extension points, release, and documentation workflows |
| development/repo/ | Repository/tooling navigation and conventions |
| operations/ | Deployment, security, recovery, and performance runbooks |
| testing/ | Test qualification and execution documentation |
| adr/ | Architectural decision records |
| pipeline-unification/ | Implemented clean-break design contract and dated delivery records |
| sessions/, reports/, investigations/, plans/, superpowers/, perf/ | Point-in-time records; retain historical context |
| archive/ | Removed-runtime history; do not rewrite as current instructions |
| eval/ | Evaluation data and fixtures |

The old docs/contributing/ directory is not the current layout; use development/.

## Action references versus source guides

An action reference answers how to invoke an operation. Keep its generated
surface block, arguments, flags, examples, and lifecycle contract aligned with
current CLI/MCP/REST registries. Use the unified jobs lifecycle rather than
inventing source-family queues or worker commands.

A source guide answers what is acquired, how to configure it, how its adapter
uses the shared pipeline, and how to debug it. Link to the existing source
operation reference; do not create a new CLI command or action page merely
because a new SourceAdapter was added. Follow
[adding a source](development/adding-source.md) and
[adding a source adapter](development/adding-source-adapter.md).

Only create cross-links to real files. Do not add placeholder action stubs for
operations that do not have an independent deep-dive. Keep implementation
ownership in the appropriate domain crate; transport projections are not
separate pipelines.

## Maintaining documentation

Update the affected guide and its index in the same change as a renamed flag,
new extension point, changed deployment path, or changed default. Cite the
repo-relative source/registry owning any implementation-specific assertion.
Avoid copying complete flag, environment-variable, migration, or dependency
inventories into prose when generated references already own them.

Plans, reports, and session logs are dated evidence. Preserve them rather than
rewriting old outcomes or presenting planned capabilities as shipped. A live
summary may link to historical evidence while stating current behavior.

SQLite migrations belong to their owning crates. The generated database
reference is reference/runtime/database-schema.json; consult its provenance
instead of maintaining another table count here.
