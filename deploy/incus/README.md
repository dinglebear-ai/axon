# Axon Incus deployment — nested Docker for qdrant/tei/chrome, native axon

One Incus system container (`axon-container-profile`) runs:

- **qdrant (default mode only)/tei/chrome as nested Docker containers**, via
  a full Docker Engine + Compose internally, using `docker-compose.prod.yaml`
  largely unchanged.
- **axon itself as a native binary managed by systemd** (`axon-native.service`),
  built directly inside the container from the repo's own Dockerfile
  builder/runtime stages, *not* as another nested-Docker service — see
  "Why axon is native, not nested-Docker" below.

See `docs/superpowers/plans/2026-07-07-incus-zabbly-upgrade.md` for the
(retained, harmless) Incus 6.3+/Zabbly upgrade this deployment builds on, and
`axon_rust-4m749` (beads epic) for the full architecture history — including
the same-day pivot to native Incus OCI containers and revert back to nested
Docker for qdrant/tei/chrome, after native OCI's GPU-compute path was found
genuinely broken (`CUDA_ERROR_OUT_OF_MEMORY` during TEI warmup) on this
host/Incus version. See bead `axon_rust-4m749.2`'s comments for the full
diagnostic trail if you're wondering why qdrant/tei/chrome aren't native OCI.

## Why axon is native, not nested-Docker

Confirmed in production 2026-07-08 (bead `axon_rust-4m749.3`): a
containerized axon reaching qdrant/tei/chrome over the nested "jakenet"
bridge works fine, but publishing axon's *own* port back out to the host (for
SWAG/Cloudflare to reach) requires an extra Docker port-proxy NAT hop —
docker-proxy accepted external TCP connections and forwarded bytes to the
container, but the connection was reset mid-relay, specifically for axon
(qdrant/tei/chrome's identical docker-proxy-published ports worked
correctly the entire time, ruling out a general nested-networking bug).
Running axon as a native binary via systemd sidesteps that hop entirely — it
binds directly in the Incus container's own network namespace, and reaches
qdrant/tei/chrome over their already-published `127.0.0.1` ports like any
other client would. `bootstrap.sh` builds the binary via `docker build
--target runtime` (same Dockerfile stage production images use) and installs
`axon-native.service`.

## Profile

`profile.yaml` in this directory is exported directly from the live,
validated Incus profile (`incus profile show axon-container-profile`),
minus the live-only `used_by` field. To apply it fresh:

```bash
incus profile create axon-container-profile
incus profile edit axon-container-profile < deploy/incus/profile.yaml
```

Key config, and why:

- `limits.memory: 24GiB` (hard) — derived as 16GiB qdrant (its own existing
  production cap, justified by its own OOM history) + 2GiB TEI + 2GiB chrome
  + 1GiB axon + 1GiB dockerd overhead, +~14% headroom. Re-verify this still
  matches dookie's actual available headroom before relying on it — it was
  derived once, not continuously monitored.
- `nvidia.runtime: "true"` + `nvidia.driver.capabilities: all` — gives the
  OUTER Incus container GPU/driver visibility (`nvidia-smi` works directly
  in it). The GPU-compute workload (TEI) runs in a NESTED Docker container
  inside this one, and gets its own GPU access via the nested dockerd's own
  `nvidia-container-toolkit` — this is a real, working double-hop, confirmed
  end-to-end with the actual production TEI image (bead `.2`).
- `security.nesting: "true"`, `security.privileged: "false"`,
  `security.syscalls.intercept.{mknod,setxattr}: "true"` — required for the
  nested Docker Engine to function inside an unprivileged Incus container.
- `security.idmap.isolated: "true"` — per-container isolated UID/GID range,
  not a privileged or shared idmap. Container UID 1000 (what axon's Docker
  images run as) maps to a host UID that's stable per-container-instance but
  **not guaranteed identical across a fresh container recreate** — always
  read it back via `incus config get <name> volatile.idmap.current` after
  creating a container, never hardcode the number.
- `gpu` device (bare, no extra options) — whole-GPU passthrough to the outer
  container.

## Storage

`~/.axon` (the real host appdata root used by today's bare-host / local-dev
deployment) is **NOT** shared directly with this Incus container. A real
attempt to `raw.idmap` the container's UID 1000 onto the real host UID 1000
(jmagar) fails: `/etc/subuid` deliberately excludes real system UIDs like
1000 from root's delegation range (`newuidmap: uid range [1000-1001) ->
[1000-1001) not allowed` — intentional Linux security policy, not a bug to
work around).

The resolution: two **separate**, dedicated host directories, chowned to
whatever host UID the container's isolated idmap currently shifts container
UID 1000 to:

| Host path | Container path | Purpose |
|---|---|---|
| `~/.axon-incus` | `/mnt/axon-data` | jobs.db, config.toml, .env, qdrant storage, TEI cache, artifacts, logs, screenshots — everything `docker-compose.prod.yaml` expects under `${AXON_HOME}` |
| `~/.axon-incus-gemini` | `/mnt/axon-gemini` (read-only) | Gemini CLI auth (`oauth_creds.json`, `gemini-credentials.json`, etc.) |

**This is a deliberate, permanent fork from `~/.axon`, not a temporary
workaround.** Host-native CLI commands (`axon doctor`, `axon stats` run
directly on the bare host, outside any Incus instance) will **not** see this
deployment's data, and vice versa. If you're debugging and see stale/empty
results, check which tree you're actually looking at before assuming
something is broken.

Both paths live under `/home/jmagar` (ZFS dataset `rpool/USERDATA/home_hon64g`),
so they're automatically covered by whatever snapshot/replication policy
already covers this host's home dataset — no new backup-side configuration
needed.

### Verified 2026-07-07 (real command output, not description)

- Mount visibility: `incus exec axon-bootstrap-temp -- ls -la /mnt/axon-data
  /mnt/axon-gemini` shows all files/dirs correctly owned by UID 1000 *as seen
  from inside the container* (idmap-shifted; the same files show as UID
  `1066536` from the host side).
- UID 1000 read/write: `docker run --rm --user 1000:1000 -v
  /mnt/axon-data:/data alpine sh -c 'touch /data/test.txt'` succeeds; the
  resulting file lands on the host owned by `1066536:1066536`, not root.
- Adversarial path-traversal test: `docker run --rm --user 1000:1000 -v
  /mnt/axon-data:/data alpine sh -c 'touch /data/../escape.txt'` →
  `Permission denied` (the bind mount is its own boundary; `..` doesn't
  escape it).
- Adversarial outer-container test: attempting to `touch
  /home/jmagar/escape-test.txt` from inside the outer Incus container (as
  root) → `No such file or directory` — the outer container's rootfs simply
  has no visibility into the host filesystem outside its declared disk
  devices at all.

### Known fragility — nvidia-procfs device

A third device, `nvidia-procfs` (bind-mounting
`/proc/driver/nvidia/gpus/<pci-address>` from host to the same path in the
container), is required for nested-Docker GPU passthrough specifically — and
does **not** reliably survive a container stop/start cycle. If nested Docker
GPU access breaks after a restart with `nvidia-container-cli: mount error:
stat failed: /proc/driver/nvidia/gpus/...: no such file or directory`,
remove and re-add this device:

```bash
incus config device remove <container> nvidia-procfs
incus config device add <container> nvidia-procfs disk \
  source=/proc/driver/nvidia/gpus/0000:03:00.0 \
  path=/proc/driver/nvidia/gpus/0000:03:00.0
```

This is a required boot-time step in `bootstrap.sh` (bead `.5`), not a
one-time fix — see that bead for the automated version.

### Required — nvidia-container-toolkit inside the container

The nested Docker Engine needs its own `nvidia-container-toolkit` + CDI
registration to expose the GPU to `docker run --gpus all` — the outer
`nvidia.runtime` profile setting only gives the *outer* container GPU
visibility (`nvidia-smi` works directly in it), not the nested Docker Engine
automatically. Confirmed missing from a from-scratch container build
2026-07-08: `docker run --gpus all ...` failed with `failed to discover GPU
vendor from CDI: no known GPU vendor found` even though the outer
container's own `nvidia-smi` worked fine. A prior working deployment had this
installed by hand outside any script; that state does not survive a
container recreate. `bootstrap.sh` now installs it and regenerates the CDI
spec on every run (the spec embeds nvidia-procfs device paths, which also
don't survive a restart — see above).

### Optional — exposing axon's port to the host

`bootstrap.sh` manages an Incus `proxy` device (`mcp-publish`) that forwards
a host-side address to axon's `127.0.0.1:8001` inside the container, when
`AXON_INCUS_PUBLISH_LISTEN` is set (e.g. `100.88.16.79:40090`, dookie's
Tailscale IP — matching what SWAG's `axon.subdomain.conf` proxies to).
**Scope this to a specific interface address, never `0.0.0.0`** — the point
is to expose axon only on the address the reverse proxy actually reaches,
not every interface on the shared host. This device does not survive a
container recreate; losing it (recreating the container without setting the
env var again) is exactly what took `axon.tootie.tv` down with a 502 after a
2026-07-08 redeploy — SWAG had nothing left to reach.
