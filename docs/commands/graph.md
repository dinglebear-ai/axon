# axon graph
Last Modified: 2026-03-20

Graph command family for Neo4j-backed extraction and exploration.

## Synopsis

```bash
axon graph build [<url> | --url <url>] [--domain <domain>] [--all]
axon graph status
axon graph explore <entity>
axon graph stats
axon graph worker
```

## Notes

- `axon graph` with no subcommand returns a usage error.
- `build` requires one of: URL, `--domain`, or `--all`.
- `worker` requires `AXON_NEO4J_URL` and graph queue connectivity.
- For full details, see [`../GRAPH.md`](../GRAPH.md).
