# Session: CLAUDE.md Audit, Improvements, and New Crate Docs
Date: 2026-03-02
Branch: feat/sidebar

## Session Overview

Full audit and improvement of all CLAUDE.md files in the axon_rust project. Assessed 9 existing files, applied 7 targeted updates, then discovered and filled a critical gap: `crates/cli` and `crates/core` had no CLAUDE.md at all. Dispatched two parallel exploration agents to do complete systematic analysis of both crates, then wrote comprehensive CLAUDE.md files for each. All 8 crates in `crates/` now have documentation.

## Timeline

1. **Skill invocation** — `/claude-md-management:claude-md-improver` invoked
2. **Discovery** — Found 9 active CLAUDE.md files (excluding worktrees and node_modules)
3. **Quality assessment** — Read all 9 files; cross-checked against actual directory contents and MEMORY.md
4. **Quality report** — Issued full scored report (9 files, avg 86/100)
5. **7 targeted updates applied** (user approved all)
6. **Coverage check** — User asked "do we have any crates that don't have a CLAUDE.md?" → found `cli` and `core` missing
7. **Parallel agent dispatch** — Two Explore agents ran concurrently (~2.5 min each):
   - Agent A: complete systematic exploration of `crates/cli/` (27 tool uses)
   - Agent B: complete systematic exploration of `crates/core/` (28 tool uses)
8. **New files written** — `crates/cli/CLAUDE.md` (216 lines) and `crates/core/CLAUDE.md` (258 lines)
9. **Final verification** — All 8 crates confirmed covered

## Key Findings

### Quality Issues Found
- `docs/CLAUDE.md`: directory layout listed only 7 entries; actual `docs/` had 20+ files. `schema.md` → `SCHEMA.md` casing wrong.
- `crates/crawl/CLAUDE.md`: missing `is_junk_discovered_url()` (URL filter hooked via `set_on_link_find`) and mid-crawl Redis cancellation mechanism — both confirmed production features.
- `./CLAUDE.md` (root): `readability: false` critical gotcha only documented in `crates/crawl/CLAUDE.md`, not the root.
- `crates/mcp/CLAUDE.md`: `scrape` and `ask` implemented as MCP action domains but missing from the Domains list. Confirmed via grep of `handlers_query.rs:153` and `handlers_system.rs:186`.
- `crates/jobs/CLAUDE.md`: `refresh/` module listed in layout but never described; AMQP reconnect backoff difference (crawl vs worker_lane) undocumented here despite being documented in root.
- `crates/web/CLAUDE.md`: no port reference, no insta snapshot workflow, no "adding a new route" guidance.
- `crates/ingest/CLAUDE.md`: `github.rs` described as "re-export shim (if present)" — file confirmed present, ambiguity removed.
- `crates/cli/` and `crates/core/`: no CLAUDE.md at all despite being the two most foundational crates.

### Critical Patterns Documented (New)

**`readability: false` gotcha** (`crates/core/content.rs:20`):
- Setting to `true` causes Mozilla Readability to strip VitePress/sidebar docs to just the page title → 97% thin rate
- `clean_html: false` also critical: `[class*='ad']` selector matches Tailwind `shadow-*` classes → wipes all shadowed elements from Tailwind sites

**`is_junk_discovered_url()`** (`crates/crawl/engine.rs`):
- 5 heuristics: URL >2048 chars, `%3C`/`%3E` in path, `%7B`/`%7D`, 3+ `%20` in path, `%20)` JS concat artifact
- Fires via `website.set_on_link_find()` BEFORE blacklist regex

**Mid-crawl Redis cancellation** (`crates/jobs/crawl/runtime/process.rs`):
- `tokio::select!` racing crawl future against `poll_cancel_key` (3s poll)
- Key: `axon:crawl:cancel:{job_id}` — `is_crawl_canceled()` returns `false` on Redis error (fail-safe)

**`maybe_handle_subcommand()` pattern** (`crates/cli/commands/crawl/subcommands.rs`):
- Returns `Ok(true)` if subcommand handled, `Ok(false)` if not
- Caller must check BEFORE treating `positional[0]` as URL

**`start_url_from_cfg()`** (`crates/cli/commands/common.rs`):
- Critical guard — never use `cfg.positional[0]` raw; always call this function first

**Docker URL rewriting** (`crates/core/config/parse/docker.rs`):
- Checks `/.dockerenv` existence; rewrites container hostnames to `127.0.0.1:mapped_port` when outside Docker

**IPv6 SSRF guard** (`crates/core/http/ssrf.rs`):
- Use `host_str()` + `host.parse::<IpAddr>()` — NOT `spider::url::Host::Ipv6` enum match (silent failure, confirmed production bug)

**Config struct literal gotcha** (multiple files):
- Adding non-`Option` fields to `Config` requires updating inline `Config { ... }` literals in `research.rs`, `search.rs`, `make_test_config()` helpers — only caught at test compile time

**Refresh module** (`crates/jobs/refresh/`):
- Periodic URL re-indexing scheduler; `RefreshSchedule` records with `claim_due_refresh_schedules` for overdue detection

## Technical Decisions

- **Verified before adding to MCP domains list**: grepped `handlers_query.rs` and `handlers_system.rs` to confirm `scrape` and `ask` are real MCP domains before adding them. Did NOT add `evaluate` or `suggest` (not found in server handlers).
- **Parallel agent dispatch** over sequential reading: both `crates/cli` and `crates/core` are large (20+ files each); agents ran concurrently in ~2.5 min vs an estimated 10+ min sequential.
- **Did not update docker/CLAUDE.md** in the 7-update batch: scored 95/100, only minor RabbitMQ mgmt UI note missing. User confirmed awareness. Not updated this session.
- **docs/CLAUDE.md flat files**: listed 20 flat docs with brief descriptions rather than deeply documenting each — keeps layout scannable without duplicating each doc's own content.

## Files Modified / Created

| File | Action | Purpose |
|------|--------|---------|
| `./CLAUDE.md` | Modified | Added `readability: false` gotcha (97% thin consequence) |
| `docs/CLAUDE.md` | Modified | Updated directory layout from 7 to 20+ entries; fixed `schema.md` → `SCHEMA.md` casing |
| `crates/crawl/CLAUDE.md` | Modified | Added `is_junk_discovered_url()` section + mid-crawl Redis cancellation section |
| `crates/mcp/CLAUDE.md` | Modified | Added `scrape` and `ask` to Domains list (verified in source) |
| `crates/jobs/CLAUDE.md` | Modified | Added Refresh module description; added AMQP reconnect backoff comparison table; updated refresh/ comment in layout |
| `crates/web/CLAUDE.md` | Modified | Added Ports section (49000), Adding a New HTTP Endpoint steps, insta snapshot workflow |
| `crates/ingest/CLAUDE.md` | Modified | Removed "if present" ambiguity from `github.rs` re-export shim entry |
| `crates/cli/CLAUDE.md` | Created | 216-line complete guide for CLI command orchestration layer |
| `crates/core/CLAUDE.md` | Created | 258-line complete guide for shared infrastructure crate |

## Commands Executed

```bash
# Discovery
find /home/jmagar/workspace/axon_rust -name "CLAUDE.md" | grep -v worktrees | grep -v node_modules
find /home/jmagar/workspace/axon_rust/crates/*/ -name "CLAUDE.md"

# Coverage check
for dir in /home/jmagar/workspace/axon_rust/crates/*/; do
  crate=$(basename "$dir")
  [ -f "$dir/CLAUDE.md" ] && echo "✓ $crate" || echo "✗ $crate"
done
# Result: ✗ cli, ✗ core — triggered agent dispatch

# MCP domain verification
grep -rn '"scrape"\|"ask"' /home/jmagar/workspace/axon_rust/crates/mcp/server/
# handlers_query.rs:153: "scrape" — confirmed
# handlers_system.rs:186: "scrape": ["scrape"] — confirmed

# Refresh module purpose
head -50 /home/jmagar/workspace/axon_rust/crates/jobs/refresh/mod.rs
# Confirmed: periodic URL re-indexing scheduler

# Final verification
for dir in /home/jmagar/workspace/axon_rust/crates/*/; do
  crate=$(basename "$dir")
  [ -f "$dir/CLAUDE.md" ] && echo "✓ $crate" || echo "✗ $crate"
done
# All 8: ✓ cli ✓ core ✓ crawl ✓ ingest ✓ jobs ✓ mcp ✓ vector ✓ web
```

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| Crate coverage | 6/8 crates had CLAUDE.md | 8/8 crates have CLAUDE.md |
| `docs/` layout | Listed 7 items, many docs undiscoverable | Lists all 20+ files with descriptions |
| `readability: false` gotcha | Only in `crates/crawl/CLAUDE.md` | Also in root `CLAUDE.md` |
| MCP domains | `scrape` and `ask` missing from list | Both added (source-verified) |
| `refresh/` module | Listed but unexplained | Described as periodic re-indexing scheduler |
| Crawl junk filter | Undocumented | Fully documented with 5 heuristics |
| Mid-crawl cancel | Undocumented | Redis key pattern, poll interval, fail-safe behavior documented |
| AMQP reconnect | Crawl vs worker_lane difference only in root CLAUDE.md | Now also in `crates/jobs/CLAUDE.md` |

## Verification Evidence

| Check | Expected | Actual | Status |
|-------|----------|--------|--------|
| All 8 crates have CLAUDE.md | ✓ all 8 | ✓ all 8 | PASS |
| `scrape` in MCP handlers | grep match | `handlers_query.rs:153` | PASS |
| `ask` in MCP handlers | grep match | `handlers_query.rs:211` | PASS |
| `evaluate`/`suggest` NOT in MCP | no match | no match | PASS — not added |
| `refresh/mod.rs` describes scheduler | RefreshSchedule exports | Confirmed in source | PASS |
| `github.rs` exists in ingest | present | `crates/ingest/github.rs` confirmed | PASS |
| `docs/SCHEMA.md` casing | uppercase | uppercase confirmed in `ls` | PASS |

## Risks and Rollback

- **Low risk**: all changes are documentation only — no code or configuration modified.
- **Rollback**: `git checkout -- '*.md'` or revert individual files. All 9 modified/created files are tracked.
- **MCP domain list accuracy**: if `scrape`/`ask` are ever removed from the MCP server, the domains list in `crates/mcp/CLAUDE.md` would become stale. Update both in the same commit.

## Decisions Not Taken

- **Did not update `docker/CLAUDE.md`**: scored 95/100. Only gap was RabbitMQ mgmt UI access note. Not worth a change for a single minor item.
- **Did not add `evaluate`/`suggest` to MCP domains**: could not confirm in source. Explicitly excluded to avoid phantom entries.
- **Did not rewrite any existing CLAUDE.md from scratch**: quality was sufficient; targeted additions only. Rewrites risk losing accurate nuance.
- **Did not update root CLAUDE.md commands table** to add `screenshot`: noticed `Screenshot` in `CommandKind` but was not asked to extend the commands table. Left as open question.

## Open Questions

- **Root CLAUDE.md commands table** is missing `screenshot`, `refresh`, and `dedupe` commands (present in `CommandKind` enum but absent from the table). Should these be documented?
- **`docker/CLAUDE.md`**: RabbitMQ management UI port 15672 is in the port table but no note on accessing it from host or default credentials location. Minor but worth a follow-up.
- **`monolith-policy.md`**: listed in old `docs/CLAUDE.md` directory layout but not found in actual `ls docs/` output. Was it deleted/renamed? Should verify.
- **`crates/cli/commands/screenshot/`**: the `screenshot` command uses Chrome CDP but there's no note in `crates/crawl/CLAUDE.md` about the shared Chrome bootstrap pattern between crawl and screenshot.

## Next Steps

- Verify `docs/monolith-policy.md` existence (may have been removed or renamed)
- Consider adding `screenshot`, `refresh`, `dedupe` to root CLAUDE.md commands table
- Consider adding RabbitMQ mgmt UI note to `docker/CLAUDE.md`
- Run `just verify` to confirm no regressions from any file touches (docs only, so should be clean)
