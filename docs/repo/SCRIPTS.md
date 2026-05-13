# Scripts Reference -- Axon

Scripts in the `scripts/` directory for maintenance, testing, and development.

## Wrapper script

| Script | Purpose |
|--------|---------|
| `scripts/axon` | Shell wrapper that auto-sources `~/.axon/.env` before running the binary, with repo `.env` fallback for local dev. Use this instead of the bare binary for local dev. |

Usage:
```bash
./scripts/axon doctor
./scripts/axon scrape https://example.com --wait true
```

## Development setup

| Script | Purpose |
|--------|---------|
| `dev-setup.sh` | Bootstrap development environment: installs Rust, just, cargo tools, and web dependencies with npm |
| `test-install-behavior.sh` | Behavioral tests for `install.sh` using fake download/setup commands |

## Quality checks

| Script | Purpose |
|--------|---------|
| `enforce_monoliths.py` | Enforce file size (500 lines) and function size (120 lines) limits on `.rs` files |
| `enforce_no_legacy_symbols.py` | Block deprecated function/type names |
| `check_shell_completions.sh` | Verify shell completion generation |

The five enforcement checks below were ported from shell scripts to the `xtask` crate (see `axon_rust-pp5`). Run via `cargo xtask <name>` or in lefthook:

| xtask command | Purpose |
|---------------|---------|
| `check-env-staged` | Block commits that include `.env` files |
| `check-no-mod-rs` | Enforce no `mod.rs` files (Rust 2018+ convention) |
| `check-mcp-http` | Verify MCP transport configuration |
| `check-unwraps` | Warn about new `.unwrap()`/`.expect(` calls in staged code (warn-only) |
| `check-claude-symlinks` | Verify AGENTS.md / GEMINI.md symlinks next to every CLAUDE.md |

## Docker and deployment

| Script | Purpose |
|--------|---------|
| `audit_compose_images.py` | Audit image references from `docker-compose.yaml`, including GHCR images |
| `plugin-setup.sh` | Configure the local plugin/systemd integration around `~/.axon` |

## Testing

| Script | Purpose |
|--------|---------|
| `test-mcp-tools-mcporter.sh` | MCP smoke test suite (50+ tool calls) |
| `live-test-all-commands.sh` | Integration test all CLI commands against live services |
| `test-client-server-mode.sh` | CLI client/server smoke against a running `axon serve` |
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
| `hook_deny_audit_sync.py` | Hook: verify cargo-deny audit |
| `hook_justfile_lefthook_sync.py` | Hook: verify Justfile/lefthook sync |

## Conventions

All scripts follow these rules:

- Bash scripts use `set -euo pipefail` (strict mode)
- Python scripts use type hints
- All variables are quoted (`"$var"`)
- Scripts are executable (`chmod +x`)
- Exit code 0 = success, 1 = failure, 2 = usage error
