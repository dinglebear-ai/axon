# axon-core — Agent Guide

`axon-core` owns **shared runtime primitives** that cross crates without being a
domain boundary: config loading + effective snapshots, data/path helpers,
id/clock/time providers, redaction primitives, URL/HTTP-safety (SSRF preflight)
helpers, local filesystem guards, artifact primitives, diagnostics, and test
utilities. Full contract (owns / API / deps / tests):
[../../../docs/pipeline-unification/crates/axon-core/README.md](../../../docs/pipeline-unification/crates/axon-core/README.md)
· boundary spec:
[../../../docs/pipeline-unification/foundation/boundary-map.md](../../../docs/pipeline-unification/foundation/boundary-map.md)
· shared-utilities spec:
[../../../docs/pipeline-unification/foundation/shared-utilities-contract.md](../../../docs/pipeline-unification/foundation/shared-utilities-contract.md).

## Status — live crate, ongoing slim (Phase 3+)
`axon-core` currently holds more than the target assigns it, and works today.
It is being **slimmed continuously from Phase 3 onward** to exactly the primitive
set above. `llm/` and `content/` are **leaving tenants**: LLM completion moves out
to `axon-llm`, and content parsing/chunking moves to `axon-parse` + `axon-document`.
Do not add provider clients or misc "utils" here — every promoted helper must be
used by at least two crates and must not create layering pressure.

## Module map
Current groups from `crates/axon-core/src/` (target modules in parens):
| Area | Owns |
|---|---|
| `config.rs` + `config/` | config loading, effective config, source tracking (`EffectiveConfig`) |
| `paths.rs` | data-dir / cache / temp / artifact path helpers (`DataDirs`/`SafePath`) |
| `env.rs` · `sqlite.rs` · `logging/` | env + local sqlite + structured logging primitives (ids/time → `ids.rs`/`time.rs`) |
| `redact.rs` | redaction primitives + safe display (`Redactor`/`SecretString`) |
| `http.rs` + `http/` | URL/HTTP safety, SSRF preflight, fs guards (`http_safety.rs`/`fs.rs`) |
| `artifacts.rs` | artifact handle primitives (`ArtifactPath`/`ArtifactKind`) |
| `health/` · `binary_status.rs` · `endpoints.rs` · `structured/` · `ui/` | diagnostics/feature-flag/test primitives (`diagnostics.rs`/`testing.rs`) |
| `llm/` · `content/` | **LEAVING** → `axon-llm` / `axon-parse` + `axon-document` |

## Boundary — keep OUT of this crate
- Pipeline orchestration, source acquisition, parsing, chunking, embedding, vector storage, job scheduling, transport routing, provider clients.
- Domain DTOs (belong in `axon-api`); policy/scope decisions (belong in `axon-authz`).
- Miscellaneous single-caller helpers — no kitchen-sink drift.

## Dependencies
- **Allowed:** `axon-error` (and `axon-api` for shared primitive DTOs only); serde/config/path/url/http utility crates.
- **Forbidden:** `axon-services`, `axon-jobs`, `axon-cli`, `axon-mcp`, `axon-web`, and any domain crate; Qdrant, TEI, LLM, Spider, rmcp, Axum, clap. Enforced by `cargo xtask check-layering`.

## Invariants (review checklist)
- Config precedence is CLI > env > TOML > defaults.
- No secrets appear in debug/display output (redaction holds on every display path).
- Path and URL safety checks are deny-by-default on ambiguous input.
- Test clocks and id providers are deterministic; the crate stays below domain, orchestration, and transport layers.

## DTO ownership
This crate exposes primitive helpers, not transport shapes: domain wire DTOs live
in **`axon-api`**. Higher crates that expose data over a transport define/return
`axon-api` DTOs — `axon-core` never redefines transport-facing shapes.

## Keep in sync when shapes change
`README.md` (crate contract) · `foundation/shared-utilities-contract.md` ·
`foundation/boundary-map.md` · `schemas/config-schema.md` · runtime security /
redaction docs.
