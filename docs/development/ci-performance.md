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
