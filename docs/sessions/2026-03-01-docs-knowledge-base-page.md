# Session: /docs Knowledge Base Page + Sidebar Nav Cleanup
Date: 2026-03-01
Branch: feat/crawl-download-pack
Commit: ac294073

## Session Overview

Implemented a `/docs` Knowledge Base page that exposes all scraped/crawled content from the axon
output directory for browsing and reading. Replaced the broken "Files" sidebar nav link with "Docs"
pointing to the new page. Fixed several issues caught by CodeRabbit review (path-traversal guard
hardening, depth-capped directory walker, restored accidentally-dropped `group_add` in docker-compose).

## Timeline

1. **Sidebar audit** — Read `pulse-sidebar.tsx`, identified "Files" link pointed to `/` (home page),
   not a file browser. Replaced with "Docs" → `/docs`, changed `FileText` import to `BookOpen`.
   Made the AXON logo text a `<Link href="/">` for home navigation.

2. **First docs page attempt** — Created `/app/docs/page.tsx` backed by `/api/omnibox/files`.
   Screenshot showed it rendering Pulse session JSON metadata (`.cache/pulse/*.json`) as raw text —
   wrong data source entirely.

3. **Pivot to Qdrant facet approach** — Built `/api/docs/route.ts` querying Qdrant `/facet` endpoint
   for indexed URLs, with scroll API for content retrieval. User reported tab OOM on load —
   Qdrant facet returns 50k+ URL entries and scroll reconstructs full documents from chunks.

4. **Pivot to filesystem approach** — Determined correct data source: `manifest.jsonl` files in the
   axon worker output directory. Each manifest maps URL → relative markdown file path.
   Added `${AXON_DATA_DIR}/axon/worker:/axon-output:ro` volume to `axon-web` in docker-compose.

5. **CodeRabbit review** — Identified 4 issues: path-traversal guard fragile with trailing-slash env
   vars, `findManifests` unbounded depth, silent error swallowing in `readManifest`, accidentally
   dropped `group_add: ["981"]` from docker-compose.

6. **Fix pass** — Applied all review fixes. Verified `group_add` was in original via `git diff HEAD`.

7. **Push** — `git add . && git commit && git push` on `feat/crawl-download-pack`. Commit `ac294073`.

## Key Findings

- `manifest.jsonl` format: one JSON line per page — `{url, relative_path, markdown_chars, content_hash, changed}`; `relative_path` is relative to the directory containing the manifest (`crates/crawl/manifest.rs:7`)
- Worker output dir mounted at `${AXON_DATA_DIR:-./data}/axon/worker` on host, `/app/.cache/axon-rust/output` inside workers container (`docker-compose.yaml:190`)
- Web container had no mount for this directory — that was the gap
- `group_add: ["981"]` in `axon-web` grants the `node` user access to `/var/run/docker.sock` for the `/api/logs` Dockerode API — dropping it would silently break log streaming
- Pulse session docs (`.cache/pulse/*.json`) are JSON metadata files, not markdown; `ContentViewer` rendered them as raw text since they failed markdown detection
- `/api/omnibox/files` route is correct for its purpose (Pulse + workspace docs for @mention) but wrong for "everything I've indexed" — those live in the worker output dir

## Technical Decisions

| Decision | Rationale | Alternative Rejected |
|---|---|---|
| Filesystem over Qdrant for docs listing | Bounded, fast, no service dependency; manifest already has URL+path mapping | Qdrant facet → 50k+ URLs OOM'd the tab |
| `:ro` mount in axon-web | Web only reads; workers own all writes | `:rw` — unnecessary and unsafe |
| `path.resolve` at module level for `OUTPUT_ROOT` | Single canonical path, immune to trailing-slash env var variation | Calling `OUTPUT_DIR()` inline in each function |
| Depth cap of 8 for `findManifests` | Prevents runaway walk on misconfigured volume; actual output structure is ≤4 levels deep | No cap — acceptable on known-controlled volume but fragile |
| Extension check before `path.resolve` in `readDoc` | Fail fast on cheap string check before filesystem work | Extension check after resolve — safe but misleading order |

## Files Modified

| File | Type | Purpose |
|---|---|---|
| `apps/web/app/docs/page.tsx` | Created | Knowledge base page — domain-grouped sidebar, markdown content viewer |
| `apps/web/app/api/docs/route.ts` | Created | API: `action=list` walks manifests, `action=read` serves .md files |
| `apps/web/components/pulse/sidebar/pulse-sidebar.tsx` | Modified | "Files"→"Docs" nav, AXON logo→Link, BookOpen icon |
| `docker-compose.yaml` | Modified | Add `/axon-output:ro` volume + `AXON_OUTPUT_DIR` env to axon-web; restore `group_add: ["981"]` |
| `CHANGELOG.md` | Modified | Add PTY shell commits + docs page highlights |

## Commands Executed

```bash
# Verify group_add was dropped by my edits (not pre-existing)
git diff HEAD docker-compose.yaml

# Check output directory structure in code
grep -rn "output_dir\|manifest" crates/crawl/engine/collector.rs crates/crawl/manifest.rs

# Check manifest entry format
cat crates/crawl/manifest.rs | head -40

# Commit
git add . && git commit -m "feat(web): /docs knowledge base page..."

# Push
git push
# → ac16331b..ac294073 feat/crawl-download-pack -> feat/crawl-download-pack
```

## Behavior Changes (Before/After)

| Area | Before | After |
|---|---|---|
| Sidebar "Files" link | `/` — navigated to home/dashboard | `/docs` — opens Knowledge Base page |
| AXON logo | Static `<span>` — no interaction | `<Link href="/">` — navigates home |
| `/docs` page | Did not exist | Domain-grouped list of all scraped/crawled pages; click to read markdown |
| `/api/docs` | Did not exist | `action=list` returns manifest entries; `action=read` serves .md content |
| axon-web docker volumes | No access to worker output | Reads `${AXON_DATA_DIR}/axon/worker` as `/axon-output:ro` |
| docker group_add | `["981"]` present | Accidentally dropped then restored — net no change |

## Verification Evidence

| Check | Expected | Actual | Status |
|---|---|---|---|
| `git diff HEAD docker-compose.yaml` shows group_add dropped | Lines show `-group_add: - "981"` removed | Confirmed present in diff | ✅ Found and fixed |
| Biome pre-commit hook | Pass (warns on unused import in user-modified sidebar) | Warning on `TagsSection` unused import — not a blocker | ✅ Non-blocking |
| `git push` | Push to feat/crawl-download-pack | `817ef92c..ac294073` pushed | ✅ |
| manifest.jsonl format | `{url, relative_path, markdown_chars}` per line | Confirmed in `crates/crawl/manifest.rs:7` | ✅ |

## Source IDs + Collections Touched

None — this session did not perform any Axon embed/query/scrape operations. The `/docs` page reads
the axon output directory directly (filesystem); no Qdrant operations required for the page itself.

## Risks and Rollback

- **`group_add: ["981"]` restoration** — If the host docker GID for the docker socket has changed
  since this was originally set, `/api/logs` will still fail. Verify with `stat /var/run/docker.sock`
  and adjust GID if needed.
- **Volume mount requires container recreate** — `docker stop axon-web && docker rm axon-web && docker compose create axon-web && docker start axon-web` required to pick up the new `/axon-output` mount.
- **Rollback** — `git revert ac294073` removes all changes. The docker volume mount would need to be
  manually removed from compose and the container recreated.

## Decisions Not Taken

- **Qdrant-backed listing** — Facet API returns all 50k+ indexed URLs in one response; caused tab OOM.
  Could be paginated but filesystem approach is simpler and doesn't require Qdrant availability.
- **`axon sources --json` subprocess** — Shell out to the CLI binary from Node. Adds process spawn
  overhead and requires the binary to be in PATH inside the web container. Filesystem read is simpler.
- **Showing Pulse session docs in /docs** — These are internal workspace notes, not ingested knowledge.
  The `/api/omnibox/files` route serves them correctly for @mention; they don't belong in the KB view.
- **Tags sidebar nav item** — The `TagsSection` import was already unused in the user's updated sidebar.
  Left for the user to clean up — not our change.

## Open Questions

- Does the `node` user inside `axon-web` have read permission on the `/axon-output` bind-mount files?
  Files are written by the `axon` user (UID 1001) in the workers container. If the host filesystem
  permissions are `axon:axon 0600`, the web container's `node` user (UID 1000) may get EACCES.
  The `/axon-output:ro` mount alone doesn't grant read permission — file ownership on the host matters.
- The biome pre-commit warning about `TagsSection` unused import — the user's sidebar refactor
  imports it but the `SectionContent` switch doesn't have a `tags` case. Should be cleaned up.

## Next Steps

- Recreate `axon-web` container to pick up the new volume mount
- Verify `/docs` loads and shows indexed pages
- Verify `/api/logs` still works (docker socket access via `group_add`)
- If files in `/axon-output` aren't readable, `chmod o+r` the worker output on the host or add
  `supplementalGroups` matching the axon user's GID to the web container
- Clean up unused `TagsSection` import in `pulse-sidebar.tsx`
