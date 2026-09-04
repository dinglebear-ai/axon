# Apple MLX TEI compatibility server

This server preserves the deployed single-dispatcher MLX pipeline while adding
aggregate performance telemetry and strict non-truncating request limits.

`/metrics` reports one epoch-wide accelerator interval union. Concurrent
requests never double-count overlapping Metal work: `request_wall_us` is the
first-request-start to last-request-completion window, `metal_busy_us` is the
union of every dispatch interval in that window, and `dispatcher_idle_us` is
the remainder. Benchmark consumers require a fresh epoch with zero prior
requests so snapshot subtraction remains an exclusive measurement.

Launch locally:

```bash
MLX_TEI_TUNED_PYTHON_PATH=/Users/jmagar/.local/opt/embedding-bench/tuned-python \
  /Users/jmagar/.local/opt/embedding-bench/mlx-serve-venv/bin/python \
  scripts/apple-mlx/mlx_tei_direct.py --host 127.0.0.1 --port 8084
```

The LaunchAgent must explicitly pass `--host 127.0.0.1`. A Tailscale or other
non-loopback bind is refused unless `MLX_TEI_AUTH_TOKEN` is set; all `/embed`,
`/info`, and `/metrics` clients must then send `Authorization: Bearer …`.
Tokens, source content, URLs, paths, token IDs, and document identifiers are
never included in metrics or access logs.

The tokenizer has padding and truncation disabled, and startup retains the
700-token probe. Requests above declared limits fail rather than truncate.
