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
- [ ] `~/.axon/.env` is used for local secrets and Compose interpolation

## Build and test

- [ ] `just verify` passes (fmt-check + clippy + check + test)
- [ ] `just precommit` passes (monolith check + verify)
- [ ] Web panel builds: `cd apps/web && npm run build`
- [ ] Doctor reports all services healthy: `axon doctor`
- [ ] MCP smoke test passes: `just mcp-smoke`
- [ ] Client/server smoke test passes: `just client-server-smoke`

## Security

- [ ] No credentials in code, docs, or git history
- [ ] `.gitignore` includes `.env`, `*.secret`
- [ ] `.dockerignore` includes `.env`, `.git/`
- [ ] Docker containers run as non-root (`user: "1000:1000"`)
- [ ] No baked environment variables in Docker images
- [ ] MCP/action auth uses `AXON_MCP_HTTP_TOKEN` or OAuth for non-loopback binds

## Infrastructure

- [ ] `docker-compose.yaml` starts cleanly with `--env-file ~/.axon/.env` (Axon server, Qdrant, TEI, Chrome)
- [ ] `axon serve` starts and owns in-process crawl/embed/extract/ingest workers
- [ ] SQLite schema migrations apply under `src/jobs/lite/migrations/`

## Documentation

- [ ] Root `CLAUDE.md` matches current architecture
- [ ] CLI command table in `CLAUDE.md` is up to date
- [ ] Environment variable section covers new variables
- [ ] `docs/MCP-TOOL-SCHEMA.md` regenerated: `just gen-mcp-schema`

## Monolith policy

- [ ] No `.rs` files exceed 500 lines (except allowlisted)
- [ ] No functions exceed 120 lines
- [ ] `scripts/enforce_monoliths.py` passes on staged files

## SQLite

- [ ] Migrations in `src/jobs/lite/migrations/` are sequential
- [ ] Schema changes are reflected in SQLite store/list/recover paths
- [ ] Migration upgrade path works against an existing `~/.axon/jobs.db`

## Web Panel

- [ ] `apps/web` builds without errors
- [ ] Panel routes in `src/web/*` still require panel password/session or MCP/action auth as appropriate
- [ ] No `NEXT_PUBLIC_*` variables leak server-side secrets
