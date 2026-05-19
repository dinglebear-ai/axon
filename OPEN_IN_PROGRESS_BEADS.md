# Open and In-Progress Bead Groups

Generated: 2026-05-18 23:29:33 UTC
Repository: `/home/jmagar/workspace/axon_rust`
Source command: `bd list --status open,in_progress --json -n 0`
Grouping: real parent epics where present; otherwise PR review threads and related unparented beads are rolled into synthetic groups.

Summary: 29 open, 3 in progress, 32 total across 12 groups.

## Groups

| Group | Basis | P | Open | In Progress | Total | Updated | Beads |
|---|---|---|---:|---:|---:|---|---|
| ask perf: epic — speed up axon ask without quality loss (`axon_rust-kj9`) | bead: feature | P1 | 0 | 2 | 2 | 2026-05-16T01:27:22Z | `axon_rust-cmm`, `axon_rust-kj9` |
| REST API: replace /v1/actions envelope with dedicated per-resource routes (`axon_rust-2qva`) | bead: epic | P2, P3 | 2 | 1 | 3 | 2026-05-18T20:28:01Z | `axon_rust-2qva`; `axon_rust-iyht`, `axon_rust-2qva.16` |
| PR #105 Review Threads (`PR #105 Review Threads`) | synthetic: PR review | P1, P2 | 7 | 0 | 7 | 2026-05-18T21:50:53Z | `axon_rust-u9pl`, `axon_rust-9dse`, `axon_rust-k1t7`, `axon_rust-hrio`, `axon_rust-cs8a`, `axon_rust-yvzr`, `axon_rust-tb4k` |
| Crawl / Retrieval / Security (`Crawl / Retrieval / Security`) | synthetic: theme | P2 | 2 | 0 | 2 | 2026-05-14T07:41:05Z | `axon_rust-wbm7`, `axon_rust-b4y` |
| Desktop / Palette HTTP API (`Desktop / Palette HTTP API`) | synthetic: theme | P2 | 3 | 0 | 3 | 2026-05-18T02:49:19Z | `axon_rust-gkus`, `axon_rust-pjvj`, `axon_rust-j19t` |
| PR #67 Review Threads (`PR #67 Review Threads`) | synthetic: PR review | P2 | 2 | 0 | 2 | 2026-05-06T12:50:44Z | `axon_rust-1ne`, `axon_rust-x9v` |
| Runtime / Jobs / Watch (`Runtime / Jobs / Watch`) | synthetic: theme | P2, P3 | 2 | 0 | 2 | 2026-05-18T20:31:13Z | `axon_rust-3qrm`, `axon_rust-m7kr` |
| Webclaw Porting (`Webclaw Porting`) | synthetic: theme | P2 | 2 | 0 | 2 | 2026-05-16T00:42:48Z | `axon_rust-puoi`, `axon_rust-zzre` |
| [C1] Score-scale mismatch: BM25 boosts + cosine threshold applied to RRF output (`axon_rust-d71.1`) | bead: epic | P2 | 1 | 0 | 1 | 2026-05-03T05:30:46Z | `axon_rust-d71.1.4` |
| Extract business logic from CLI/MCP into services layer (`axon_rust-dvo`) | bead: epic | P2 | 6 | 0 | 6 | 2026-05-07T03:30:28Z | `axon_rust-dvo.6`, `axon_rust-dvo.5`, `axon_rust-dvo.4`, `axon_rust-dvo.3`, `axon_rust-dvo.2`, `axon_rust-dvo` |
| Wire detect_challenge into crawl/HTTP fetch path with ChallengeDetected escalation (`axon_rust-jej7.1`) | bead: feature | P3 | 1 | 0 | 1 | 2026-05-16T12:20:31Z | `axon_rust-jej7.1.2` |
| Config / Data Home / Memory (`Config / Data Home / Memory`) | synthetic: theme | P4 | 1 | 0 | 1 | 2026-05-16T23:14:55Z | `axon_rust-g4v4` |

## Notes

- This file is group-level. It lists bead IDs for traceability but does not include individual bead titles.
- Generated after applying the 2026-05-18 audit cleanup; closed addressed/stale beads are tracked in `BEAD_AUDIT_2026-05-18.md`.
