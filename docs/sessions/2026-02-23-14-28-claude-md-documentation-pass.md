# Session: CLAUDE.md Documentation Pass
**14:28:58 | 02/23/2026**

## Session Overview

Full documentation audit and expansion pass for the axon_rust project. Created 5 nested `CLAUDE.md` files covering subsystems with dense non-obvious patterns (jobs, vector, crawl, ingest, docker), updated the root `CLAUDE.md` with missing context, added TEI remote service configuration details from live inspection of `steamy-wsl`, and created `AGENTS.md`/`GEMINI.md` symlinks at every level. All 6 files pushed from B-range scores to 90+ (A grade).

Also diagnosed and fixed a Serena MCP tool failure: no active project was set, requiring `activate_project("axon_rust")` to be called at session start.

---

## Timeline

| Time | Activity |
|------|----------|
| 14:13 | Created root `AGENTS.md` → `CLAUDE.md` and `GEMINI.md` → `CLAUDE.md` symlinks |
| 14:14 | Ran `/claude-md-management:claude-md-improver` with "suggest and create nested CLAUDE.md files" — explored codebase, assessed gaps, created 5 nested files |
| 14:16 | Diagnosed Serena MCP failure: `activate_project("axon_rust")` was missing |
| 14:18 | SSH'd into `steamy-wsl` to read `~/compose/tei/docker-compose.yaml` and `.env` |
| 14:19 | Added TEI section to `crates/vector/CLAUDE.md` (model, pooling, default prompt, auto-truncate, remote host) |
| 14:22 | Ran second `/claude-md-management:claude-md-improver` — full quality audit of all 6 files |
| 14:24 | Applied proposed updates: worker count fix, `just` section, `spider_agent` gotcha, Config literal gotcha, docker introspection |
| 14:27 | Added Testing sections, Troubleshooting table, Known Gaps, env vars table to all nested files — pushed all to 90+ |
| 14:27 | Created `AGENTS.md`/`GEMINI.md` symlinks in all 5 nested directories |

---

## Key Findings

- **Serena requires explicit project activation**: `mcp__plugin_serena_serena__activate_project("axon_rust")` must be called at session start or Serena tools return "No active project" errors.
- **TEI is on steamy-wsl (RTX 4070)**: Model is `Qwen/Qwen3-Embedding-0.6B`, `last-token` pooling, with a default query instruction prompt auto-prepended to all requests. Never on localhost.
- **`--default-prompt` in TEI is asymmetric**: The instruction prefix `"Instruct: ...\nQuery: "` is prepended to all embed requests — both query and document text. Code does not need to handle this manually.
- **Root CLAUDE.md had stale worker count**: Said "4 workers" but ingest-worker is now the 5th.
- **`just` commands were entirely absent** from CLAUDE.md despite a full Justfile with `just verify`, `just fix`, `just precommit`, `just watch-check`, `just rebuild`.

---

## Technical Decisions

- **TEI `HF_TOKEN` not included in CLAUDE.md**: It's a secret from the `.env` file. Only model name, pooling strategy, prompt prefix, and operational URL documented.
- **No `crates/core/CLAUDE.md` or `crates/cli/CLAUDE.md`**: These subsystems are well-covered in root CLAUDE.md already; adding nested files would duplicate, not add.
- **`scripts/CLAUDE.md` skipped**: Scripts are simple enough that inline comments suffice; no dense non-obvious patterns warranting a CLAUDE.md.
- **`ingest errors` gap documented as Known Gap**: Rather than fixing the silently-unhandled subcommand in this session, it's documented clearly in `crates/ingest/CLAUDE.md` with the fix path noted.
- **All nested dirs get AGENTS.md + GEMINI.md symlinks**: Matches the root convention and ensures Codex/Gemini agents get the same context as Claude in all subdirectories.

---

## Files Modified/Created

### Created
| File | Purpose |
|------|---------|
| `AGENTS.md` | Symlink → `CLAUDE.md` (Codex compatibility) |
| `GEMINI.md` | Symlink → `CLAUDE.md` (Gemini compatibility) |
| `crates/jobs/CLAUDE.md` | Jobs subsystem: AMQP lifecycle, JobStatus enum, pool consolidation, bounded channels, stale recovery, worker_lane, queue injection |
| `crates/jobs/AGENTS.md` | Symlink → `CLAUDE.md` |
| `crates/jobs/GEMINI.md` | Symlink → `CLAUDE.md` |
| `crates/vector/CLAUDE.md` | Vector subsystem: TEI 413/429, ensure_collection GET-first, facet vs scroll, ranking pipeline, TEI remote config |
| `crates/vector/AGENTS.md` | Symlink → `CLAUDE.md` |
| `crates/vector/GEMINI.md` | Symlink → `CLAUDE.md` |
| `crates/crawl/CLAUDE.md` | Crawl engine: crawl_raw vs crawl, configure_website chain, readability:false root cause, auto-switch, troubleshooting table |
| `crates/crawl/AGENTS.md` | Symlink → `CLAUDE.md` |
| `crates/crawl/GEMINI.md` | Symlink → `CLAUDE.md` |
| `crates/ingest/CLAUDE.md` | Ingest sources: octocrab, Reddit OAuth2, yt-dlp subprocess, session parsers, known gaps, yt-dlp PATH requirement |
| `crates/ingest/AGENTS.md` | Symlink → `CLAUDE.md` |
| `crates/ingest/GEMINI.md` | Symlink → `CLAUDE.md` |
| `docker/CLAUDE.md` | Docker/s6: s6-overlay root requirement, adding workers, just shortcuts, container introspection, port table |
| `docker/AGENTS.md` | Symlink → `CLAUDE.md` |
| `docker/GEMINI.md` | Symlink → `CLAUDE.md` |

### Modified
| File | Changes |
|------|---------|
| `CLAUDE.md` | Worker count 4→5; added `just` commands section; added `spider_agent` path dep gotcha; added Config struct literal update trap |
| `crates/vector/CLAUDE.md` | Added TEI Service section (model, pooling, default prompt, auto-truncate, connectivity); added Testing section; added Key Env Vars table |
| `crates/jobs/CLAUDE.md` | Added Testing section with live-service caveat |
| `crates/crawl/CLAUDE.md` | Added Testing section; added Troubleshooting table |
| `crates/ingest/CLAUDE.md` | Added Testing section; added Known Gaps table; added explicit yt-dlp PATH error message |
| `docker/CLAUDE.md` | Added `just` shortcuts section; added Container Introspection section |

---

## Commands Executed

```bash
# Symlinks
ln -sf CLAUDE.md AGENTS.md && ln -sf CLAUDE.md GEMINI.md
# → lrwxrwxrwx ... AGENTS.md -> CLAUDE.md / GEMINI.md -> CLAUDE.md

# TEI config inspection (steamy-wsl)
ssh steamy-wsl cat ~/compose/tei/docker-compose.yaml
ssh steamy-wsl cat ~/compose/tei/.env
# → Model: Qwen/Qwen3-Embedding-0.6B, port 52000, last-token pooling, default-prompt set

# Nested symlinks (all 5 dirs)
for dir in crates/jobs crates/vector crates/crawl crates/ingest docker; do
  (cd /home/jmagar/workspace/axon_rust/$dir && ln -sf CLAUDE.md AGENTS.md && ln -sf CLAUDE.md GEMINI.md)
done
# → 5x lrwxrwxrwx ... AGENTS.md -> CLAUDE.md confirmed
```

---

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| AI context for jobs subsystem | Root CLAUDE.md only — no AMQP/pool/JobStatus patterns | `crates/jobs/CLAUDE.md` with full lifecycle, testing, and gotchas |
| AI context for vector subsystem | Root only — no TEI model info, no facet vs scroll guidance | `crates/vector/CLAUDE.md` with TEI remote config, perf table, testing |
| AI context for crawl subsystem | Root only — some gotchas duplicated, no troubleshooting | `crates/crawl/CLAUDE.md` with full engine details, troubleshoot table |
| AI context for ingest subsystem | Root only — no source-specific API patterns | `crates/ingest/CLAUDE.md` with octocrab/OAuth2/yt-dlp patterns, known gaps |
| AI context for docker | Root only — no s6-overlay specifics | `docker/CLAUDE.md` with worker management, introspection, port table |
| Codex/Gemini compatibility | Root only had AGENTS.md/GEMINI.md | All 6 directories now have AGENTS.md/GEMINI.md symlinks |
| just workflow visibility | Completely absent from docs | `just verify/fix/precommit/watch-check/rebuild/up/down` all documented |
| TEI location awareness | "external service, set TEI_URL" | `steamy-wsl:52000`, Qwen3, last-token, default-prompt, auto-truncate all documented |
| Worker count | "4 workers" (stale) | "5 workers (crawl/batch/extract/embed/ingest)" |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `ls -la AGENTS.md GEMINI.md` | symlinks → CLAUDE.md | `lrwxrwxrwx ... -> CLAUDE.md` (both) | ✅ |
| `ls -la crates/*/AGENTS.md docker/AGENTS.md` | 5 symlinks → CLAUDE.md | All 5 confirmed `-> CLAUDE.md` | ✅ |
| `ssh steamy-wsl cat ~/compose/tei/docker-compose.yaml` | TEI compose config | Full config returned: model, ports, pooling | ✅ |
| `ssh steamy-wsl cat ~/compose/tei/.env` | TEI env vars | `TEI_EMBEDDING_MODEL=Qwen/Qwen3-Embedding-0.6B`, port 52000 | ✅ |
| `wc -l crates/*/CLAUDE.md docker/CLAUDE.md` | Non-zero files | 57/81/54/62/90 lines (pre-update); all grown after updates | ✅ |
| Justfile content | `just verify`, `just fix`, etc. | 16 recipes confirmed including `just up/down/rebuild` | ✅ |

---

## Source IDs + Collections Touched

No Axon embed/retrieve operations were performed during this session (documentation-only session). TEI service was read from `steamy-wsl` for informational purposes only — no embedding occurred.

---

## Risks and Rollback

- **Risk**: Nested CLAUDE.md files may drift from reality as code evolves. Mitigation: run `/claude-md-management:claude-md-improver` periodically (suggested: after major refactors).
- **Risk**: `ingest errors` known gap is documented but unfixed — a user running `axon ingest errors <uuid>` will get a confusing error. Low urgency but should be fixed.
- **Rollback**: All changes are documentation only (symlinks + markdown). `git checkout CLAUDE.md` restores root; `git rm crates/*/CLAUDE.md docker/CLAUDE.md` removes nested files. No code behavior changed.

---

## Decisions Not Taken

- **`crates/core/CLAUDE.md`**: Skipped — core module patterns are already well-documented in root CLAUDE.md (config.rs, http.rs SSRF guard, content.rs). Adding a nested file would duplicate, not add.
- **`crates/cli/CLAUDE.md`**: Skipped — CLI command structure is covered in root CLAUDE.md's Commands table; no hidden gotchas specific enough to warrant a separate file.
- **`scripts/CLAUDE.md`**: Skipped — scripts are standalone utilities with clear names; no non-obvious patterns.
- **Including HF_TOKEN in CLAUDE.md**: Rejected — it's a secret. Only operational metadata documented.

---

## Open Questions

- Is `--default-prompt` in TEI actually desirable for document embedding (not just queries)? Qwen3-Embedding is asymmetric, but the current config applies the query prefix to both. Could affect recall if document embeddings should be query-instruction-free.
- Should `ingest errors <uuid>` be fixed in the same sprint as other ingest completion work, or tracked as a separate issue?
- `crates/core/config/` subdirectory exists (noted in `ls` output) but architecture tree shows `config.rs` as a flat file. Are there split module files that should be reflected in docs?

---

## Next Steps

- [ ] Fix `ingest errors <uuid>` silently-unhandled gap in `crates/jobs/ingest_jobs.rs`
- [ ] Investigate `crates/core/config/` directory vs `config.rs` file discrepancy — update architecture tree if needed
- [ ] Consider adding `crates/core/.claude.local.md` if config module has grown complex enough to warrant it
- [ ] Re-run `claude-md-management:claude-md-improver` after next major code refactor to keep scores above 90
