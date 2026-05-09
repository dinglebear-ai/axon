# Command Reference
Last Modified: 2026-03-25

Index of Axon CLI command docs.

## Client/server mode

Set `AXON_SERVER_URL` or pass `--server-url` to make supported stateful
commands call a running `axon serve` process through `/v1/actions`.
Server-mode commands use server-owned jobs, outputs, screenshots, and
artifacts; the host CLI does not write local markdown as the source of truth.
Use `--local` to bypass server mode for one command.

## Core
- [ask](./ask.md)
- [crawl](./crawl.md)
- [debug](./debug.md)
- [dedupe](./dedupe.md)
- [doctor](./doctor.md)
- [domains](./domains.md)
- [embed](./embed.md)
- [evaluate](./evaluate.md)
- [extract](./extract.md)
- [ingest](./ingest.md)
- [map](./map.md)
- [mcp](./mcp.md)
- [migrate](./migrate.md)
- [query](./query.md)
- [research](./research.md)
- [retrieve](./retrieve.md)
- [scrape](./scrape.md)
- [screenshot](./screenshot.md)
- [search](./search.md)
- [serve](./serve.md)
- [sessions](./sessions.md)
- [sources](./sources.md)
- [stats](./stats.md)
- [status](./status.md)
- [suggest](./suggest.md)
- [watch](./watch.md)

## Ingest Source Redirects
The following are now ingest sub-targets — see [ingest](./ingest.md) for the unified command:
- [github](./github.md) (auto-detected by `axon ingest`)
- [reddit](./reddit.md) (auto-detected by `axon ingest`)
- [youtube](./youtube.md) (auto-detected by `axon ingest`)

## Shell
- [completions](./completions.md)
