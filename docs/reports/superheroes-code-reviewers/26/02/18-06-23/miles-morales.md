# Miles Morales — Issue Resolution Report

**Date:** 2026-02-18
**Partner:** Gwen Stacy
**Assigned Issues:** P0-10, P1-08, P1-13, P1-14, P2-04, P2-08, P2-14, P2-15

## Summary

All 8 assigned issues resolved. Work covered CI/CD pipeline creation, dependency security scanning, documentation accuracy, dead code cleanup, build optimization, Docker image pinning, and comprehensive CLAUDE.md documentation updates.

---

## Issue Resolutions

### P0-10 (Critical): No CI/CD Pipeline

**Root cause:** No `.github/workflows/` directory existed. Zero build gates on any push/PR.

**Fix:**
- Created `.github/workflows/ci.yml` with two parallel jobs:
  - **check:** cargo check, clippy (warnings = errors), fmt, test
  - **security:** cargo-audit (CVE scanning), cargo-deny (license + ban policy)
- Both jobs use `actions/checkout@v4`, `dtolnay/rust-toolchain@stable`, `Swatinem/rust-cache@v2`
- Created `rust-toolchain.toml` pinning to `stable` channel

**Files created:**
- `.github/workflows/ci.yml`
- `rust-toolchain.toml`

**Verification:** YAML syntax validated (parsed without errors).

---

### P1-08 (High): No Dependency CVE Scanning

**Root cause:** 5,667-line `Cargo.lock` with no CVE visibility or license enforcement.

**Fix:**
- Created `deny.toml` with:
  - Allowed licenses: MIT, Apache-2.0, BSD-2/3-Clause, ISC, Unicode-3.0, Unicode-DFS-2016, Zlib, OpenSSL
  - Copyleft: warn
  - Multiple versions: warn
  - Wildcards: warn
  - Unknown registries/git: deny
- `cargo-deny` step integrated into CI workflow (part of P0-10 security job)

**Files created:**
- `deny.toml`

---

### P1-13 (High): Binary Name Mismatch in CLAUDE.md

**Root cause:** Previously documented `axon_cli_rust` references.

**Finding:** CLAUDE.md already uses `cortex` everywhere. Grepped for `axon_cli_rust` — zero matches. This was already fixed in a prior session.

**Status:** Verified clean. No changes needed.

---

### P1-14 (High): passthrough.rs Dead Code

**Root cause:** Architecture diagram in CLAUDE.md listed `passthrough.rs` but the file does not exist on disk and is not declared in `commands/mod.rs`.

**Fix:** Removed `passthrough.rs` line from the architecture diagram in CLAUDE.md.

**Files modified:**
- `CLAUDE.md` (line 81-82: removed passthrough.rs entry, changed `common.rs` from `├──` to `└──`)

---

### P2-04 (Medium): No rust-toolchain.toml + No Release Optimizations

**Root cause:** No pinned toolchain, no `[profile.release]` in Cargo.toml.

**Fix:**
- `rust-toolchain.toml` already created as part of P0-10
- Added `[profile.release]` to Cargo.toml:
  - `opt-level = 3` (maximum optimization)
  - `lto = "thin"` (link-time optimization)
  - `codegen-units = 1` (better optimization at cost of compile time)
  - `strip = true` (strip debug symbols from release binary)

**Files modified:**
- `Cargo.toml`

---

### P2-08 (Medium): Floating Image Tags in docker-compose.yaml

**Root cause:** Three services used rolling tags that could break on any `docker pull`.

**Fix:**
- `redis:alpine` -> `redis:7.4-alpine`
- `rabbitmq:management` -> `rabbitmq:4.0-management`
- `qdrant/qdrant:latest` -> `qdrant/qdrant:v1.13.1`
- `postgres:17-alpine` was already pinned

**Files modified:**
- `docker-compose.yaml`
- `CLAUDE.md` (Docker Services table updated to match)

**Verification:** `docker compose config --quiet` passed.

---

### P2-14 (Medium): 26 Global CLI Flags Undocumented in CLAUDE.md

**Root cause:** CLAUDE.md documented 11 of 37 GlobalArgs flags.

**Fix:** Replaced the "Key Global Flags" section with a comprehensive "Global Flags Reference" organized into 6 categories:
1. **Core Behavior** (3 flags): `--wait`, `--yes`, `--json`
2. **Crawl & Scrape** (11 flags): `--max-pages`, `--max-depth`, `--render-mode`, `--format`, `--include-subdomains`, `--respect-robots`, `--discover-sitemaps`, `--max-sitemaps`, `--min-markdown-chars`, `--drop-thin-markdown`, `--delay-ms`
3. **Output** (2 flags): `--output-dir`, `--output`
4. **Vector & Embedding** (5 flags): `--collection`, `--embed`, `--limit`, `--query`, `--urls`
5. **Performance Tuning** (8 flags): `--performance-profile`, `--batch-concurrency`, `--concurrency-limit`, `--crawl-concurrency-limit`, `--sitemap-concurrency-limit`, `--backfill-concurrency-limit`, `--request-timeout-ms`, `--fetch-retries`, `--retry-backoff-ms`
6. **Service URLs** (8 flags): all `--pg-url`, `--redis-url`, etc. with env var fallbacks
7. **Queue Configuration** (5 flags): `--shared-queue`, `--crawl-queue`, etc.

Highlighted high-impact flags: `--respect-robots` (defaults false), `--include-subdomains` (defaults true), `--delay-ms`, `--drop-thin-markdown`.

**Files modified:**
- `CLAUDE.md`

---

### P2-15 (Medium): Database Schema Not Documented in CLAUDE.md

**Root cause:** Four auto-created Postgres tables with no documentation anywhere.

**Fix:** Added "Database Schema" section to CLAUDE.md with complete column definitions for all 4 tables:
- `axon_crawl_jobs` (10 columns + 1 index) — source: `crates/jobs/crawl_jobs.rs:107-118`
- `axon_batch_jobs` (10 columns) — source: `crates/jobs/batch_jobs.rs:58-69`
- `axon_extract_jobs` (10 columns) — source: `crates/jobs/extract_jobs.rs:55-66`
- `axon_embed_jobs` (10 columns) — source: `crates/jobs/embed_jobs.rs:54-65`

Each table includes column name, type, nullable, default, and description. Noted the `CREATE TABLE IF NOT EXISTS` auto-creation pattern and the `ensure_schema()` entry point.

**Files modified:**
- `CLAUDE.md`

---

## Coordination Notes

- Coordinated with Gwen Stacy on `docker-compose.yaml` — completed image tag pins before her resource limits and Qdrant healthcheck edits.
- No file conflicts encountered.

## Files Changed Summary

| File | Action | Issues |
|------|--------|--------|
| `.github/workflows/ci.yml` | Created | P0-10, P1-08 |
| `rust-toolchain.toml` | Created | P0-10, P2-04 |
| `deny.toml` | Created | P1-08 |
| `Cargo.toml` | Modified (added [profile.release]) | P2-04 |
| `docker-compose.yaml` | Modified (pinned image tags) | P2-08 |
| `CLAUDE.md` | Modified (removed passthrough.rs, updated Docker table, added full flags reference, added DB schema) | P1-13, P1-14, P2-08, P2-14, P2-15 |
