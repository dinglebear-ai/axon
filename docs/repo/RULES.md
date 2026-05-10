# Coding Rules -- Axon

Standards and conventions enforced across the Axon codebase.

## Git workflow

### Conventional commits

| Prefix | Purpose | Example |
|--------|---------|---------|
| `feat:` | New feature | `feat(mcp): add screenshot action` |
| `fix:` | Bug fix | `fix(crawl): handle timeout on sitemap backfill` |
| `chore:` | Maintenance | `chore: update spider to 2.47` |
| `refactor:` | Code restructure | `refactor(services): extract ask pipeline` |
| `test:` | Tests | `test(vector): add hybrid search tests` |
| `docs:` | Documentation | `docs: update CONFIG reference` |
| `ci:` | CI/CD changes | `ci: add nextest to verify` |

### Branch strategy

- `main` is production-ready
- Feature branches for development
- PR required before merge

### Never commit

- `.env` files
- API keys, tokens, or passwords
- Large binary files
- `target/`, `node_modules/`, `.next/`

## Version bumping

### Bump type rules

| Commit prefix | Bump | Example |
|---------------|------|---------|
| `feat!:` or `BREAKING CHANGE` | Major | `0.35.0` -> `1.0.0` |
| `feat:` or `feat(...):` | Minor | `0.35.0` -> `0.36.0` |
| Everything else | Patch | `0.35.0` -> `0.35.1` |

### Version-bearing files

All must have the same version:

| File | Field |
|------|-------|
| `Cargo.toml` | `version = "X.Y.Z"` in `[package]` |
| `apps/web/package.json` | `"version": "X.Y.Z"` |
| `CHANGELOG.md` | New entry under `## X.Y.Z` |

## Monolith policy (enforced)

Changed `.rs` files are checked at CI and via lefthook pre-commit:

| Metric | Warn | Fail |
|--------|------|------|
| File size | -- | 500 lines |
| Function size | 80 lines | 120 lines |

Exempt: `tests/**`, `benches/**`, `config/**`, `**/config.rs`.

Exceptions: add to `.monolith-allowlist`.

Enforcement: `scripts/enforce_monoliths.py` runs on staged files.

## Module layout (enforced)

Rust 2018+ file-per-module layout. `mod.rs` is forbidden:

```
foo.rs          <- module root (declarations: mod bar; mod baz;)
foo/
  bar.rs        <- submodule
  baz.rs        <- submodule
```

Enforcement: `cargo xtask check-no-mod-rs`.

## Rust code standards

- `cargo fmt` before committing
- `cargo clippy` clean (all warnings are errors in CI)
- `unsafe` code is denied (`#[deny(unsafe_code)]` in `Cargo.toml`)
- Errors: `Box<dyn Error>` at command boundaries, typed errors internally
- Logging: `log_info` / `log_warn` (not `println!` in library code)
- `--json` flag enables machine-readable output on all result-printing commands
- Structured log output via `tracing` with `env-filter`

### Services layer contract

- CLI commands, MCP handlers, and HTTP routes all call through `src/services/`
- Each service function returns a typed result struct (no raw JSON, no stdout side-effects)
- Service result types live in `src/services/types/service.rs`

## TypeScript code standards (web panel)

- ESM modules, `import` syntax
- No `any` types
- Strict mode in `tsconfig.json`
- Static Next.js export only; runtime APIs are served by Rust under `src/web`

## Pre-commit hooks (lefthook)

| Hook | Purpose |
|------|---------|
| `enforce_monoliths.py` | File and function size limits |
| `enforce_no_legacy_symbols.py` | Block deprecated names |
| `cargo xtask check-env-staged` | Block .env commits |
| `cargo xtask check-no-mod-rs` | No mod.rs files |
| `cargo xtask check-unwraps` | Flag new .unwrap() calls (warn-only) |
| `cargo xtask check-mcp-http` | MCP transport configuration parity |
| `cargo xtask check-claude-symlinks` | AGENTS.md/GEMINI.md symlinks present |

Install hooks:

```bash
./scripts/install-git-hooks.sh
```

## Performance profiles

Concurrency is tuned relative to available CPU cores:

| Profile | Crawl | Sitemap | Backfill | Timeout | Retries |
|---------|-------|---------|----------|---------|---------|
| `high-stable` (default) | CPUs x 8 | CPUs x 12 | CPUs x 6 | 20s | 2 |
| `balanced` | CPUs x 4 | CPUs x 6 | CPUs x 3 | 30s | 2 |
| `extreme` | CPUs x 16 | CPUs x 20 | CPUs x 10 | 15s | 1 |
| `max` | CPUs x 24 | CPUs x 32 | CPUs x 20 | 12s | 1 |
