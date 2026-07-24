# Bare-metal systemd deployment

Axon runs as a native binary under a systemd unit. This is the same model the
Incus deployment uses internally (see `deploy/incus/bootstrap.sh`, which writes
an equivalent `axon-native.service` inside the Incus container), lifted onto a
bare host: the same `/usr/local/bin/axon serve` process, just without the
Incus wrapper.

`axon serve` hosts the HTTP API (`/v1/*`), MCP-over-HTTP (`/mcp`), the web
control panel, and the in-process worker runtime in a single process. Detached
jobs (source, extract, watch ticks, retries) advance as long as `axon serve` is
running.

## Prerequisites

- A Linux host with systemd.
- The `axon` binary. Install it via the one-line installer
  (`curl -fsSL https://raw.githubusercontent.com/jmagar/axon/main/install.sh | sh`)
  on the host, or copy a release binary from GitHub Releases.
- A reachable Qdrant, Hugging Face TEI (with `Qwen/Qwen3-Embedding-0.6B`), and
  Chrome/CDP. These typically run as containers (use the infra portions of
  `docker-compose.prod.yaml` for the canonical image versions and ports) or on
  external hosts. Axon reaches them by URL — no local process ownership
  required.
- An NVIDIA GPU + NVIDIA driver on the host that runs TEI (for embedding
  throughput). Axon itself is CPU-only.

## Install

As root:

```bash
# 1. Dedicated service user and data directory.
useradd --system --home /var/lib/axon --shell /usr/sbin/nologin axon
install -d -o axon -g axon /var/lib/axon

# 2. Install the binary.
install -m 0755 axon /usr/local/bin/axon

# 3. Environment file: service URLs, secrets, auth. Copy .env.example as a
#    starting point and fill in QDRANT_URL, TEI_URL, AXON_CHROME_REMOTE_URL,
#    AXON_HTTP_TOKEN, and any adapter credentials you need.
install -d /etc/axon
install -m 0600 your.env /etc/axon/axon.env
chown root:axon /etc/axon/axon.env

# 4. Install the unit and bring it up.
install -m 0644 deploy/systemd/axon.service /etc/systemd/system/axon.service
systemctl daemon-reload
systemctl enable --now axon
```

The unit reads `/etc/axon/axon.env` for all runtime configuration and forces
`AXON_HOME`/`AXON_DATA_DIR` to `/var/lib/axon` so jobs.db, logs, artifacts,
and output all live there. By default `axon serve` binds `127.0.0.1:8001`; to
expose it, put a reverse proxy (nginx, Caddy, Tailscale Funnel, SWAG, etc.) in
front, or set `AXON_HTTP_HOST=0.0.0.0` in `/etc/axon/axon.env` if you have
your own network boundary.

## Operating

```bash
systemctl status axon
systemctl restart axon
journalctl -u axon -f              # live logs (tracing + progress)
journalctl -u axon --since today

# Axon's own self-checks (run as the axon user, or via the HTTP API):
sudo -u axon axon doctor
sudo -u axon axon preflight
sudo -u axon axon status
```

`axon setup init` can populate `/var/lib/axon/config.toml` and `/var/lib/axon/.env`
with sensible defaults if you'd rather generate config than hand-write it; run it
as the `axon` user with `AXON_HOME=/var/lib/axon` set.

## Updating

```bash
install -m 0755 new-axon /usr/local/bin/axon
systemctl restart axon
```

Or use `axon update` (it pulls the latest GitHub Release binary in place).

## Infrastructure (Qdrant / TEI / Chrome)

Axon treats these as external providers located by URL. The canonical image
versions and port mappings live in `docker-compose.prod.yaml`:

| Service | Image | Default port |
|---|---|---|
| Qdrant | `qdrant/qdrant:v1.18.2` | `53333` (HTTP), `53334` (gRPC) |
| TEI | `ghcr.io/huggingface/text-embeddings-inference:89-1.9` | `52000`, GPU |
| Chrome/CDP | built from `config/chrome/Dockerfile` | `6000` (mgmt), `9222` (CDP) |

You can run them as containers on the same host, on another host, or under any
process supervisor — Axon only needs `QDRANT_URL`, `TEI_URL`, and
`AXON_CHROME_REMOTE_URL` to reach them. Point those at wherever your infra lives
in `/etc/axon/axon.env`.

## Relationship to the Incus deployment

`deploy/incus/` is the preferred path: it bundles the whole stack (axon native
under systemd + Qdrant/TEI/Chrome as nested containers) into one Incus system
container with GPU passthrough, profile management, and an idempotent bootstrap.
This bare-metal path is for hosts where you already run the infra yourself, or
where Incus isn't available. Both paths run the identical `axon serve` binary
under systemd — only the wrapper differs.
