# train
Last Modified: 2026-06-01

<!-- BEGIN GENERATED ACTION SURFACES -->
## Surfaces

| Surface | Entry point |
|---|---|
| CLI | `axon train ...` |
| REST | Not inventoried |
| MCP | Not exposed as a dedicated MCP action. |
| Service | `Not inventoried` |

Parity notes: This action page is missing from docs/reference/api-parity.md.
<!-- END GENERATED ACTION SURFACES -->


Collect human preference votes over retrieved RAG candidates to build a local training signal.

## Synopsis

```bash
axon train [OPTIONS] [TEXT]...
```

## Arguments

| Argument | Required | Description |
|----------|----------|-------------|
| `[TEXT]...` | Yes | The query or prompt to retrieve candidates for. |

## Flags

| Flag | Default | Description |
|------|---------|-------------|
| `--best <RANK>` | — | Record this 1-based candidate rank as the preferred result without prompting interactively. |
| `--notes <NOTES>` | — | Optional note stored alongside the preference event. |
| `--collection <name>` | `axon` | Qdrant collection name. |
| `--limit <n>` | `10` | Maximum candidates to retrieve (clamped 2–50). |
| `--since <date>` | — | Filter to content indexed on or after this date. |
| `--before <date>` | — | Filter to content indexed on or before this date. |
| `--no-hybrid-search` | `false` | Force dense-only retrieval (disable BM42 sparse + RRF). |
| `--json` | `false` | Machine-readable JSON output. |

## Usage

```bash
# Interactively vote on the best candidate for a query
axon train "how does hybrid search rank results"

# Record a vote non-interactively (3rd candidate is best)
axon train "embedding pipeline" --best 3 --notes "most complete answer"

# JSON mode: list candidates first, then rerun with --best to record
axon train "chunking strategy" --json
```

## Behavior

- Runs the same retrieval as [`ask`](ask.md) with the explain/diagnostics trace enabled, then presents the kept candidates for a preference vote.
- Interactive mode prints the ranked candidates and prompts for a choice; you may skip without recording.
- `--best <rank>` records the vote non-interactively. In `--json` mode without `--best`, the command prints the candidates and a message telling you to rerun with `--best <rank>` to record a vote.
- Recorded votes are appended to `~/.axon/training/preferences.jsonl` (one JSON event per line).
- Errors if retrieval returns no kept candidates to vote on.

## See also

- [`ask`](ask.md) — the underlying RAG retrieval + synthesis.
- [`evaluate`](evaluate.md) — RAG vs baseline with an independent LLM judge.
