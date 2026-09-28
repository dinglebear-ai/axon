---
title: "Agent guidance audit: all scoped instructions"
created: 2026-09-27
updated: 2026-09-27
---

# Agent guidance audit

## Scope and evidence

This pass updates the text of all 53 nested AGENTS.md files plus the root.
It is not only a symlink conversion. The starting point was
[46ccae943](https://github.com/dinglebear-ai/axon/commit/46ccae94336e29cb326c52111d27933cc2e6dd02).
Each existing scoped guide was read and compared with crate exports, manifest
dependencies, named implementation modules, relevant sidecars, and linked docs.

The root is 7,428 characters with 23 document links. The 54 instruction scopes
contain 696 link occurrences; every referenced local target was checked.
The largest tracked root-to-scope instruction chain is 12,388 bytes.
The existing 108 direct CLAUDE.md/GEMINI.md aliases retain their scoped targets.

These are documentation and boundary checks, not a claim that every function
or every runtime feature received a fresh end-to-end audit. No full product
build, reindex, database cleanup, service restart, or deployment was performed.

## Corrected guidance

- axon-api already depends on axon-error; it is not a future migration.
- CLI parsing and bare-source configuration also live in axon-core/config.
- Embedding caching is concrete behavior with injected persistence; durable
  cache storage lives in axon-jobs rather than a stateless-only model.
- LLM and retrieval code include concrete runtime entry points alongside
  boundary traits. Historical scaffold comments do not establish live behavior.
- Ledger and prune instructions no longer assume empty databases. Existing-state
  upgrades, recovery, generation fencing, and partial cleanup must be tested.
- Extractor implementation and dispatch belong to two crates, not two repos.
- MCP documentation distinguishes primary action schema, system/watch requests,
  auxiliary tools, resources, and task protocol behavior.
- Diagnostics explicitly require actionable context, retry prerequisites,
  redaction, partial-effect status, and real fallback impact.
- Palette/plugin/docs scopes link their actual components, manifests, test
  harnesses, and current operational references instead of stale file indexes.

## Scope coverage

| Domain | Live code guidance | Contract documentation guidance |
|---|---|---|
| adapters | [Implementation](../../crates/axon-adapters/src/AGENTS.md) | [Design maintenance](../pipeline-unification/crates/axon-adapters/AGENTS.md) |
| api | [Implementation](../../crates/axon-api/src/AGENTS.md) | [Design maintenance](../pipeline-unification/crates/axon-api/AGENTS.md) |
| authz | [Implementation](../../crates/axon-authz/src/AGENTS.md) | [Design maintenance](../pipeline-unification/crates/axon-authz/AGENTS.md) |
| cli | [Implementation](../../crates/axon-cli/src/AGENTS.md) | [Design maintenance](../pipeline-unification/crates/axon-cli/AGENTS.md) |
| codex | [Implementation](../../crates/axon-codex/src/AGENTS.md) | No historical contract scope |
| core | [Implementation](../../crates/axon-core/src/AGENTS.md) | [Design maintenance](../pipeline-unification/crates/axon-core/AGENTS.md) |
| document | [Implementation](../../crates/axon-document/src/AGENTS.md) | [Design maintenance](../pipeline-unification/crates/axon-document/AGENTS.md) |
| embedding | [Implementation](../../crates/axon-embedding/src/AGENTS.md) | [Design maintenance](../pipeline-unification/crates/axon-embedding/AGENTS.md) |
| error | [Implementation](../../crates/axon-error/src/AGENTS.md) | [Design maintenance](../pipeline-unification/crates/axon-error/AGENTS.md) |
| extract | [Implementation](../../crates/axon-extract/src/AGENTS.md) | [Design maintenance](../pipeline-unification/crates/axon-extract/AGENTS.md) |
| graph | [Implementation](../../crates/axon-graph/src/AGENTS.md) | [Design maintenance](../pipeline-unification/crates/axon-graph/AGENTS.md) |
| jobs | [Implementation](../../crates/axon-jobs/src/AGENTS.md) | [Design maintenance](../pipeline-unification/crates/axon-jobs/AGENTS.md) |
| ledger | [Implementation](../../crates/axon-ledger/src/AGENTS.md) | [Design maintenance](../pipeline-unification/crates/axon-ledger/AGENTS.md) |
| llm | [Implementation](../../crates/axon-llm/src/AGENTS.md) | [Design maintenance](../pipeline-unification/crates/axon-llm/AGENTS.md) |
| mcp | [Implementation](../../crates/axon-mcp/src/AGENTS.md) | [Design maintenance](../pipeline-unification/crates/axon-mcp/AGENTS.md) |
| memory | [Implementation](../../crates/axon-memory/src/AGENTS.md) | [Design maintenance](../pipeline-unification/crates/axon-memory/AGENTS.md) |
| observe | [Implementation](../../crates/axon-observe/src/AGENTS.md) | [Design maintenance](../pipeline-unification/crates/axon-observe/AGENTS.md) |
| parse | [Implementation](../../crates/axon-parse/src/AGENTS.md) | [Design maintenance](../pipeline-unification/crates/axon-parse/AGENTS.md) |
| prune | [Implementation](../../crates/axon-prune/src/AGENTS.md) | [Design maintenance](../pipeline-unification/crates/axon-prune/AGENTS.md) |
| retrieval | [Implementation](../../crates/axon-retrieval/src/AGENTS.md) | [Design maintenance](../pipeline-unification/crates/axon-retrieval/AGENTS.md) |
| route | [Implementation](../../crates/axon-route/src/AGENTS.md) | [Design maintenance](../pipeline-unification/crates/axon-route/AGENTS.md) |
| services | [Implementation](../../crates/axon-services/src/AGENTS.md) | [Design maintenance](../pipeline-unification/crates/axon-services/AGENTS.md) |
| vectors | [Implementation](../../crates/axon-vectors/src/AGENTS.md) | [Design maintenance](../pipeline-unification/crates/axon-vectors/AGENTS.md) |
| web | [Implementation](../../crates/axon-web/src/AGENTS.md) | [Design maintenance](../pipeline-unification/crates/axon-web/AGENTS.md) |

Additional updated scopes:
[docs](../AGENTS.md), [stack](../architecture/stack/AGENTS.md),
[repository references](../development/repo/AGENTS.md),
[MCP references](../reference/mcp/AGENTS.md),
[Palette](../../apps/palette-tauri/AGENTS.md), and
[plugin](../../plugins/axon/AGENTS.md).

## Local environment verification

The private override was replaced with observed configuration facts rather
than inherited deployment assumptions. Evidence included native config/env,
SSH host resolution, process/listener checks, Docker inspection, resolved
Compose configuration, data/config mounts, binary version, and readiness.
Running process settings were compared with env files on disk and resolved
Compose settings because those sources can disagree. Private hostnames,
addresses, paths, and credentials do not belong in this tracked report.

The local override records installation differences from the supported default,
stale values superseded by overlays, configuration-specific tuning, and any
unresolved readiness result. Configuration/provider state remained unchanged.
The override and its direct local alias remain ignored; secrets were not copied.

## Verification

- All 53 nested guides differ substantively from the audit's starting snapshot.
- Local link targets, canonical regular files, direct aliases, character budgets,
  and absence of private host details in shared guides were checked.
- Operational documentation checks passed, including the root 7,500-character
  limit and at least three references per guide.
- Five valid/invalid guide-reference and inherited-budget fixture cases passed.
- 81 workflow-shape tests, 32 changed-path classifier tests, and 12 fleet policy
  adapter tests passed.

The Qdrant recipe wording contract now checks the dedicated
[recipe reference](../development/repo/recipes.md), not a mandatory copy in
root agent instructions. See [documentation maintenance](../development/documentation.md)
for the repeatable validation entry points.
