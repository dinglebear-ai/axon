# axon-core — Agent Guide

`axon-core` owns **shared runtime primitives** that cross crates without being a
domain boundary: config loading + effective snapshots, data/path helpers,
id/clock/time providers, redaction primitives, URL/HTTP-safety (SSRF preflight)
helpers, local filesystem guards, artifact primitives, diagnostics, and test
utilities. Full contract (owns / API / deps / tests):
[../../../crates/axon-core/src/CLAUDE.md](../../../crates/axon-core/src/CLAUDE.md)
· boundary spec:
[../../../docs/architecture/boundary-map.md](../../../docs/architecture/boundary-map.md)
· shared-utilities spec:
[../../../crates/axon-core/src/CLAUDE.md](../../../crates/axon-core/src/CLAUDE.md).

## Current status
`axon-core` currently holds more than the target assigns it, and works today.
It is kept intentionally focused on shared primitives and infrastructure; the
set above. The real **LLM completion backends have already left** for `axon-llm`
(Gemini headless, Codex app-server, OpenAI-compat, dispatch, concurrency). What
stays in `llm.rs` is only the LLM **DTO/config layer** (`CompletionRequest`,
`LlmBackendConfig`, `LlmBackendKind`, `SynthesisModelProfile`, `configured_model_*`,
the `CompletionRunner`/`TextCompleter` traits) — these are embedded in `Config`
and consumed by config parsing/tuning, so moving them would create a cycle
(`axon-llm` depends on `axon-core`). `content/` is still a **leaving tenant** →
`axon-parse` + `axon-document`. Do not add provider clients or misc "utils" here —
every promoted helper must be used by at least two crates and must not create
layering pressure.

The `TextCompleter` trait is the injection seam that lets `axon-core`-internal
callers (the extract LLM fallback in `content/engine.rs`) execute completions
through the real backends without depending on `axon-llm`: the backend is passed
in as `Arc<dyn TextCompleter>` by a higher layer. The doctor's LLM legs use the
same pattern via `health::LlmDoctorProbe` (computed by `axon_llm::build_llm_doctor_probe`
and injected into `build_doctor_report`).

## Module map
Current groups from `crates/axon-core/src/` (target modules in parens):
| Area | Owns |
|---|---|
| `config.rs` + `config/` | config loading, effective config, source tracking (`Config` in `config/types/`) |
| `paths.rs` | data-dir / cache / temp / artifact path helpers (`axon_data_dir`, `ensure_private_dir`, `open_private_append`) |
| `env.rs` · `sqlite.rs` · `logging/` | env + local sqlite + structured logging primitives (ids/time → `ids.rs`/`time.rs`) |
| `redact.rs` | redaction primitives + safe display (`redact_secrets`, `is_secret_like`) |
| `http.rs` + `http/` | URL/HTTP safety, SSRF preflight, fs guards (`http_safety.rs`/`fs.rs`) |
| `artifacts.rs` | artifact write primitives (`ArtifactWriteError`; `ArtifactKind` is an `axon-api` DTO) |
| `health/` · `binary_status.rs` · `endpoints.rs` · `structured/` · `ui/` | diagnostics/feature-flag/test primitives (`diagnostics.rs`/`testing.rs`) |
| `llm.rs` | LLM **DTO/config layer only** (backends now live in `axon-llm`); embedded in `Config`, so it stays here to avoid a cycle. `TextCompleter`/`CompletionRunner` injection seams. |
| `content/` | **LEAVING** → `axon-parse` + `axon-document` |

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
