# Axon Ask Model Comparison Run

Created: 2026-06-07T19:05:29-04:00

- Questions: /home/jmagar/workspace/axon/reports/llm-ask-comparison-2026-06-07/questions-indexed-general.md
- Axon: /home/jmagar/workspace/axon/target/release/axon
- Models: current,gemini-flash,gemma-local
- Run JSON: run.json

## Status

This was an intermediate run with one failed profile. `cli-api-gemini-3.5-flash-low` failed all 10 answer runs with exit code 1; `current-config` and `llamacpp-gemma-4-e4b-q4` completed 10/10 each. No separate explain-failure fields are present in this run schema.

## Share Warning

Do not publish `run.json` without review/redaction. It can include internal URLs, OAuth client IDs, email addresses, local service endpoints, and redacted-but-sensitive environment metadata.

Temporary env files are generated outside this directory and removed on exit.
