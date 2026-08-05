# Pipeline-unification continuation handoff — 2026-07-27

## Objective and checkout

Continue until every task and acceptance criterion in
`docs/superpowers/plans/2026-07-25-pipeline-unification-completion.md` is
implemented, verified, committed, pushed, and honestly recorded in the plan.

Work only in:

```text
/home/jmagar/workspace/axon/.worktrees/codex-pipeline-task2c-chat-stream-facade
branch: codex/pipeline-task2c-chat-stream-facade
```

Do not work on `main`. Do not use `SOLDR_BYPASS=1`; the user explicitly said
not to bypass Soldr. Normal `cargo` invokes the local Soldr front-door.

The user also explicitly expects the *entire* plan to be carried through, not
just the current Task 4 slice. Do not mark the overall goal complete until a
requirement-by-requirement audit proves every plan checkbox and grouped-finding
acceptance item.

## Relevant instructions and workflow

- The user explicitly invoked `superpowers:executing-plans`; read and follow
  `/home/jmagar/.codex/plugins/cache/dendrite-no-mcp/superpowers/6.2.0/skills/executing-plans/SKILL.md`.
- The current Task 4 H4 change was begun test-first; the TDD skill was read.
  Preserve its red/green evidence instead of claiming a test passed before its
  process actually finishes.
- Use `apply_patch` for edits. Preserve existing unrelated dirty changes.
- Use Beads (`bd`) for task tracking. The parent epic is `axon_rust-enbmu`.
- Do not turn runtime deployments into a plan-branch deployment: current plan
  binaries are not yet proven schema compatible with the live store.
- Before expensive validation use the smallest relevant check. Code changes do
  require focused Rust checks/tests; document-only changes do not.

## Current branch and commits

At handoff, `HEAD` is:

```text
2f3bd5614 fix(status): expose terminal job counts
```

Recent useful commits, newest first:

```text
2f3bd5614 fix(status): expose terminal job counts
26d01b68a fix(status): project durable job phase
edaa65ce3 fix(adapters): isolate memory source execution state
2a96cbc46 chore(layering): remove code search refresh exception
6da49d359 refactor(source): route code search through canonical pipeline
14e088fea docs(plan): record scheduler authority progress
a04d918cb fix(jobs): reconcile durable provider leases
ff98f6f2d test(jobs): cover failed reservation release
e50026b9d fix(jobs): release failed provider reservations
```

`2f3bd5614` may be ahead of `origin/codex/pipeline-task2c-chat-stream-facade`;
verify with `git status -sb` before committing/pushing. Do not assume the
current worktree dirt has been authored or staged by the next session.

There is a separate pushed branch:

```text
codex/status-redaction-clean
8fdb42aa1 fix(status): preserve local paths and prune labels
```

It must **not** be merged directly into `main`: live evidence showed
`origin/main` is not an ancestor and the three-dot diff was 148 files / about
7,777 insertions. Later cherry-pick only focused commits after review.

## Current uncommitted worktree state

The worktree is intentionally dirty. At handoff it contains these groups.

### Task 4 H4: injected discovery fetch boundary (new work this session)

Modified files:

```text
crates/axon-adapters/src/web.rs
crates/axon-adapters/src/web/site_discovery.rs
crates/axon-adapters/src/web/site_discovery_tests.rs
crates/axon-adapters/src/web_engine/engine/llms_txt.rs
crates/axon-adapters/src/web_engine/engine/llms_txt_tests.rs
crates/axon-adapters/src/web_engine/engine/map.rs
crates/axon-adapters/src/web_engine/engine/map/strategy.rs
crates/axon-adapters/src/web_engine/engine/sitemap.rs
crates/axon-adapters/src/web_engine/engine/sitemap/backfill.rs
crates/axon-adapters/src/web_engine/engine/sitemap/discover.rs
crates/axon-adapters/src/web_engine/engine/sitemap_tests.rs
```

Purpose: close Bead `axon_rust-bfsp5` / Task 4 H4. `WebSourceAdapter` already
owned `Arc<dyn FetchProvider>`, but Site/Docs/Map discovery bypassed it through
raw `reqwest` clients and hand-rolled retry/backoff.

The in-progress implementation does the following:

1. Adds `map_discovery_uses_the_injected_fetch_provider` in
   `web/site_discovery_tests.rs`. It drives a Map discover through
   `WebSourceAdapter` and asserts the injected fake received a fetch call.
   Against the old direct-client implementation, its call log was necessarily
   empty; this is the intended red behavior. **The process used to execute the
   red test did not return a trustworthy result before the handoff, so rerun it
   and record the real red/green evidence.**
2. Threads `Arc<dyn FetchProvider>` from `WebSourceAdapter` through
   `site_discovery::manifest_items`, `discover_site_urls`, seed resolution,
   sitemap/robots discovery, `llms.txt`, bounded root-anchor discovery, and
   sitemap backfill.
3. Replaces the discovery module's private request loop with
   `sitemap::fetch_text(fetch, url, max_bytes)`. That helper constructs a
   `FetchRequest`, asks the provider, requires 2xx, converts inline text/bytes,
   and enforces the local post-response byte cap. Provider retry, cooldown,
   reservation, redirect, and connect-time policy are intentionally left to
   `FetchProvider`.
4. Replaces `JoinSet`-spawned raw HTTP batches with `futures::future::join_all`
   over cloned `Arc<dyn FetchProvider>` values, retaining bounded batches while
   avoiding the old direct transport path.
5. Updates unit tests that previously asserted private retry or client behavior.
   The oversized-content test now exercises `fetch_text` against
   `FakeAdapterProviders`; the llms cap test uses the same fake and no real
   HTTP request.

Required follow-up before this slice may be committed:

- Run `cargo fmt` and ensure it actually completes. `cargo fmt` was attempted,
  but the normal wrapper returned only toolchain-update lines while the build
  system was saturated; inspect `git diff --check` and rerun once idle.
- Let the focused check finish, repair all compiler errors, then run:

  ```bash
  cargo test -p axon-adapters map_discovery_uses_the_injected_fetch_provider -- --exact --nocapture
  cargo test -p axon-adapters sitemap -- --nocapture
  cargo test -p axon-adapters llms_txt -- --nocapture
  cargo test -p axon-adapters web -- --nocapture
  ```

  Use normal Cargo/Soldr, one build at a time. If a command is wrapped in a
  shell redirection, use a bounded `timeout` and inspect its log; prior calls
  became orphaned through the wrapper and were terminated.
- Search to prove no raw discovery call remains:

  ```bash
  rg -n 'fetch_text_with_retry|build_client\(|http_client\(|fetch_html\(' \
    crates/axon-adapters/src/web_engine/engine/{map,sitemap,llms_txt.rs} \
    crates/axon-adapters/src/web/site_discovery.rs crates/axon-adapters/src/web.rs
  ```

  The intended result is empty for this boundary. Do not overclaim from a
  source scan alone: the behavioral fake-provider test is required.
- Review public API impact of changing re-exported engine signatures. These
  functions were only referenced inside `axon-adapters` according to the last
  `rg`, but confirm with `cargo xtask check-public-api` later in Task 10/12.
- Inspect `FetchRequest::max_bytes`: `resolve_map_seed_url` uses 512 KiB
  because the provider does not offer a headers-only capability. If provider
  semantics make this inappropriate, introduce a typed discovery/HEAD policy
  at the boundary rather than restoring direct raw requests.
- The completion target is not merely adapter discovery. H4’s bead explicitly
  calls out sitemap backfill too, so retain the provider argument through that
  path. If backfill becomes unreachable legacy code, remove it only with a
  test/proof rather than leaving a raw client path.

### Task 4 H5 / canonical classifier cleanup (pre-existing dirty work)

Modified/deleted files:

```text
crates/axon-services/src/source.rs
crates/axon-services/src/source/classify.rs                 (deleted)
crates/axon-services/src/source/classify_tests.rs           (deleted)
crates/axon-services/src/source/dispatch_kind.rs
crates/axon-services/src/source/enqueue.rs
crates/axon-services/src/source/graph.rs
crates/axon-services/src/source/graph_tests.rs
crates/axon-services/src/source/routing.rs
crates/axon-services/src/source/security.rs
crates/axon-services/src/watch.rs
crates/axon-services/src/map.rs
crates/axon-services/src/map_tests.rs
crates/axon-web/src/server/handlers/sources.rs
crates/axon-web/src/server/handlers/sources_tests.rs
```

Intent visible in the diff: delete `SourceInputKind`/the `source/classify`
layer and consume canonical `SourceKind` from the route plan. This is aligned
with Task 4’s “delete both family classifiers and route from canonical source
identity,” but it is **not verified or committed** in this state.

Bead `axon_rust-yygrl` remains in progress. Its required completion is more
than classifier deletion: move registry `pkg_*` → `package_*` / source-family
normalization and session metadata allowlisting into the respective adapters,
then delete shared-pipeline family branches (`sanitize_documents` and session
vector metadata sanitization). Inspect the adapters and existing dirty diff
before deciding whether this work is already included.

### Status/projection changes (pre-existing dirty work)

```text
crates/axon-cli/src/commands/status.rs
crates/axon-cli/src/commands/status_tests.rs
```

`2f3bd5614` contains terminal job count projection. The current unstaged
status changes must be reviewed separately; do not accidentally bundle them
with the H4 adapter-boundary commit.

## Current verification process — stop/restart instructions

At handoff, one normal Soldr check was still compiling a cold target:

```bash
timeout 600s /home/jmagar/.local/bin/cargo check -p axon-adapters --message-format=short \
  > /tmp/axon-adapters-provider-check.log 2>&1
```

It had reached ordinary dependency compilation, with no Axon diagnostic yet:
`serde`, `tokio`, `openssl`, `rustls`, etc. Soldr emitted periodic advisory
lines such as “cargo diagnostic capture still running after 240s”. It is not a
pass, and it is not a confirmed failure.

Before resuming, check whether the process is still live:

```bash
pgrep -af 'cargo check.*axon-adapters|soldr.*axon-adapters'
tail -160 /tmp/axon-adapters-provider-check.log
```

If continuing this handoff immediately, it is reasonable to terminate only
the process group rooted at the above command and start one fresh focused
check after inspecting the terminal. Do **not** kill unrelated Cortex/Labby
builds. Earlier stale Axon verification children from this session were
already explicitly terminated.

## Plan ledger snapshot

The canonical plan is
`docs/superpowers/plans/2026-07-25-pipeline-unification-completion.md`.

- Task 1: marked complete (operational baseline).
- Task 2A–2D: marked complete.
- Task 3: partially complete; still lacks identical route/stage/normalized/
  publication assertions and conversion of known defective-family tests into
  failing characterization tests.
- Task 4: partial. Already marked complete: fail-closed registry validation,
  per-execution state isolation, `SourceRouter::validate_options` removal,
  CodeSearch routing. Still open: shared adapter registry proof, sequential /
  concurrent same-instance leakage tests, both classifier removal, H4 provider
  injection, ETag/Last-Modified ledger trust and `CachePolicy::Revalidate`,
  focused test suite.
- Tasks 5–12: each has meaningful remaining implementation; the plan’s open
  checkboxes are authoritative. Do not mark any completed just because schema
  or partial code exists.

Do not update plan checkboxes for H4 until its behavioral test, focused suite,
and raw-boundary audit pass. When a task genuinely closes, update the dated
plan ledger in the same commit (or a small dedicated docs commit) with exact
commands/results.

## Beads to use next

```text
axon_rust-bfsp5  [P1 OPEN]       H4 FetchProvider discovery bypass
axon_rust-yygrl  [P1 IN_PROGRESS] H5 shared-pipeline per-family rewrite
axon_rust-yow0c  [P1 IN_PROGRESS] T7 real transport output parity
```

Claim `axon_rust-bfsp5` before continuing if it remains open:

```bash
bd update axon_rust-bfsp5 --claim
```

Do not close these beads until their stated engineering-review acceptance is
met and committed evidence exists. Task 10 transport parity must remain open
until actual CLI/REST/MCP requests are compared, not declarations.

## Confirmed runtime/deployment facts from prior work

These are operational context, not permission to deploy the current plan
branch:

- Host CLI at `/home/jmagar/.local/bin/axon` and Incus native binary at
  `/usr/local/bin/axon` both reported 7.2.2 during the prior live validation.
- Incus container is `axon`; Axon runs natively under
  `axon-native.service`, not inside Docker. TEI and Chrome remain the intended
  Docker services in the Incus container. Llama was stopped.
- External service target was `http://198.51.100.4:40090`; `/readyz` showed
  SQLite/Qdrant/TEI ready.
- Artifact content failures were fixed by setting
  `AXON_OUTPUT_DIR=/mnt/axon-data` in the container’s `/mnt/axon-data/.env`,
  aligning artifact writers/readers. Existing artifact content then returned
  HTTP 200.
- Required REST endpoints were previously live-tested with auth: `/healthz`,
  `/readyz`, `/v1/providers`, `POST /v1/sources`, job get/events/artifacts,
  and artifact content.
- Do not use a missing `docker ps` Axon application container as a failure
  signal; service is systemd-native in the Incus container.

If any later task deploys, rebuild host and Bookworm artifacts from **merged
main**, back up/atomically install, restart systemd, and repeat direct plus
production-ingress validation. Never deploy a branch simply because its unit
tests compile.

## Recommended immediate resumption sequence

1. Read this handoff, the plan, and the executing-plans skill.
2. Inspect `git status --short` and `git diff` before editing. Do not discard
   the classifier/status changes.
3. Check or stop the single H4 check process described above; retain its log if
   it finished.
4. Run `git diff --check`, `cargo fmt --check`, then the focused adapter check.
   Fix compile errors one at a time. Likely places: exported function signature
   callers, `futures` dependency/import availability, and test imports left
   after the raw-client removal.
5. Run the exact H4 regression test first. Record the green result only after
   it returns; then run the scoped sitemap/llms/web test groups.
6. Review whether the test is sufficiently behavioral: it must prove
   `WebSourceAdapter::discover` crosses the injected provider, not merely that
   a helper accepts a parameter. Consider a URL-aware fake if a stronger test
   is required to prove sitemap results originate from provider content.
7. Commit H4 independently from classifier/status work, push, update
   `axon_rust-bfsp5`, and update the plan ledger only with evidence.
8. Continue Task 4’s classifier/adapter work, then systematically execute the
   remaining tasks in plan order. The plan has many open scheduler, fencing,
   publication, security, transport, docs, and release requirements; preserve
   the full scope.

## Commands useful for audit and closeout

```bash
git status -sb
git diff --check
git diff -- crates/axon-adapters/src/web_engine
bd show axon_rust-bfsp5 axon_rust-yygrl axon_rust-yow0c
bd list --parent axon_rust-enbmu --status open --limit 0
cargo xtask check-layering
cargo xtask docs generate --check
cargo xtask check-public-api
```

At the eventual end of the whole plan, follow repository completion rules:
create/close beads as appropriate, run required quality gates, `git pull
--rebase`, `bd dolt push`, `git push`, and verify the branch is clean and up to
date with origin. Do not say the work is complete until that state and the
plan’s Task 12 audit are real.
