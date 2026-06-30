# axon-core Crate Contract
Last Modified: 2026-06-30

## Purpose

`axon-core` owns shared runtime primitives that are useful across crates but do
not represent a domain boundary by themselves.

## Owns

- config loading, effective config snapshots, and config source tracking
- data directory, cache, artifact, and temp path helpers
- id, clock, deterministic test time, and run/job/source id helpers
- redaction primitives and safe display wrappers
- URL/HTTP safety helpers, SSRF preflight primitives, and local filesystem guards
- diagnostics, feature flags, and test utility primitives

## Must Not Own

- pipeline orchestration, source acquisition, parsing, chunking, embedding,
  vector storage, job scheduling, transport routing, or provider clients
- domain DTOs that belong in `axon-api`
- policy decisions that belong in `axon-authz`

## Public Modules

```text
lib.rs
config.rs
paths.rs
ids.rs
time.rs
redact.rs
http_safety.rs
artifact.rs
fs.rs
diagnostics.rs
testing.rs
```

## Public API

- `EffectiveConfig`, `ConfigSource`, `ConfigLoadReport`
- `DataDirs`, `PathPolicyInput`, `SafePath`
- `IdProvider`, `Clock`, `SystemClock`, `FixedClock`
- `Redactor`, `SecretString`, `Redacted<T>`
- `HttpSafetyCheck`, `UrlPolicyInput`, `SafeUrl`
- `ArtifactPath`, `ArtifactKind`
- `DiagnosticsSnapshot`

## Dependencies Allowed

- `axon-error`
- serde/config/path/url/http utility crates
- no heavy domain or transport dependencies

## Dependencies Forbidden

- `axon-services`, `axon-jobs`, `axon-cli`, `axon-mcp`, `axon-web`
- Qdrant, TEI, LLM, Spider, rmcp, Axum, clap
- domain crates that would make `axon-core` a kitchen sink

## Generated Artifacts

- config schema inputs for [../../schemas/config-schema.md](../../schemas/config-schema.md)
- redaction examples for runtime/security docs

## Fixtures And Fakes

- temp data directory fixture
- fixed clock and deterministic id provider
- redactor fixture with known secret values
- URL/path safety fixtures

## Tests

- config precedence is CLI > env > TOML > defaults
- no secrets appear in debug/display output
- path and URL safety checks are deny-by-default on ambiguous input
- test clocks and ids are deterministic

## Acceptance Criteria

- utilities promoted here are used by at least two crates
- adding a domain-specific helper here requires an explicit reason in docs
- this crate stays below domain, orchestration, and transport layers

See [../README.md](../README.md) and
[../../foundation/shared-utilities-contract.md](../../foundation/shared-utilities-contract.md).
