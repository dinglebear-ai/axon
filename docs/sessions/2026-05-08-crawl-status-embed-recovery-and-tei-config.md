# 2026-05-08 Crawl Status, Embed Recovery, and TEI Config

## Context

Working directory: `/home/jmagar/workspace/axon_rust`

Branch: `main`

This session followed live debugging of `axon crawl`, `axon status`, embed recovery, plugin-cache binary drift, and TEI throughput.

## Terminal Output Cleanup

The crawl command output was redesigned to stop dumping every option by default. The direction was to show a compact, colored terminal summary focused on:

- target URL
- scope, depth, and page cap
- render mode and browser status
- cache and embedding state
- job ID and next status command

The goal is operator-readable output first, with full config available through debug/json paths rather than normal command output.

## Build and Install Drift

Findings:

- The live MCP service was running `axon` from the Claude plugin cache:

```text
/home/jmagar/.claude/plugins/cache/jmagar-lab/axon/575c090bcaf5/bin/axon serve mcp
```

- This created repeated drift between freshly built repo binaries and the binary actually serving MCP requests.

Actions:

- Updated the `Justfile` install recipes so debug/release installs also refresh the plugin-cache binary path.
- The install flow restarts `axon-mcp.service` after replacing the binary so service runtime matches the latest built artifact.

## Sccache Stability

The recurring warning was:

```text
sccache: warning: The server looks like it shut down unexpectedly, compiling locally instead
```

Findings:

- `sccache` needed to be treated as a real long-running daemon rather than an incidental process.
- A user service was created/enabled for stable `sccache` startup.

Current observed process during this session:

```text
/usr/bin/sccache
```

## Crawl Depth Default

The default crawl depth was changed from `5` to `10`.

Files touched for the default/documentation/golden update include:

- `README.md`
- `docs/MCP-TOOL-SCHEMA.md`
- `docs/commands/crawl.md`
- `docs/mcp/TOOLS.md`
- `src/core/config/cli/global_args.rs`
- `src/core/config/types.rs`
- `src/core/config/types/config_impls.rs`
- `src/core/config/types/subconfigs.rs`
- `tests/fixtures/export_schema_v3.golden.json`

Live job evidence later showed new crawl jobs using depth `10`.

## Stranded Embed Recovery

Symptom:

- `axon status` showed embed jobs as pending/running after an unexpected worker shutdown.
- The crawls had completed and markdown existed on disk, but the embed jobs looked stranded.

Findings:

- Completed crawls had enqueued embed work, then workers exited before finishing.
- Status needed to make that recovery state explicit rather than making it look like the crawl had not happened.

Actions:

- Reclaimed stale running jobs.
- Changed stale recovery text from `reclaimed after unexpected shutdown` to clearer wording:

```text
recovered after worker shutdown; processing resumed
```

- Cleared stale `error_text` when recovered jobs are claimed/completed.

Verification:

- The previously stranded `code.claude.com` embed jobs completed:

```text
65bc4875-5e3f-4d27-8278-22d450c3ed80 completed 1312 docs 29620 chunks
4f291fb3-0406-4d46-93c9-0ca633e6e32c completed 1313 docs 29632 chunks
eee1257b-a471-43fc-a276-e02347e3c60e completed 1313 docs 29632 chunks
e7286ed3-83a9-4f5b-80f1-e72d8139889b completed 1313 docs 29632 chunks
```

## Embed Progress Display

Status output now includes document progress and percentage where possible.

Example observed output during recovery:

```text
1112/1312 docs · 84.8% · 26283 chunks
1109/1313 docs · 84.5% · 26169 chunks
1119/1313 docs · 85.2% · 26433 chunks
1124/1313 docs · 85.6% · 26440 chunks
```

Implementation areas:

- `src/jobs/lite/workers/progress.rs`
- `src/cli/commands/status.rs`
- `src/jobs/lite.rs`
- `src/jobs/lite/ops/lifecycle.rs`
- `src/jobs/lite/ops/tests.rs`

## Default Exclude Path Bug

Question investigated:

```text
it embedded 1313 docs? can you investigate why the default excludepaths werent used?
```

Findings:

- The queued crawl configs did include the default `exclude_path_prefix` list.
- Each relevant crawl config contained `93` default exclude prefixes.
- The output directory still included localized files such as:

```text
docs-es-legal-and-compliance
docs-fr-permission-modes
docs-de-agent-sdk-typescript
docs-ja-settings
docs-zh-cn-agent-sdk-claude-code-features
```

Root cause:

- Excludes like `/fr`, `/de`, and `/ja` only matched root-relative paths:

```text
https://code.claude.com/fr/...
```

- They did not match localized docs nested below the first path segment:

```text
https://code.claude.com/docs/fr/...
```

Actions:

- Updated `src/crawl/engine/url_utils.rs` so exclude matching checks both root paths and first-segment-relative paths.
- Updated Spider blacklist generation to emit both root and `/<first-segment>/<excluded-prefix>` patterns.
- Updated `src/core/content.rs` so embed directory preparation applies the same nested-path exclude logic to already-saved markdown.
- Added tests proving `/docs/fr` and `/docs/ja-jp` are excluded while `/docs/javascript` is not.

Verification:

```bash
cargo fmt --check
cargo test --lib exclude_path_prefix --locked
cargo test --lib excludes_first_segment_relative_locale_paths --locked
cargo test --lib build_exclude_blacklist_patterns --locked
cargo check --bin axon --locked
```

## TEI Embedding Config

Question investigated:

```text
check our embedding config like how many were batching etc
```

Effective TEI server config from `/info`:

```json
{
  "model_id": "Qwen/Qwen3-Embedding-0.6B",
  "model_dtype": "float16",
  "max_concurrent_requests": 512,
  "max_input_length": 32768,
  "max_batch_tokens": 163840,
  "max_batch_requests": 128,
  "max_client_batch_size": 96,
  "tokenization_workers": 8
}
```

Axon-side embed behavior:

- `AXON_EMBED_DOC_CONCURRENCY` was unset, so default is `min(CPUs, 8)`; this host reports `23` CPUs, so effective concurrency is `8`.
- Qdrant point flush buffer defaults to `256`.
- TEI request timeout defaults to `30000ms`.
- TEI retries default to `5` after the initial attempt.
- Per-document embed timeout defaults to `300s`.

Bug found:

- `.env` had `TEI_MAX_CLIENT_BATCH_SIZE` twice:

```text
TEI_MAX_CLIENT_BATCH_SIZE=128
TEI_MAX_CLIENT_BATCH_SIZE=96
```

- TEI was capped at `96`, while Axon was sending some client batches as large as `128`.
- TEI logs showed repeated rejected requests:

```text
batch size 128 > maximum allowed batch size 96
batch size 117 > maximum allowed batch size 96
batch size 113 > maximum allowed batch size 96
batch size 105 > maximum allowed batch size 96
batch size 104 > maximum allowed batch size 96
```

Action:

- Removed the duplicate `TEI_MAX_CLIENT_BATCH_SIZE=128` from `.env`.
- Left a single `TEI_MAX_CLIENT_BATCH_SIZE=96`.
- Restarted `axon-mcp.service` after confirming no embed jobs were running.

Verification:

```text
.env: TEI_MAX_CLIENT_BATCH_SIZE=96
doctor: max_client_batch_size=96
axon-mcp.service: active (running)
```

## Current Dirty Worktree

Tracked files modified at session-save time:

```text
Justfile
README.md
docs/MCP-TOOL-SCHEMA.md
docs/commands/crawl.md
docs/mcp/TOOLS.md
src/cli/commands/status.rs
src/core/CLAUDE.md
src/core/config/cli/global_args.rs
src/core/config/types.rs
src/core/config/types/config_impls.rs
src/core/config/types/subconfigs.rs
src/core/content.rs
src/crawl/CLAUDE.md
src/crawl/engine/tests.rs
src/crawl/engine/url_utils.rs
src/jobs/lite.rs
src/jobs/lite/ops/lifecycle.rs
src/jobs/lite/ops/tests.rs
src/jobs/lite/workers/progress.rs
src/lib.rs
tests/fixtures/export_schema_v3.golden.json
```

Machine-local untracked/ignored config changed:

```text
.env
```

The `.env` change is intentionally local and should not be committed unless project policy changes.

## Open Questions

- The debug install/restart flow should be used consistently so the plugin-cache binary does not drift from the repo build.
- Consider adding explicit runtime logging for resolved embed config values at `embed_pipeline` startup: doc concurrency, TEI client batch size, TEI timeout, retries, Qdrant flush buffer, and TEI `/info` max client batch size.
- Consider making Axon compare client batch size to TEI `/info.max_client_batch_size` and warn or clamp automatically when the server reports a lower cap.
