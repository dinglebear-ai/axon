# Pipeline Unification Performance Verification - 2026-08-18

## Scope

This report records the performance and correctness evidence gathered while
finishing the unified source pipeline on the DOOKIE RTX 4070 deployment.

The host was under extreme unrelated CPU, memory, and swap pressure during the
final audit, so historical controlled benchmark results are retained as the
latency evidence. Final live work on 2026-08-18 is used for correctness,
coverage, and TEI capacity validation rather than as a new wall-clock benchmark.
The compact machine-readable evidence summary is kept beside this report in
`pipeline-unification-performance-evidence-2026-08-18.json`.

## Measured progression

The controlled `code.claude.com/docs` corpus benchmark progressed as follows:

| Candidate | Median / wall time | Manifest documents | Chunks |
|---|---:|---:|---:|
| Original baseline | 145.2 s median | 370 | 7,940 |
| Provider-concurrency candidate | 92.1 s | 370 | about 7,960 |
| Markdown-alternate dedupe + provider work | 68.3 s | 187 | 3,875 |

The preserved 370-document provider candidate used the 96/8/320 TEI client
profile. A later 128-input cold smoke on the same corpus took 110.2 s, so 96 is
the better measured client batch size for this hardware/profile. The 187-document
run used 96-input client batches and issued 47 TEI requests.

These measurements are hardware/profile evidence, not universal constants.
Re-run the benchmark when the model, chunking policy, GPU, or service topology
changes.

## Proof that 370 -> 187 does not remove semantic documents

### Historical manifest accounting

The preserved pre-dedupe SQLite manifest contains exactly 370 source items:

- 185 Markdown URLs
- 185 non-Markdown URLs
- all 185 Markdown URLs are distinct

Pairing Markdown URLs with their extensionless semantic route gives:

- 183 exact HTML/Markdown route pairs
- 2 Markdown-only routes:
  - `0https://code.claude.com/docs/en/settings.md`
  - `0https://code.claude.com/docs/en/whats-new/index.md`
- 2 HTML-only routes:
  - `0https://code.claude.com`
  - `0https://code.claude.com/docs/en/whats-new`

Therefore the original 370 manifest entries represent exactly:

`0183 paired semantic routes + 2 Markdown-only + 2 HTML-only = 187 semantic routes`

The deduplicated benchmark URL artifact contains exactly:

- 185 Markdown URLs
- the same 2 HTML-only URLs
- 187 URLs total

So the reduction removes the 183 redundant HTML representations and no semantic
route. The chunk count roughly halves because the pipeline no longer embeds both
representations of those routes.

### Algorithmic contract

The implementation is intentionally narrower than "if `0/foo.md` exists, delete
`0/foo`". An extensionless HTML route is displaced only when the Markdown
alternate was explicitly advertised by `0llms.txt` while that provenance is still
known in map discovery.

Independently discovered `0/guide` and `0/guide.md` routes are preserved as
separate documents. Root-anchor discovery is allowed to fill missing routes, but
cannot reintroduce an HTML representation that an explicitly advertised
Markdown alternate already replaced. The advertised counterpart and candidate
are compared using the same canonical URL key, including trailing-slash and
query handling.

Regression tests assert:

- advertised Markdown replacement preserves the complete semantic route set
- HTML-only and Markdown-only routes survive
- unadvertised HTML/Markdown siblings both survive
- root anchors cannot reintroduce a displaced advertised HTML representation
- trailing-slash and query variants use the canonical dedupe key

### Fresh live audit

A fresh 2026-08-18 audit of the current Claude documentation independently
fetched:

- 187 Markdown URLs advertised by `0https://code.claude.com/llms.txt`
- all 187 extensionless HTML counterparts
- 187/187 successful Markdown fetches
- 187/187 successful HTML-counterpart fetches
- zero pair fetch errors

The current sitemap explicitly lists 186 of those HTML counterparts. The one
remaining extensionless counterpart is live and returned HTTP 200, but is not
listed in the sitemap.

A representation-content sanity check found 4,066 of 4,077 Markdown headings in
the visible HTML text (99.73%). This comparison is diagnostic only. HTML and
Markdown are different representations, so the correctness guarantee is route
coverage, not byte-for-byte identity.

## TEI 196608 live deployment validation

### Deployment change

The live `0axon-tei` environment override was changed from:

`0TEI_MAX_BATCH_TOKENS=163840`

to:

`0TEI_MAX_BATCH_TOKENS=196608`

Only the TEI service was recreated. Its running command was verified to contain
`0--max-batch-tokens 196608`, and the service returned healthy with zero
restarts.

Relevant server/client limits for this profile:

- TEI max concurrent requests: 512
- TEI max batch requests: 512
- TEI max batch tokens: 196608
- TEI max client batch size: server accepts 128; Axon uses 96
- Axon TEI max concurrent requests: 8
- Axon TEI max in-flight inputs: 320

### Representative 320-input validation

Six repeated waves used the exact Axon weighted-input ceiling:

`096 + 96 + 96 + 32 = 320 simultaneous inputs`

Results:

- 24/24 HTTP 200
- zero TEI restarts
- zero OOM or admission errors
- peak observed VRAM: 3,086 MiB / 12,282 MiB
- peak observed GPU utilization: 100%

### Long-chunk 320-input validation

A separate exact-320 wave used deliberately much longer inputs to exercise the
high-token edge of the new server budget:

- 4/4 HTTP 200
- zero TEI restarts
- zero OOM or admission errors
- peak observed VRAM: 11,086 MiB / 12,282 MiB
- peak observed GPU utilization: 100%

This is the important safety boundary: 196608 is viable on the current RTX 4070,
but a long-chunk full Axon envelope can use most of the card. The 320-input Axon
gate must remain part of this profile.

### Deliberately out-of-envelope validation

An 8 x 96 long-input wave intentionally exceeded Axon's 320-input gate. TEI
accepted the initial work and rejected excess admission with HTTP 429 and the
server-side "no permits available" condition. It did not CUDA OOM.

This verifies two layers of protection:

1. Axon normally prevents a single embedding operation from presenting this
   request shape by enforcing the 320-input weighted gate.
2. TEI has its own admission backpressure before unbounded request fanout.

The out-of-envelope run is not a supported operating target. Its purpose was to
prove graceful pressure behavior.

## Current verification status

- `0axon-adapters`: 720 passed, 0 failed, 2 ignored
- `0axon-graph`: 80 passed, 0 failed
- `0axon-ledger`: 75 passed, 0 failed
- `0git diff --check`: passed
- embedding/vector/jobs and services verification: run as part of the final
  branch verification after this report is written

## Benchmark caveat

Do not compare a new wall-clock run captured during the 2026-08-18 final audit
directly to the controlled baseline. DOOKIE was simultaneously running many
unrelated Rust/BEAM/Cortex builds and had load averages above 100 with swap
fully occupied. A clean 3-cold/3-warm benchmark should be repeated when the host
is quiescent before assigning a new latency number to the final branch.

## Follow-up performance candidates

The following are measurement candidates, not correctness defects:

- overlap one Qdrant write with the next TEI embed batch if measured pipeline
  ordering and memory costs remain acceptable
- overlap fetching of the next acquisition batch with downstream processing
- evaluate Git object IDs as a fingerprint shortcut for clean repository trees
- profile serial cleanup-debt draining on warm generations
- reduce remaining O(N) manifest metadata ownership if very large manifests
  make it material

None of these should be adopted without a controlled before/after benchmark.
