# Scripts Reference -- Axon

Scripts in the `scripts/` directory for maintenance, testing, and development.

## Wrapper script

| Script | Purpose |
|--------|---------|
| `scripts/axon` | Shell wrapper that auto-sources `.env` before running the binary. Use this instead of the bare binary for local dev. |

Usage:
```bash
./scripts/axon doctor
./scripts/axon scrape https://example.com --wait true
```

## Development setup

| Script | Purpose |
|--------|---------|
| `dev-setup.sh` | Bootstrap development environment: installs Rust, just, pnpm, cargo tools |

## Quality checks

| Script | Purpose |
|--------|---------|
| `enforce_monoliths.py` | Enforce file size (500 lines) and function size (120 lines) limits on `.rs` files |
| `enforce_no_legacy_symbols.py` | Block deprecated function/type names |
| `check_dockerignore_guards.sh` | Verify `.dockerignore` contains required patterns |
| `check_env_staged.sh` | Block commits that include `.env` files |
| `check_no_mod_rs.sh` | Enforce no `mod.rs` files (Rust 2018+ convention) |
| `check_no_next_middleware.sh` | Block Next.js middleware files |
| `check_pg_advisory_lock.sh` | Verify advisory lock usage |
| `check_shell_completions.sh` | Verify shell completion generation |
| `check_mcp_http_only.sh` | Verify MCP transport configuration |

## Docker and deployment

| Script | Purpose |
|--------|---------|
| `rebuild-fresh.sh` | Build Docker images and start containers |
| `check-container-revisions.sh` | Verify container git SHA matches local HEAD |
| `check_docker_context_size.sh` | Audit Docker build context for large files |
| `audit_compose_images.py` | Audit image references from `config/docker-compose.services.yaml`, including GHCR images |
| `cache-guard.sh` | Build cache management (status/prune) |

## Testing

| Script | Purpose |
|--------|---------|
| `test-mcp-tools-mcporter.sh` | MCP smoke test suite (50+ tool calls) |
| `live-test-all-commands.sh` | Integration test all CLI commands against live services |
| `test-ask-quality-regressions.sh` | RAG answer quality regression tests |
| `test-mcp-oauth-protection.sh` | MCP OAuth endpoint security tests |
| `test_qdrant_quality.py` | Qdrant data quality analysis |

## Code generation

| Script | Purpose |
|--------|---------|
| `generate_mcp_schema_doc.py` | Regenerate `docs/MCP-TOOL-SCHEMA.md` from source |
| `mcp_doc_renderer.py` | MCP schema documentation renderer |
| `mcp_schema_parser.py` | MCP schema parser |
| `mcp_schema_models.py` | MCP schema model definitions |

## Data management

| Script | Purpose |
|--------|---------|
| `reingest.py` | Re-ingest sources from export manifest |
| `migrate_legacy_source_urls.py` | Migrate legacy source URL formats |
| `extract-base-urls.sh` | Extract base URLs from indexed sources |
| `list-all-domains.sh` | List all indexed domains |

## Analysis

| Script | Purpose |
|--------|---------|
| `qdrant-quality.py` | Entry point for Qdrant quality analysis |
| `qdrant_quality_analysis.py` | Quality analysis implementation |
| `qdrant_quality_client.py` | Qdrant client for analysis |
| `qdrant_quality_impl.py` | Quality metrics implementation |
| `qdrant_quality_models.py` | Quality model definitions |
| `qdrant_quality_reporting.py` | Quality reporting |
| `qdrant_quality_runtime.py` | Quality runtime |
| `qdrant_quality_settings.py` | Quality settings |
| `qdrant_quality_ui.py` | Quality UI |

## Miscellaneous

| Script | Purpose |
|--------|---------|
| `cleanup-claude.sh` | Clean up Claude Code artifacts |
| `install-git-hooks.sh` | Install lefthook git hooks |
| `install-agent-skill.sh` | Install agent skill symlinks |
| `validate_skills_ref.sh` | Validate skill references |
| `warn_new_unwraps.sh` | Warn about new `.unwrap()` calls in staged code |
| `hook_deny_audit_sync.py` | Hook: verify cargo-deny audit |
| `hook_justfile_lefthook_sync.py` | Hook: verify Justfile/lefthook sync |

## Conventions

All scripts follow these rules:

- Bash scripts use `set -euo pipefail` (strict mode)
- Python scripts use type hints
- All variables are quoted (`"$var"`)
- Scripts are executable (`chmod +x`)
- Exit code 0 = success, 1 = failure, 2 = usage error
