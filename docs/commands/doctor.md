# axon doctor
Last Modified: 2026-03-03

Run connectivity and pipeline diagnostics for the local Axon stack.

## Synopsis

```bash
axon doctor [FLAGS]
```

## Flags

All global flags apply. Key flags for this command:

| Flag | Default | Description |
|------|---------|-------------|
| `--json` | `false` | Print full structured report JSON. |

## Checks Performed

`doctor` probes and reports:

- Job pipeline readiness for `crawl`, `extract`, `embed`, `ingest`
- Service health for Postgres, Redis, AMQP, Qdrant, TEI, optional Chrome endpoint
- OpenAI-compatible endpoint probe via `GET {OPENAI_BASE_URL}/models`
- Queue names in active config (`crawl`, `extract`, `embed`, `ingest`)
- Browser runtime diagnostics settings
- Stale and pending job counts
- Probe timing metrics

### Lite Mode (`--lite`)

When `--lite` is set, doctor skips all Postgres, Redis, and AMQP probes and checks the SQLite database file instead. The lite report includes:

- SQLite file presence (`exists`) and path
- TEI and Qdrant service probes
- OpenAI endpoint probe
- Chrome endpoint probe
- Browser runtime diagnostics
- `"lite_mode": true` in the report

## Examples

```bash
# Human-readable diagnostic report
axon doctor

# Full JSON report
axon doctor --json
```

## Notes

- `OPENAI_BASE_URL` and `OPENAI_MODEL` affect OpenAI probe and extract LLM-readiness fields.
- Chrome is optional; report includes `configured` and probe status separately.
- `all_ok` focuses on core pipeline + TEI + Qdrant readiness.
