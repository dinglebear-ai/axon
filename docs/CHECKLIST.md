# Release Checklist -- Axon

Pre-release quality checklist. Complete all items before tagging a release.

## Version and metadata

- [ ] All version-bearing files in sync: `Cargo.toml`, `apps/web/package.json`, `CHANGELOG.md`
- [ ] `CHANGELOG.md` has an entry for the new version
- [ ] README version badge is correct (if present)

## Configuration

- [ ] `.env.example` documents every environment variable the binary reads
- [ ] `.env.example` has no actual secrets -- only placeholders
- [ ] `.env` is in `.gitignore` and `.dockerignore`
- [ ] `services.env` is in `.gitignore` and `.dockerignore`

## Build and test

- [ ] `just verify` passes (fmt-check + clippy + check + test)
- [ ] `just precommit` passes (monolith check + verify)
- [ ] `just docker-build` succeeds
- [ ] Web UI builds: `cd apps/web && pnpm build`
- [ ] Doctor reports all services healthy: `axon doctor`
- [ ] MCP smoke test passes: `just mcp-smoke`

## Security

- [ ] No credentials in code, docs, or git history
- [ ] `.gitignore` includes `.env`, `services.env`, `*.secret`
- [ ] `.dockerignore` includes `.env`, `services.env`, `.git/`
- [ ] Docker containers run as non-root (s6-setuidgid, UID 1001)
- [ ] No baked environment variables in Docker images
- [ ] `AXON_WEB_API_TOKEN` is not exposed as `NEXT_PUBLIC_*`

## Infrastructure

- [ ] `config/docker-compose.services.yaml` starts cleanly (Qdrant, TEI, Chrome)
- [ ] All worker types start (crawl, embed, extract, ingest)
- [ ] SQLite schema auto-creates via `ensure_schema()`

## Documentation

- [ ] Root `CLAUDE.md` matches current architecture
- [ ] CLI command table in `CLAUDE.md` is up to date
- [ ] Environment variable section covers new variables
- [ ] `docs/MCP-TOOL-SCHEMA.md` regenerated: `just gen-mcp-schema`

## Monolith policy

- [ ] No `.rs` files exceed 500 lines (except allowlisted)
- [ ] No functions exceed 120 lines
- [ ] `scripts/enforce_monoliths.py` passes on staged files

## Database

- [ ] Migrations in `migrations/` are sequential
- [ ] Schema changes reflected in the corresponding `ensure_schema()` in `crates/jobs/*_jobs.rs`
- [ ] `ensure_schema()` handles upgrade path from previous version

## Web UI

- [ ] `apps/web` builds without errors
- [ ] Biome lint passes: `cd apps/web && pnpm lint`
- [ ] No `NEXT_PUBLIC_*` variables leak server-side secrets
- [ ] WebSocket auth token handling is correct (two-tier model)
