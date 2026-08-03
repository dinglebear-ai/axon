---
title: "Feature Delivery Framework"
updated: 2026-08-02
---

# Feature Delivery Framework

Deliver features through the existing domain boundaries instead of creating a
transport-owned implementation path. CLI, MCP, REST, and app clients are
projections over shared DTOs and services.

## 1. Identify the owner

Choose the crate that owns the data or behavior:

| Concern | Owner |
|---|---|
| DTO, enum, envelope, wire schema | `axon-api` |
| Error taxonomy | `axon-error` |
| Auth policy and scope decisions | `axon-authz` |
| Config, paths, HTTP safety, redaction | `axon-core` |
| Source resolution | `axon-route` |
| Acquisition | `axon-adapters` |
| Parsing and document preparation | `axon-parse`, `axon-document` |
| Generation and manifest state | `axon-ledger` |
| Jobs, workers, watches, scheduling | `axon-jobs` |
| Vector storage or retrieval | `axon-vectors`, `axon-retrieval` |
| Graph | `axon-graph` |
| Memory | `axon-memory` |
| LLM synthesis | `axon-llm` |
| Cross-domain composition | `axon-services` |
| CLI presentation | `axon-cli` |
| MCP transport | `axon-mcp` |
| REST/OpenAPI/panel transport | `axon-web` |

Single-domain logic stays in the domain crate. Use `axon-services` when an
operation coordinates multiple owners or participates in the shared runtime.

## 2. Define the contract first

Before transport work:

1. Add or update transport-neutral DTOs in `axon-api`.
2. Add typed errors in `axon-error` when the existing taxonomy is
   insufficient.
3. Define authorization, execution-affinity, and visibility requirements.
4. Decide synchronous versus durable-job behavior.
5. Identify generated schemas or references that must change.

Avoid raw JSON between layers. Domain/service APIs return typed values and do
not print to stdout.

## 3. Implement the domain path

Add the behavior to the owning crate and cover it with focused tests. When
cross-domain composition is required, expose it through
`crates/axon-services/src/` without duplicating domain logic.

For source work, preserve the canonical sequence:

```text
SourceRequest
  -> route and authorize
  -> acquire
  -> diff and generation planning
  -> normalize/parse/prepare
  -> embed and vectorize
  -> publish
  -> graph
  -> cleanup debt
  -> SourceResult
```

Do not create a second source-family job store or publication path.

## 4. Add transport projections

### CLI

- Parser/config ownership: `crates/axon-core/src/config/cli/`
- Command handlers and rendering: `crates/axon-cli/src/commands/`
- Generated command registry: `docs/reference/cli/commands.json`

Keep handlers thin: parse, call the shared service/domain boundary, render.

### MCP

- Shared request DTOs: `crates/axon-api/src/mcp_schema/`
- Action registry/auth classification: `crates/axon-mcp/src/server/authz.rs`
- Handler dispatch: `crates/axon-mcp/src/server/`
- Generated wire contract: `docs/reference/mcp/tool-schema.json`

MCP exposes one tool named `axon`; add an action/subaction rather than a new
tool.

### REST and web

- Router: `crates/axon-web/src/server/routing.rs`
- Handlers: `crates/axon-web/src/server/handlers/`
- OpenAPI registry: `crates/axon-web/src/server/openapi.rs`
- Generated OpenAPI: `docs/reference/rest/openapi.json`

Apply the same scope, error envelope, and durable-job semantics as CLI/MCP.

### Client apps

Regenerate client bindings from OpenAPI/DTO sources. Do not hand-invent a
parallel wire shape in Android, Palette, Chrome extension, or web code.

## 5. Update generated contracts

Depending on the change, run:

```bash
cargo xtask schemas generate --update-fixtures
cargo xtask docs generate
cargo xtask presentation generate
cargo xtask gen-api-parity
python3 scripts/generate_action_docs.py
cargo xtask gen-public-api
cargo xtask gen-dep-graph
```

Generated artifacts have one owner. A generator input must never be one of its
own outputs or a downstream generated-doc output.

## 6. Test at the right layers

Minimum evidence normally includes:

- domain unit tests
- fake-boundary or store/provider tests
- service orchestration tests for cross-domain work
- CLI/MCP/REST shape and parity tests when transports change
- authorization and redaction tests for public surfaces
- durable job recovery/cancellation tests for async work
- generated schema/docs drift checks

Use live provider tests only when they prove behavior a fake cannot cover.

## 7. Review structural constraints

Before commit:

```bash
cargo fmt --all -- --check
cargo xtask check-layering
cargo xtask check-fetch-divergence
cargo xtask check-crate-contracts
cargo xtask check-repo-structure
cargo xtask docs check
python3 scripts/enforce_monoliths.py --staged
```

Then run:

```bash
just precommit
```

## Definition of done

A feature is complete when:

- one canonical implementation owns the behavior;
- every exposed transport uses the same typed contract;
- auth, redaction, errors, observability, and durable lifecycle are explicit;
- generated schemas/docs and client bindings are synchronized;
- no removed compatibility surface is revived;
- focused tests and the full repository gate pass;
- review findings are resolved and CI is green.
