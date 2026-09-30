---
title: "Feature Delivery Framework"
created: 2026-02-26
updated: 2026-09-29
---

# Feature Delivery Framework

Last reviewed: 2026-09-29

## Purpose and scope

Deliver features through the existing ownership and service contracts, not a
parallel pipeline or a transport-specific implementation. This guide is a
checklist under the canonical [AGENTS.md](../../AGENTS.md),
[crate ownership](../architecture/crate-ownership.md), and
[contributing](contributing.md) rules; it does not override them.

The unified pipeline is implemented. Old instructions to create root
`src/services/`, leave all existing transport bypasses in place, or put every
new domain function in services are obsolete.

## Architecture standard

### Domain-first, shared orchestration

DTOs belong in `axon-api`, typed error contracts in `axon-error`, and
single-domain logic in its owning crate. `axon-services` composes domains and
job-aware workflows. CLI, MCP, and HTTP parse/authorize requests, call shared
services, and render public results. Transports must not import
domain-internal `::ops::*`.

Acquisition belongs in `axon-adapters`, preparation in `axon-document`,
embedding in `axon-embedding`, publication in `axon-vectors`, and queries in
`axon-retrieval`. Ledger owns source identities/generations/manifests/status
and cleanup debt; jobs own attempts, workers, reservations, and execution.
See [source pipeline](../architecture/source-pipeline.md).

### One lifecycle, not one queue per provider

Reuse SourceAdapter and the existing family when an input fits. Adapters
emit normalized `SourceDocument` values, not chunks/vectors/transport
responses. A new provider name is not a reason to create another pipeline.
Keep one source job ID across stages; watches and focused projections reuse
that lifecycle instead of handing off to child embedding jobs.

Retries/cooling remain with embedding providers, upserts with vectors, and
cleanup debt with the ledger. Shared orchestration does not centralize every
provider-specific retry or store operation in one function.

## Surface decision matrix

| Surface | Required integration |
|---|---|
| CLI | Real parser/dispatch, human and structured output, exit/progress behavior |
| MCP | Canonical operation/projection, schema, authorization, resources/tasks where relevant |
| HTTP | Route/OpenAPI, caller identity, origin/auth policy, public request/result mapping |
| Client UI | Shared API/DTO/presentation contracts, user-visible progress and diagnostics |

Choose the surfaces the feature actually needs. Family-level parity means
those families are exposed, not identical subcommands or request shapes.
A UI-only interaction may reuse an existing API; it does not require a
new transport-specific domain model.

## Delivery lifecycle

### Phase 0: classify and inspect

Identify the domain owner, synchronous versus durable execution, read/write
effects, external prerequisites, cancellation boundaries, expected output
size, and required surfaces. Inspect existing code/configuration and the
applicable local instruction chain before editing. Preserve unrelated work.

### Phase 1: design contracts

Define public DTOs, stable codes/causes, events, and compatibility/migration
behavior. For a source adapter, register identity, scopes, options, auth,
parsing/chunking, metadata, and graph facts in the family spec.
`onboarding_status()` proves declarations are complete, not that acquisition
or publication works.

### Phase 2: implement owning domains and orchestration

Implement domain behavior behind the existing public boundary and compose
it through services. Enforce caller authorization, filesystem/SSRF policy,
timeouts, bounded pagination/output, and redaction. Tool metadata discovery
is not execution permission. Materialized resources need cleanup after
success, failure, and cancellation.

For source refresh, distinguish added/modified/removed/unchanged items. Use
stable hashes to skip unchanged work. Infer deletion only from a complete
snapshot or explicit tombstone. Publish before advancing derived graph state
and retain cleanup debt when safe cleanup cannot finish.

### Phase 3: wire selected projections

Map public contracts through the actual CLI/MCP/HTTP entry points. Preserve
caller context, ownership/visibility, progress metadata, and result/error
semantics. Do not introduce trusted-local authorization into a remote path.

MCP includes legacy, atomic, and combined projections, auxiliary dashboard,
schema/resources, separate system/watch requests, and protocol tasks. The
primary action enum is not the entire catalog. Only supported operations
may accept negotiated task augmentation. Use
[adding an MCP action](adding-mcp-action.md) and
[adding a REST route](adding-rest-route.md).

### Phase 4: validate and regenerate

Run the owning tests, including failed/partial/canceled/retried paths, then
regenerate schema/contract inputs before dependent Markdown. Verify a second
read-only check passes and review the generated diff. A schema or job ID
alone is not completion evidence.

### Phase 5: review and roll out

Review the implementation, migration/configuration impacts, tests, generated
references, and operator recovery guidance in the PR. Deploy only when
requested, using verified installation notes. Reconnect and inspect the
actual runtime/catalog after service or MCP configuration changes.

## File-level integration checklist

Use the existing owner/module, not guessed files from an old monolithic tree:

| Concern | Current location or guide |
|---|---|
| DTOs and domain errors | `crates/axon-api/`, `crates/axon-error/` |
| Shared orchestration | `crates/axon-services/` |
| CLI parser/dispatch | `crates/axon-core/src/config/`, `crates/axon-cli/` |
| MCP | `crates/axon-mcp/src/server.rs` and its scoped request/handler modules |
| HTTP/REST | `crates/axon-web/` and generated OpenAPI |
| Source extension | [Adding a source](adding-source.md), [adapter onboarding](adding-source-adapter.md) |
| Parsing/chunking | [Adding a parser](adding-parser.md), [chunking](../reference/sources/chunking.md) |
| Durable schema | New owning-crate migrations, never rewritten applied migrations |
| Documentation | [Generator ownership and checks](documentation.md) |

Preserve `#[path]`, test sidecar filenames, and cfg gates. No `mod.rs`.
Respect source-file/function budgets and existing checker exemptions.

## Streaming, reliability, and degradation

Expose progress and heartbeat/phase evidence while work is active, using
existing typed event contracts rather than a new invented enum per surface.
Structured CLI output belongs on stdout, logs on stderr; correlate through
`axon-observe`. MCP can carry progress and negotiated tasks, so it must not
be classified categorically as non-streaming.

Failures must identify operation/stage, severity, safe cause, affected
entity/provider, relevant IDs, retryability, and concrete recovery. State
completed work, possible side effects, and whether repetition is safe.
Unknown commit status must be explicit, not an invitation to duplicate
a write. Warnings explain impact and the actual fallback. Partial success,
skipped work, degraded results, and failure are distinct outcomes.

See [error contracts](../pipeline-unification/runtime/error-handling.md) and
[observability](../reference/runtime/observability.md).

## Testing standard

```bash
cargo build --bin axon
cargo test -p <affected-crate> <test-filter> --locked
cargo fmt --all -- --check
cargo xtask check-layering
cargo xtask generated-contracts refresh
cargo xtask generated-contracts check
```

Select real crate/test names and verify tests actually ran. `cargo check`
does not compile all test sidecars, and a zero-test filter is not evidence.
Use `just verify` for broad integration and the
[testing guide](testing.md) for provider/isolation prerequisites.

Include refresh/removal visibility, stable-hash skipping, partial snapshots,
cancellation, heartbeat/recovery, retries, invalid input, denied auth/origin,
redaction, and existing-state upgrades for affected boundaries. Validate
the real MCP catalog/calls or REST route against a matching build.

## Definition of done and PR review

The owning domain and shared service are wired through the selected surfaces;
tests cover success and failure paths; generated outputs reproduce; docs
name real commands/settings and explain recovery; migrations preserve prior
state; and actual validation results are recorded.

Document provider-dependent skips and unverified deployment behavior. A
passing build, accepting descriptor, or configuration write is not proof of
a completed source pipeline or successful deployment. No future-tense plan
should be silently relabeled as current behavior.
