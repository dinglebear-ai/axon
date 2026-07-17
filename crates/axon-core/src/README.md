# src/core
Last Modified: 2026-03-03

Shared runtime primitives used across CLI, jobs, source adapters, vectors, and
web transport modules.

## Purpose
- Provide centralized config parsing/resolution.
- Standardize HTTP/content processing and safety checks.
- Keep logging/health/UI utilities reusable across subsystems.

## Responsibilities
- CLI/env configuration schema and merge logic.
- Runtime HTTP client and safety controls.
- HTML/content normalization and markdown extraction helpers.
- Health probes and user-facing output utilities.

## Key Files
Each `<name>.rs` below is a module **root** with a sibling `<name>/` subdirectory holding submodules (modern Rust 2018 layout — no `mod.rs`).

- `config.rs` + `config/`: clap schema (`cli.rs`/`cli/global_args.rs`), env+flag merge (`parse/build_config.rs` + helpers), canonical `Config` (`types/config.rs`, `types/config_impls.rs`, `types/enums.rs`, `types/subconfigs.rs`, `types/overrides.rs`), validation, secret handling, performance-profile defaults.
- `http.rs` + `http/`: HTTP fetch + request safety (`ssrf.rs`, `client.rs`, `normalize.rs`, `cdp.rs`, `error.rs`, `headers.rs`).
- `content.rs` + `content/`: deterministic content handling (`engine.rs`, `engine/chrome.rs`, `deterministic.rs`).
- `health.rs` + `health/doctor*`: service health probes (TEI, OpenAI, browser).
- `logging.rs`: structured logging helpers used across runtime.
- `paths.rs`: filesystem-path helpers (data/output/cache).
- `ui.rs`: spinner / colored output / `confirm_destructive`.

## Integration Points
- `lib.rs` command dispatch consumes config produced here.
- `src/cli` command handlers depend on `Config` and utility helpers.
- Source adapters and vector providers use the shared HTTP/content layers.
- `src/jobs` workers use config, health, and logging utilities.

## Notes
- Config changes should be coordinated with command handlers and test config builders that construct `Config` literals.
- Keep environment and flag precedence rules centralized in `config/parse.rs`.

## Related Docs
- [Repository README](../../../README.md)
- [Architecture](../../../docs/architecture/overview.md)
- [Docs Index](../../../docs/README.md)
