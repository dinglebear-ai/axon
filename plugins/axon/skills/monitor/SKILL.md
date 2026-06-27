---
name: monitor
description: Use Axon watch and monitor jobs to track recurring URL changes and observe async job activity.
---

# Axon Monitor

Axon has two related surfaces:

- `axon watch` creates recurring URL change-detection jobs.
- `axon monitor jobs` streams crawl, extract, embed, and ingest job lifecycle events.

Use this skill when the user wants repeated checks, change history, or job monitoring.

## URL Change Watch

Create a watch with an explicit task payload:

```bash
mkdir -p .axon
cat > .axon/watch-payload.json <<'JSON'
{
  "urls": ["https://example.com/pricing"],
  "ignore_patterns": [],
  "change_threshold_words": 0,
  "summarize": true
}
JSON

axon watch create "pricing-watch" \
  --task-type watch \
  --every-seconds 3600 \
  --task-payload "$(cat .axon/watch-payload.json)"
```

Manage it:

```bash
axon watch list
axon watch run-now <watch-id>
axon watch history <watch-id> --limit 20
```

## Job Event Monitor

```bash
axon monitor jobs --jsonl
axon monitor jobs --watch --jsonl --interval-secs 5
```

## Guidance

- Use `watch` for recurring URL change detection.
- Use `monitor jobs` for queue visibility while async crawl/extract/embed/ingest jobs run.
- Do not document unsupported hosted webhook/email monitor flows unless Axon implements them.
