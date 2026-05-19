---
date: 2026-05-09 03:17:43 EST
repo: git@github.com:jmagar/axon.git
branch: chore/canonical-axon-home
head: 6f5ff6d0
plan: none
agent: Codex
session id: 019e0b99-8701-7be1-bcd5-1726e0d805e5
transcript: /home/jmagar/.codex/sessions/2026/05/09/rollout-2026-05-09T03-17-51-019e0b99-8701-7be1-bcd5-1726e0d805e5.jsonl
working directory: /home/jmagar/workspace/axon_rust
worktree: /home/jmagar/workspace/axon_rust  6f5ff6d0 [chore/canonical-axon-home]
pr: none
---

# Mem0 Ask Retrieval Debug Session

## User Request

Investigate why `axon ask "how do i setup self-hosted mem0 docker compose with qdrant and hugginface"` said there were no relevant sources even though `https://docs.mem0.ai` had just been crawled.

## Session Overview

- Traced the failure through status, Qdrant domains/sources, job rows, container logs, and embed job configuration.
- Found that async job snapshots serialized host-local service URLs, causing Docker workers to call `127.0.0.1` inside the `axon` container.
- Patched job config snapshotting so loopback/unspecified endpoint hosts fall back to the worker process config.
- Repaired the current mem0 index by re-embedding the crawled markdown with the crawl manifest present.
- Rebuilt and installed the host binary, rebuilt `axon:local`, and recreated the live `axon` service.

## Sequence of Events

1. Confirmed `axon ask` was retrieving unrelated OpenClaw and Claude Code sources.
2. Checked `axon domains` and saw no `mem0.ai` or `docs.mem0.ai` vectors in the active collection.
3. Inspected `axon status` and SQLite job rows; the mem0 crawl completed, but embed jobs had `chunks_embedded: 0`.
4. Read Docker logs and found TEI requests going to `http://127.0.0.1:52000/embed` from inside the `axon` container.
5. Verified persisted crawl/embed `config_json` contained `tei_url=http://127.0.0.1:52000/` and `qdrant_url=http://127.0.0.1:53333/`.
6. Patched snapshot logic, added regression coverage, canceled the bad mem0 embed job, and re-embedded from the copied crawl output plus `manifest.jsonl`.
7. Verified `docs.mem0.ai` retrieval and `axon ask`, then rebuilt/restarted the live runtime.

## Key Findings

- `axon domains` initially listed `agent-browser.dev`, `code.claude.com`, `docs.openclaw.ai`, and `ui.shadcn.com`, but no mem0 domains.
- The mem0 docs crawl row had `md_created=211`, while its embed row was `running` or later `canceled` with `chunks_embedded=0`.
- The failing embed job config persisted loopback endpoints: `tei_url=http://127.0.0.1:52000/` and `qdrant_url=http://127.0.0.1:53333/`.
- Docker logs showed repeated `TEI request transport error for http://127.0.0.1:52000/embed`.
- The endpoint snapshot code treated non-credentialed loopback URLs as serializable public URLs before the fix: `src/jobs/lite/config_snapshot.rs:490`.
- The regression test now covers `127.0.0.1`, `localhost`, and `[::1]`: `src/jobs/lite/workers/runners.rs:145`.

## Technical Decisions

- Fixed the config snapshot boundary rather than special-casing mem0 jobs, because the defect affects any async job submitted from the host and executed by a container worker.
- Kept DNS/service endpoint URLs such as `http://axon-tei:80` serializable, while treating process-local hosts as worker-local fallback values.
- Re-embedded from host-published ports after a container one-shot embed using reqwest still failed against `axon-tei`/`axon-qdrant`; this got the data repaired without blocking on that separate networking/client question.
- Copied `manifest.jsonl` alongside `markdown/` before the final re-embed so Qdrant payload URLs were real `https://docs.mem0.ai/...` sources instead of `/tmp/...` file paths.

## Files Modified

- `src/jobs/lite/config_snapshot.rs`: added `endpoint_host_is_process_local()` and excluded loopback/unspecified hosts from serialized endpoint snapshots.
- `src/jobs/lite/workers/runners.rs`: added a regression test proving process-local TEI/Qdrant/OpenAI base URLs fall back to worker config.
- `docs/sessions/2026-05-09-mem0-ask-retrieval-debug.md`: this session note.

Pre-existing unrelated dirty files were present before the session and were not intentionally edited as part of this fix.

## Commands Executed

- `axon status`: showed the mem0 crawl completed and the docs embed job stuck/canceled with zero chunks.
- `axon domains`: initially showed no mem0 domain; after repair showed `docs.mem0.ai vectors=1730`.
- `axon query "self-hosted mem0 docker compose qdrant huggingface" --limit 3`: after repair returned `https://docs.mem0.ai/open-source/features/rest-api`, `https://docs.mem0.ai/open-source/overview`, and `https://docs.mem0.ai/components/vectordbs/dbs/qdrant`.
- `sqlite3 /home/jmagar/.axon/jobs.db ...`: confirmed persisted job configs contained loopback TEI/Qdrant URLs.
- `docker logs --tail 200 axon`: showed repeated TEI transport errors to `127.0.0.1:52000`.
- `docker cp ... manifest.jsonl` and `docker cp ... markdown`: copied crawled mem0 output out of the container for host-side repair embedding.
- `axon --tei-url http://127.0.0.1:52000 --qdrant-url http://127.0.0.1:53333 --collection axon embed /tmp/axon-mem0-docs/markdown --wait true`: embedded `1730 chunks from 211 docs into axon`.
- `cargo build --release --bin axon`: built the patched release binary.
- `cp target/release/axon /home/jmagar/.claude/plugins/cache/jmagar-lab/axon/575c090bcaf5/bin/axon`: replaced the installed symlink target used by `/home/jmagar/.local/bin/axon`.
- `docker compose build axon`: rebuilt `axon:local`.
- `docker compose up -d --no-deps axon`: recreated only the `axon` container.

## Errors Encountered

- `docker compose up -d axon` attempted to recreate dependency containers and failed on existing named `axon-tei`; reran with `--no-deps`.
- A first test assertion failed because serialized config contained `::1` in bracketed form; fixed host normalization to trim IPv6 brackets.
- A one-shot container embed using `http://axon-tei:80` and `http://axon-qdrant:6333` still logged reqwest transport errors even though `curl` from the same container succeeded. This was bypassed by host-side repair embedding and left as an open question.
- The first repair embed omitted `manifest.jsonl`, producing `unknown` domain file-path sources. Deleted those points and re-embedded with the manifest present.

## Behavior Changes (Before/After)

| Area | Before | After |
| --- | --- | --- |
| Async job config snapshots | Host-local service URLs could be replayed by Docker workers. | Loopback/unspecified endpoint URLs fall back to worker process config. |
| Mem0 retrieval | `ask` retrieved OpenClaw/Claude Code sources for the mem0 setup question. | Retrieval returns `docs.mem0.ai` sources for the mem0 setup question. |
| Active Qdrant collection | No mem0 domain vectors. | `docs.mem0.ai vectors=1730`. |
| Runtime | Host binary and `axon` service used the prior build. | Host installed binary and `axon:local` container were rebuilt from the patched source. |

## Verification Evidence

| command | expected | actual | status |
| --- | --- | --- | --- |
| `cargo test -q lite_config_snapshot` | Snapshot regression tests pass. | `4 passed; 0 failed`. | pass |
| `cargo fmt --check` | Formatting is clean. | No output, exit 0. | pass |
| `git diff --check` | No whitespace errors. | No output, exit 0. | pass |
| `axon domains` | `docs.mem0.ai` appears with vectors. | `docs.mem0.ai vectors=1730`. | pass |
| `axon query "self-hosted mem0 docker compose qdrant huggingface" --limit 3` | Top results are mem0 docs. | Top results were `docs.mem0.ai/open-source/features/rest-api`, `/open-source/overview`, and `/components/vectordbs/dbs/qdrant`. | pass |
| `axon ask "how do i setup self-hosted mem0 docker compose with qdrant and hugginface"` | Answer cites mem0 docs. | Answer cited `docs.mem0.ai` sources including REST API, overview, configuration, Qdrant, Hugging Face, and LlamaIndex pages. | pass |
| `docker ps ...` | Axon service stack healthy. | `axon`, `axon-qdrant`, `axon-tei`, and `axon-chrome` all reported healthy. | pass |

## Risks and Rollback

- Risk: async jobs submitted before this fix can still contain stale loopback endpoints in their persisted `config_json`; cancel/requeue or re-embed those jobs if they are still relevant.
- Risk: the live job status still shows the original `docs.mem0.ai` embed job as canceled, even though the collection has been repaired by a separate host-side embed.
- Rollback: revert `src/jobs/lite/config_snapshot.rs` and `src/jobs/lite/workers/runners.rs`, rebuild/install the binary and container, and re-submit any affected jobs with explicit container-reachable service URLs.
- Data rollback: delete the repaired mem0 points by filtering `domain=docs.mem0.ai` in Qdrant if the repair embed needs to be undone.

## Decisions Not Taken

- Did not commit or push because the worktree already contains many unrelated modified files.
- Did not delete or rewrite the canceled historical embed job row; it remains useful evidence of the failed async job.
- Did not diagnose the reqwest-vs-curl container DNS/client discrepancy fully because host-side repair embedding restored the user-visible behavior.

## References

- `https://docs.mem0.ai/open-source/features/rest-api`
- `https://docs.mem0.ai/open-source/overview`
- `https://docs.mem0.ai/open-source/configuration`
- `https://docs.mem0.ai/components/vectordbs/dbs/qdrant`
- `https://docs.mem0.ai/components/embedders/models/huggingface`

## Open Questions

- Why did `reqwest` calls from the `axon` container to `http://axon-tei/embed` and `http://axon-qdrant:6333/collections/axon` fail while `curl` from the same container succeeded?
- Should status presentation indicate when a crawl's original embed job failed or was canceled but the same source domain has since been repaired by a separate embed?

## Next Steps

Started but not completed:

- Commit/push was not performed.
- No Beads issue was created for the reqwest-vs-curl container networking/client discrepancy.

Follow-on tasks not yet started:

- Add a runtime smoke test that submits an async job from a host CLI and verifies a Docker worker uses container-local service endpoints.
- Consider adding status diagnostics that surface zero-chunk completed embed jobs more prominently.
