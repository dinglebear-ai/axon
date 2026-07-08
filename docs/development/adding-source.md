# Adding a New Source (End-to-End Checklist)

This is the end-to-end checklist for bringing a whole new source **family**
online — everything from URI resolution through docs generation. For the
narrower "how do I write the `SourceAdapter` implementation" mechanics, see
[`adding-source-adapter.md`](adding-source-adapter.md); this page is the
onboarding checklist that sits above it.

Canonical contract:
`docs/pipeline-unification/sources/new-source-contract.md` (this guide
summarizes and links out to the per-area contracts it references — treat the
contract doc as authoritative if the two ever diverge).

## Every source enters through one pipeline

```text
SourceRequest -> SourceResolver -> SourceRouter -> SourceAdapter
  -> SourceLedger -> SourceDocument -> SourceParseFacts / GraphCandidate
  -> DocumentPreparer -> EmbeddingProvider -> VectorStore -> DocumentStatus
```

No new source may bypass this by writing `PreparedDocument`, vector points,
graph rows, or transport responses directly. Source-specific behavior is
only allowed behind adapter, parser, chunking, metadata, auth, and graph
contracts — not by inventing a second pipeline.

## Required onboarding checklist

Every row below must be implemented before a new source family is
considered onboarded. This is the same 15-row shape
`crates/axon-adapters/src/onboarding.rs::onboarding_status()` mechanically
checks against your `SourceAdapterSpec` — use that function to sanity-check
you haven't left a row empty, but note it only checks *declared*, not
*correct*, behavior.

| Area | Required work | Owning contract |
|---|---|---|
| Source identity | Define source kind, URI forms, canonical URI, source id strategy, and aliases. | `docs/pipeline-unification/sources/url-normalization.md` |
| Resolver | Add resolver rules and ambiguity handling. | `docs/pipeline-unification/foundation/source-pipeline.md` |
| Router | Register adapter selection and scope defaults. | `docs/pipeline-unification/sources/adapter-scopes.md` |
| Adapter | Implement acquisition that emits `SourceDocument` only. | [`adding-source-adapter.md`](adding-source-adapter.md) |
| Scopes | Declare supported scopes, default scope, map behavior, watch support, and option schema. | `docs/pipeline-unification/sources/adapter-scopes.md` |
| Ledger | Define item keys, manifest hash inputs, diff behavior, generation semantics, and cleanup debt. | `docs/pipeline-unification/runtime/ledger-contract.md`, [`docs/reference/runtime/ledger.md`](../reference/runtime/ledger.md) |
| Parsing | Add a parser family or declare parse-free behavior. | `docs/pipeline-unification/sources/parsing-contract.md`, [`adding-parser.md`](adding-parser.md) |
| Graph | Declare required and optional graph facts. | `docs/pipeline-unification/sources/source-graph.md` |
| Chunking | Select chunk profile and routing hints. | `docs/pipeline-unification/sources/chunking-contract.md` |
| Metadata | Define shared and source-specific metadata fields. | `docs/pipeline-unification/sources/metadata-payload.md` |
| Auth/secrets | Define credential requirements, scopes, and redaction rules. | `docs/pipeline-unification/runtime/auth-contract.md`, `docs/pipeline-unification/runtime/security-contract.md` |
| Observability | Emit standard phases, counts, heartbeats, warnings, and degradation. | `docs/pipeline-unification/runtime/observability-contract.md` |
| Error handling | Define retryable, degraded, and terminal failure modes. | `docs/pipeline-unification/runtime/error-handling.md` |
| Tests | Add resolver, adapter, parser, metadata, graph, and source-job fixtures. | `docs/pipeline-unification/delivery/testing-contract.md` |
| Docs | Add source help, examples, capability docs, and generated schema coverage. | `docs/pipeline-unification/delivery/documentation-contract.md` |

## `SourceAdapterSpec` is the single declaration point

Every source registers one `SourceAdapterSpec` in
`crates/axon-adapters/src/family_matrix.rs` (see
[`adding-source-adapter.md`](adding-source-adapter.md) for the full field
list and required rules). Key naming/versioning rules from the contract:

- `adapter` is a stable snake_case name used in logs, payloads, docs, and
  capability output — do not rename it casually once shipped.
- `version` changes when emitted documents, metadata, graph facts, or parser
  behavior materially changes — downstream consumers (ledger item keys,
  cleanup debt selectors) may key off it.
- `source_kinds` use the closed `SourceKind` enum — do not invent a
  source-specific string kind.
- `supported_schemes` declare explicit URI schemes or prefixes (e.g.
  `github:`, `npm:`, `youtube:`, `mcp:`, `file:`) so the resolver can route
  unambiguously.

## Security-sensitive families need extra gates

Per the contract's global constraints (also enforced structurally by the
`SourceAdapterSpec` booleans):

- **Tool execution sources** (`CliTool`, `McpTool` families) default to
  metadata-only/no-exec and require explicit opt-in, allowlists, env
  allowlists, timeout/output caps, audit metadata, and redaction before
  writes.
- **Network/render sources** (web, feed) must enforce SSRF checks before any
  HTTP or Chrome access.
- **Local sources** require `axon:local` or trusted local context and must
  redact absolute paths from public payloads.

## Existing families as reference implementations

Rather than starting from a blank page, read an existing family end to end:
`crates/axon-adapters/src/local.rs` (filesystem — simplest, no network),
`crates/axon-adapters/src/git.rs` (repository), or
`crates/axon-adapters/src/feed.rs` (RSS/Atom/JSON — smallest network-facing
example). Each has a sidecar `_tests.rs` file and a fixture tree under
`crates/axon-adapters/fixtures/<family>/`.

## Generated docs and schemas

Adapter capability docs and schemas are generated from the family matrix, not
hand-written:

```bash
cargo xtask schemas generate
```

Run this after registering a new `SourceAdapterSpec` so generated capability
docs and payload/provider-capability schemas stay in sync with the matrix —
do not hand-edit generated output.
