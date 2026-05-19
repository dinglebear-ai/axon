# Miles Morales -- Code Review Report

**Date:** 2026-02-19
**Partner:** Gwen Stacy
**Branch:** `chore/housekeeping`
**Issues Assigned:** #52, #54, #55, #57, #58, #59, #68, #69, #80, #81, #84, #85

---

## Summary

Resolved 12 issues across Docker init scripts, shell scripts, and documentation files. All fixes target security hardening, correctness, and consistency.

---

## Issues Resolved

### MAJOR

| # | File | Problem | Fix |
|---|------|---------|-----|
| **#68** | `docker/s6/cont-init.d/10-load-axon-env:22` | `.env` parser treats bare vars (no `=`) as `KEY=KEY`; invalid key names (e.g. `NEXT-PUBLIC-URL`) pass through | Skip lines without `=`; validate keys match `^[A-Za-z_][A-Za-z0-9_]*$` with warning on skip |
| **#69** | `docker/s6/cont-init.d/10-load-axon-env:46` | Path injection via `$output_dir` -- `chown -R`/`chmod -R` on unvalidated path | Validate non-empty, resolve with `realpath -m`, restrict to `/app/*\|/home/*\|/data/*\|/tmp/*` prefixes |
| **#52** | `skills/axon/references/troubleshooting.md:799` | "Command Not Found" section tells users to install Node.js/npm for a Rust binary | Replaced with Rust-appropriate guidance: `which axon`, `cargo build --release`, PATH setup |
| **#85** | `skills/axon/references/troubleshooting.md:712` | "Memory Errors" section references `NODE_OPTIONS="--max-old-space-size=4096"` | Replaced with Rust/Linux guidance: `free -h`, `ulimit -v`, `--batch-concurrency`, batch processing |
| **#57** | `skills/axon/scripts/crawl-site.sh:109` | API key passed as CLI arg visible in `ps` output | Removed `--api-key` flag; pass via `FIRECRAWL_API_KEY=... cmd` environment prefix |
| **#58** | `skills/axon/scripts/scrape.sh:9` | Path uses `$HOME/claude-homelab/.env` (no dot prefix) | Changed to `${ENV_FILE:-$HOME/.claude-homelab/.env}` with dynamic default |
| **#59** | `skills/axon/scripts/search-scrape.sh:18` | Hardcoded `.env` path `~/claude-homelab/.env` | Changed to `${ENV_FILE:-$HOME/.claude-homelab/.env}` |

### Minor

| # | File | Problem | Fix |
|---|------|---------|-----|
| **#54** | `skills/axon/scripts/crawl-site.sh:18` | `--help` unreachable if `.env` missing (sourced before arg parse) | Moved usage() and `--help`/`-h` check before `.env` sourcing |
| **#55** | `skills/axon/scripts/crawl-site.sh:87` | Regex `^[0-9]+$` accepts `0` but error says "positive number" | Changed to `^[1-9][0-9]*$` for both limit and max-depth |
| **#80** | `skills/axon/references/best-practices.md:269` | URL used as filename produces invalid path (`/`, `:` chars) | Added `tr "/:?&#" "_____"` sanitization in xargs example |
| **#81** | `skills/axon/references/best-practices.md:292` | `wait -n` requires bash >= 4.3 | Added `BASH_VERSINFO` check; falls back to `wait` on older bash |
| **#84** | `skills/axon/references/troubleshooting.md:31` | All `.env` paths use `~/claude-homelab/.env` (no dot) | Replaced all occurrences with `~/.claude-homelab/.env` |

---

## Validation

```
$ shellcheck docker/s6/cont-init.d/10-load-axon-env skills/axon/scripts/crawl-site.sh skills/axon/scripts/scrape.sh skills/axon/scripts/search-scrape.sh
# Only SC1090 (non-constant source) warnings -- expected for dynamic ENV_FILE paths
# Zero errors
```

---

## Files Modified

1. `docker/s6/cont-init.d/10-load-axon-env` -- bare var skip, key validation, path injection guard
2. `skills/axon/scripts/crawl-site.sh` -- help before .env, regex fix, API key via env, dynamic .env path
3. `skills/axon/scripts/scrape.sh` -- dynamic .env path, API key via env
4. `skills/axon/scripts/search-scrape.sh` -- dynamic .env path, API key via env
5. `skills/axon/references/troubleshooting.md` -- Node.js references replaced, .env paths fixed
6. `skills/axon/references/best-practices.md` -- URL filename sanitization, wait -n compat
