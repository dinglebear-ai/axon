# Ask Backend Quality Parity - 2026-05-07

This report records the first parity substrate for `axon_rust-6dl`. It uses
`docs/eval/ask-golden.jsonl` as the approved golden set and consumes structured
`axon evaluate --json` score fields.

Judge isolation:

- Judge backend: disclosed per JSONL row by `scripts/evaluate-ask-golden.sh`
- Current `axon evaluate` runs answer generation and judging under the same
  backend/agent cell. The JSONL rows disclose that actual cell backend/agent;
  this report is therefore a structured parity substrate, not a final
  pinned-judge parity claim.
- Judge model: recorded from `JUDGE_MODEL` or `$OPENAI_MODEL`.

Status meanings:

| Status | Meaning |
|--------|---------|
| `parsed` | All four score axes parsed from judge output. |
| `partial` | At least one score axis parsed but at least one expected axis missing. |
| `parse_failed` | Judge output did not match the structured score contract. |
| `unavailable_or_failed` | The backend cell did not complete, commonly because a CLI is unavailable, unsafe, or unauthenticated. |

Current implementation status:

| Agent | ACP | Headless |
|-------|-----|----------|
| Claude | Supported through existing ACP path. | Implemented with no-tool stream-json command builder and parser. |
| Codex | Supported through existing ACP path when configured. | Unavailable until a no-tool synthesis posture is proven. |
| Gemini | Supported through existing ACP path when configured. | Unavailable until a no-tool synthesis posture is proven. |

Threshold:

Headless must be within 2 percentage points of same-agent ACP on accuracy,
relevance, completeness, and specificity. Cells marked unavailable are not zero
scores and require follow-up before parity can be claimed.

Run command:

```bash
scripts/evaluate-ask-golden.sh
```

The script writes `docs/perf/quality-parity-YYYY-MM-DD.jsonl`, rejects duplicate
golden IDs and blank questions, discloses the judge backend/agent used for each
cell, and preserves unavailable/auth/parse statuses as explicit rows.
