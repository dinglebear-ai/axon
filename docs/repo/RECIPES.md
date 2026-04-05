# Justfile Recipes -- Axon

Run `just --list` to see all available recipes.

## Development

| Recipe | Purpose |
|--------|---------|
| `just setup` | Bootstrap development environment (installs all dependencies) |
| `just check` | Fast type check (`cargo check`) |
| `just check-tests` | Type check including test code |
| `just test` | Run all tests (prefers cargo-nextest, falls back to cargo test) |
| `just test-fast` | Run lib tests only (no integration tests) |
| `just test-infra` | Run infrastructure integration tests (requires services) |
| `just test-all` | Run all targets with all features |
| `just fmt` | Format all Rust code |
| `just fmt-check` | Check formatting without modifying |
| `just clippy` | Run clippy lints |
| `just fix` | Auto-fix: format + clippy --fix |
| `just fix-all` | Fix Rust + web (pnpm format) |
| `just clean` | Remove build artifacts |

## Quality gates

| Recipe | Purpose |
|--------|---------|
| `just verify` | Full CI gate: dockerignore check + fmt + clippy + check + test |
| `just ci` | Alias for verify |
| `just precommit` | Full pre-commit: monolith check + verify |
| `just lint-all` | Rust fmt-check + clippy + web lint |

## Docker

| Recipe | Purpose |
|--------|---------|
| `just docker-build` | Build workers Docker image (`axon:local`) |
| `just services-up` | Start infrastructure (Postgres, Redis, RabbitMQ, Qdrant, TEI, Chrome) |
| `just services-down` | Stop infrastructure |
| `just up` | Build and start app containers (workers + web) |
| `just down` | Stop app containers |
| `just down-all` | Stop everything (app + infrastructure) |
| `just rebuild-fresh` | Full rebuild: check + build + start containers |
| `just rebuild` | check + test + docker-build |

## Local stack

| Recipe | Purpose |
|--------|---------|
| `just dev` | Full local dev: stop existing, start infra, build, run `axon serve` |
| `just serve [port]` | Run `axon serve` (debug build, default port 49000) |
| `just serve-release [port]` | Run `axon serve` (release build) |
| `just workers` | Start all 6 worker types as background processes |
| `just stop` | Kill running axon serve, workers, and Next.js processes |

## Web UI

| Recipe | Purpose |
|--------|---------|
| `just web-dev` | Start Next.js dev server standalone |
| `just web-build` | Build Next.js for production |
| `just web-lint` | Run Biome linter on web code |
| `just web-format` | Format web code with Biome |

## Testing

| Recipe | Purpose |
|--------|---------|
| `just mcp-smoke` | Run MCP tool smoke tests |
| `just test-infra-up` | Start test infrastructure containers |
| `just test-infra-down` | Stop test infrastructure |

## Build tools

| Recipe | Purpose |
|--------|---------|
| `just build` | Release build (`cargo build --release`) |
| `just install` | Build release + symlink to `~/.local/bin/axon` |
| `just nextest-install` | Install cargo-nextest |
| `just llvm-cov-install` | Install cargo-llvm-cov for coverage |
| `just coverage-branch` | Generate lcov coverage report |

## Maintenance

| Recipe | Purpose |
|--------|---------|
| `just gen-mcp-schema` | Regenerate MCP-TOOL-SCHEMA.md from source |
| `just cache-status` | Check build cache status |
| `just cache-prune` | Prune build cache |
| `just docker-context-probe` | Check Docker build context size |
| `just check-container-revisions` | Verify container git SHA matches |
| `just watch-check` | cargo-watch: check + test on every save |

## Chaining

```bash
just fmt clippy check test    # Run quality checks in sequence
just verify                    # Same as above, single command
```

The `verify` recipe is the standard pre-PR gate.
