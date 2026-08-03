# Shared Utilities Contract
Last Modified: 2026-06-30

## Contract

`axon-core` owns small, stable, dependency-light shared utilities used across
pipeline crates. Shared utilities are not a place for domain logic, provider
clients, orchestration, or transport rendering.

There is no standalone `helpers` layer. A helper either belongs to a domain
crate, becomes a named boundary, or qualifies as a small `axon-core` utility.

## Allowed Utility Families

| Family | Examples |
|---|---|
| IDs | stable id builders, UUID helpers, source id hashing |
| Time | clock trait, timestamps, duration parsing |
| Paths | Axon data dirs, artifact-safe paths, local path normalization |
| Config | config file discovery, layered config loading helpers |
| Redaction | shared redactor implementation and field classification |
| HTTP safety | SSRF-safe URL normalization helpers, header redaction |
| Serialization | JSON schema helpers, serde utilities |
| Text | bounded truncation, byte-safe slicing, display-safe snippets |
| Testing | fake clock, temp dirs, deterministic ids |

## Promotion Rules

A utility must become a dedicated boundary/crate when:

- it has multiple implementations
- tests need a fake implementation
- it crosses network/process/security boundaries
- provider capability negotiation changes behavior
- failure/retry semantics materially affect pipeline behavior
- it owns durable state
- it becomes source-specific logic

Examples:

- embedding client belongs to `axon-embedding`, not `axon-core`
- Qdrant client belongs to `axon-vectors`, not `axon-core`
- ledger schema belongs to `axon-ledger`, not `axon-core`
- parser logic belongs to `axon-parse`, not `axon-core`
- prune logic belongs to `axon-prune`, not `axon-core`

## Ban List

Do not add:

- `helpers.rs`
- `utils.rs` with unrelated functions
- transport rendering helpers in `axon-core`
- source adapter shortcuts in `axon-core`
- Qdrant/TEI/Gemini/Codex clients in `axon-core`
- hidden global config reads from utility functions
- filesystem writes from generic helpers except path-safe primitives

## Public API Shape

Shared utilities expose narrow modules:

```text
axon_core::ids
axon_core::time
axon_core::paths
axon_core::config
axon_core::redact
axon_core::http_safety
axon_core::serde
axon_core::text
axon_core::testing
```

Every module has:

- focused public functions/types
- no transport dependencies
- no provider client dependencies
- unit tests
- examples when behavior is subtle

## Testing Requirements

- deterministic id tests
- path traversal tests
- redaction tests
- config precedence tests
- URL/SSRF normalization tests
- byte-safe text truncation tests
- fake clock/temp-dir tests
