---
title: "CI Performance and Timing"
created: 2026-08-08
updated: 2026-08-08
---

# CI performance and timing

Axon routes each changed path through `scripts/ci/changed_paths.py`. Required
branch-protection checks still start for every pull request so GitHub can report
their stable check names, but their expensive jobs must be skipped unless the
classifier marks the corresponding surface as changed.

## Baseline before the 2026-08-08 gating pass

The baseline commit is `642e71a901177c608d1e7059e30e77d9cb859a86`. It changed
only `.github/workflows/release-please.yml` and `tests/workflow_shapes.rs`, but
the old classifier routed the structural test as full CI. That launched all app
builds and every CodeQL language.

| Workflow | Wall time | Runner time | Jobs run/skipped | Longest job |
|---|---:|---:|---:|---|
| CI | 33m 17s | 1h 22m 04s | 17/5 | rust-contracts, 18m 36s |
| CodeQL | 7m 06s | 13m 40s | 7/0 | java-kotlin, 6m 37s |
| Docker image | 53s | 48s | 2/0 | build-and-push, 41s |
| Repository contract | 21s | 15s | 2/0 | contract, 12s |
| auto-tag | 5m 27s | 5m 24s | 1/2 | plan, 5m 24s |
| release-please | 20m 13s | 20m 07s | 2/1 | release-pr-fixups, 17m 31s |

The six workflows consumed about 2h 02m of runner time for that one workflow
contract change.

## Workflow routing inventory

| Workflow | Trigger policy | Expensive-work policy |
|---|---|---|
| `CI` | Required on every PR; path-filtered on `main`; weekly/manual | One trusted classifier gates Rust, web, Android, Palette, Chrome, Compose-adjacent, security, MCP, RAG, and release work independently. |
| `CodeQL` | Required on every PR; language-input paths on `main`; weekly/manual | The matrix contains only languages present in the change. Kotlin keeps a manual build because CodeQL cannot analyze Kotlin in `none` mode. |
| `Compose smoke` | Required on every PR; manual | Classifier and stable gate always report; Compose validation and image build are independent. |
| `Repository contract` | Required on PRs; manual | The merge-to-main duplicate was removed. |
| `Docker image` | Relevant image inputs on `main`, `v*` tags, or manual | Builds once and pushes only for a relevant source change or explicit release invocation. |
| `auto-tag` | Native CLI shipping paths on `main` | Reuses the validated release-plan artifact produced by CI instead of compiling `xtask` again. |
| `release-please` | Successful main CI or manual | A fast component classifier skips release automation for unrelated successful CI runs; fixups build `xtask` once. |
| `release` | Manual/release dispatch | Web assets build once and are shared by Linux and Windows packaging. |
| `palette-release` | Manual/release dispatch | The frontend builds once; Linux and Windows consume the same artifact without rerunning Vite. |
| `android-release` | Manual/release dispatch | Only the Android artifact is built. |
| `chrome-extension-release` | Manual/release dispatch | Only the extension artifact is packaged. |
| `Session-log auto-merge` | PRs touching `docs/sessions/**` | The job still rejects mixed-path PRs before enabling auto-merge. |
| `CI timing report` | Manual | Read-only reporting; concurrent reports cancel the older run. |

GitHub-generated dynamic workflows such as Dependabot and Copilot are excluded
from timing aggregation. The report inventories active repository workflow
files by workflow ID, shows workflows with zero samples, and does not merge
different workflows that happen to share a display name.

## Generate a report

The `CI timing report` workflow is manual and has no effect on normal CI. With
no inputs it reports recent median and p95 durations for every workflow. With
`baseline_sha` and `candidate_sha`, it creates a directly comparable before and
after table. Both Markdown and the underlying per-job JSON are retained as a
workflow artifact for 90 days.

The same report can be generated locally:

```bash
python3 scripts/ci/report_workflow_timings.py \
  --repo dinglebear-ai/axon \
  --sha before=642e71a901177c608d1e7059e30e77d9cb859a86 \
  --sha after=<candidate-sha> \
  --output ci-timing.md \
  --json-output ci-timing.json
```

Wall time is the end-to-end workflow duration. Runner time is the sum of all
non-skipped job durations, so it exposes parallel work and duplicated effort
that wall time alone hides.
