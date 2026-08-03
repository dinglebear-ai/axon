# axon source
Last Modified: 2026-08-02

<!-- BEGIN GENERATED ACTION SURFACES -->
## Surfaces

| Surface | Entry point |
|---|---|
| CLI | `axon source ...` |
| REST | `POST /v1/sources` |
| MCP | `{ "action": "source" }` |
| Service | `Shared domain/service implementation` |
<!-- END GENERATED ACTION SURFACES -->


Index a source through the unified pipeline

## Commands

| Command | Summary |
|---|---|
| `axon source` | Index a source through the unified pipeline |

## Help

```bash
axon source --help
```

The generated [CLI registry](../cli/commands.md) is authoritative for the full command inventory.

## Examples

```bash
axon source https://example.com --scope page --wait true
axon source https://docs.example.com --scope site --wait true
axon source /home/user/project --wait true
axon source https://github.com/owner/repo --scope repo --wait true
axon source r/rust --scope subreddit --wait true
axon source https://www.reddit.com/r/rust/comments/POST_ID/TITLE --scope thread --wait true
axon source https://www.youtube.com/watch?v=VIDEO_ID --scope video --wait true
axon source https://www.youtube.com/playlist?list=PLAYLIST_ID --scope playlist --wait true
```

See [Source Pipeline](../../architecture/source-pipeline.md) and [Adapter Scopes](../sources/adapter-scopes.md).
