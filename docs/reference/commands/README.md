# Command Reference
Last Modified: 2026-06-01

Index of Axon CLI command docs.

## Client/server mode

Set `AXON_SERVER_URL` to make supported stateful commands call a running
`axon serve` process through direct `/v1` REST routes.
Server-mode commands use server-owned jobs, outputs, screenshots, and
artifacts; the host CLI does not write local markdown as the source of truth.
Use `--local` to bypass server mode for one command.

## Core
- [ask](ask.md)
- [crawl](crawl.md)
- [debug](debug.md)
- [dedupe](dedupe.md)
- [doctor](doctor.md)
- [domains](domains.md)
- [embed](embed.md)
- [endpoints](endpoints.md)
- [evaluate](evaluate.md)
- [extract](extract.md)
- [ingest](ingest.md)
- [map](map.md)
- [mcp](mcp.md)
- [migrate](migrate.md)
- [query](query.md)
- [research](research.md)
- [retrieve](retrieve.md)
- [scrape](scrape.md)
- [screenshot](screenshot.md)
- [search](search.md)
- [serve](serve.md)
- [setup](setup.md)
- [sessions](sessions.md)
- [sources](sources.md)
- [stats](stats.md)
- [status](status.md)
- [suggest](suggest.md)
- [summarize](summarize.md)
- [watch](watch.md)

## Setup & Ops

`axon setup`, `preflight`, `compose`, and `smoke` are documented together in [setup](setup.md).

## Ingest Source Redirects
The following are now ingest sub-targets — see [ingest](ingest.md) for the unified command:
- [github](github.md) (auto-detected by `axon ingest`)
- [reddit](reddit.md) (auto-detected by `axon ingest`)
- [youtube](youtube.md) (auto-detected by `axon ingest`)

## Shell
- [completions](completions.md)

## Commands without a dedicated doc yet

These subcommands exist in the binary (run `axon <command> --help`) but do not yet have
their own reference page. See the gap list in the docs-refresh report for what each should cover.

- `brand` — extract a URL's brand identity (colors, fonts, logos, favicon)
- `diff` — diff two URLs and show what changed
- `config` — read/write `~/.axon/.env` and `~/.axon/config.toml` (subcommands: `list`, `get`, `set`, `unset`, `path`)
- `train` — collect human preference votes for retrieved RAG candidates
- `monitor` — stream job-lifecycle events as a line-oriented feed
- `sync` — reconcile locally produced server-mode artifacts
