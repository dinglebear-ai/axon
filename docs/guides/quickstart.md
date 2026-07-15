# Quickstart
Last Modified: 2026-07-15

This is the clean-break quickstart for the unified source pipeline.

## Check The Runtime

Run doctor first:

```text
axon doctor
```

## Index A Source

Use the source itself as the command target:

```text
axon https://example.com/docs --scope site
axon ./my-project --scope directory
```

## Single Page

Use `scrape` for the retained one-page convenience command:

```text
axon scrape https://example.com/page
```

By default, scrape still publishes vectors.

## Ask From Indexed Content

```text
axon ask "what changed in this source?"
```
