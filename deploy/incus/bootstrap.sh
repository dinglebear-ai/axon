#!/usr/bin/env bash
# Idempotent bootstrap for axon's Incus deployment.
#
# Split model (confirmed in production 2026-07-08, bead axon_rust-4m749.3):
#   - qdrant (default mode only)/tei/chrome run as NESTED DOCKER containers
#     inside the Incus container — they need the GPU-passthrough Docker
#     Engine setup this script builds.
#   - axon itself runs as a NATIVE BINARY via systemd directly inside the
#     Incus container, NOT as a nested-Docker service. A containerized axon
#     talking to nested-Docker qdrant/tei/chrome over the "jakenet" bridge
#     needs no NAT hairpin of its own, but publishing axon's port back out to
#     the host (for SWAG/Cloudflare) requires an extra NAT hop through
#     Docker's own port-proxy for every single request; that hop was found to
#     silently reset connections in this environment (TCP handshake
#     completes, backend byte relay resets) while every OTHER nested service
#     on the same bridge/docker-proxy mechanism worked fine — i.e. axon
#     specifically, not nested Docker networking in general. Native + systemd
#     sidesteps that hop entirely: axon binds directly to the Incus
#     container's own network namespace, and only reads from
#     qdrant/tei/chrome over their already-published localhost ports.
#
# Usage:
#   deploy/incus/bootstrap.sh [default|external-qdrant]
#
# default mode:          bundled qdrant starts alongside tei/chrome.
# external-qdrant mode:  requires AXON_EXTERNAL_QDRANT_URL; bundled qdrant is
#                        not started, axon points at the external instance.
#
# Optional: set AXON_INCUS_PUBLISH_LISTEN (e.g. "100.88.16.79:40090") to have
# this script manage the Incus `proxy` device that exposes axon's HTTP port
# to the host for SWAG/Cloudflare. Scope this to a specific interface
# address, never 0.0.0.0 — the whole point is to expose axon only on the
# Tailscale-reachable address SWAG proxies to, not every interface on the
# shared host. If unset, no proxy device is created/updated here; manage
# host-side exposure yourself.
#
# Safe to re-run at any time (idempotent) — including to redeploy a new axon
# binary build after a code change. Also the intended entry point for the
# companion systemd unit (axon-incus-bootstrap.service) that re-runs this on
# every host boot, since boot.autostart=true alone does not re-apply the
# known-fragile nvidia-procfs device, re-verify GPU access, or restart the
# native axon service.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CONTAINER_NAME="${AXON_INCUS_CONTAINER:-axon}"
PROFILE_NAME="axon-container-profile"
GPU_PCI_ADDRESS="${AXON_GPU_PCI_ADDRESS:-0000:03:00.0}"
DEPLOY_PATH="/opt/axon-deploy"
MODE="${1:-default}"

log() { echo "[bootstrap] $*"; }
fatal() { echo "[bootstrap] FATAL: $*" >&2; exit 1; }

case "$MODE" in
  default|external-qdrant) ;;
  *) fatal "unknown mode '$MODE' (expected 'default' or 'external-qdrant')" ;;
esac
if [ "$MODE" = "external-qdrant" ] && [ -z "${AXON_EXTERNAL_QDRANT_URL:-}" ]; then
  fatal "AXON_EXTERNAL_QDRANT_URL must be set for external-qdrant mode"
fi

### 1. Profile: create from the committed definition if missing. Never
### overwrite an existing profile — live edits during development are
### intentional and this script must not clobber them.
if ! incus profile show "$PROFILE_NAME" >/dev/null 2>&1; then
  log "creating profile $PROFILE_NAME from deploy/incus/profile.yaml"
  incus profile create "$PROFILE_NAME"
  incus profile edit "$PROFILE_NAME" < "$REPO_ROOT/deploy/incus/profile.yaml"
else
  log "profile $PROFILE_NAME already exists, leaving as-is"
fi

### 2. Container: create if missing.
if ! incus info "$CONTAINER_NAME" >/dev/null 2>&1; then
  log "creating container $CONTAINER_NAME"
  incus launch images:debian/bookworm "$CONTAINER_NAME" -p default -p "$PROFILE_NAME"
else
  log "container $CONTAINER_NAME already exists"
fi

### 3. Ensure running.
state="$(incus list "$CONTAINER_NAME" --format csv -c s 2>/dev/null || echo "")"
if [ "$state" != "RUNNING" ]; then
  log "starting container (was: ${state:-absent})"
  incus start "$CONTAINER_NAME"
fi

### 4. Wait for exec-readiness — bounded (30 attempts * 2s = 60s max).
ready=0
for _ in $(seq 1 30); do
  if incus exec "$CONTAINER_NAME" -- true >/dev/null 2>&1; then
    ready=1
    break
  fi
  sleep 2
done
[ "$ready" = "1" ] || fatal "container did not become exec-ready within 60s"

### 5. Install Docker inside the container if missing (idempotent). Needed
### for the nested qdrant/tei/chrome services (and to build axon's own
### binary via the repo's Dockerfile builder stage — see step 15).
if ! incus exec "$CONTAINER_NAME" -- sh -c 'command -v docker' >/dev/null 2>&1; then
  log "Docker not found inside container, installing..."
  incus exec "$CONTAINER_NAME" -- sh -c '
    set -e
    apt-get update
    apt-get install -y ca-certificates curl gnupg
    install -m 0755 -d /etc/apt/keyrings
    curl -fsSL https://download.docker.com/linux/debian/gpg -o /etc/apt/keyrings/docker.asc
    chmod a+r /etc/apt/keyrings/docker.asc
    codename="$(. /etc/os-release && echo "$VERSION_CODENAME")"
    arch="$(dpkg --print-architecture)"
    echo "deb [arch=${arch} signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/debian ${codename} stable" \
      > /etc/apt/sources.list.d/docker.list
    apt-get update
    apt-get install -y docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin
  '
else
  log "Docker already present inside container"
fi

### 6. Install nvidia-container-toolkit inside the container (idempotent).
### REQUIRED for the nested Docker Engine to expose the GPU to containers via
### `docker run --gpus all` — confirmed missing from a from-scratch container
### build 2026-07-08: without it, `docker run --gpus all ...` fails with
### "failed to discover GPU vendor from CDI: no known GPU vendor found" even
### though the outer Incus container's own `nvidia-smi` works fine (the
### `nvidia.runtime` profile setting gives the OUTER container GPU access;
### the NESTED Docker Engine needs its own toolkit + CDI registration on top
### of that). A prior working deployment had this set up manually, outside
### this script — that state does not survive a container recreate, hence
### this step.
if ! incus exec "$CONTAINER_NAME" -- sh -c 'command -v nvidia-ctk' >/dev/null 2>&1; then
  log "nvidia-container-toolkit not found inside container, installing..."
  incus exec "$CONTAINER_NAME" -- sh -c '
    set -e
    curl -fsSL https://nvidia.github.io/libnvidia-container/gpgkey \
      | gpg --dearmor -o /usr/share/keyrings/nvidia-container-toolkit-keyring.gpg
    curl -s -L https://nvidia.github.io/libnvidia-container/stable/deb/nvidia-container-toolkit.list \
      | sed "s#deb https://#deb [signed-by=/usr/share/keyrings/nvidia-container-toolkit-keyring.gpg] https://#g" \
      > /etc/apt/sources.list.d/nvidia-container-toolkit.list
    apt-get update -qq
    apt-get install -y -qq nvidia-container-toolkit
    nvidia-ctk runtime configure --runtime=docker
    systemctl restart docker
  '
else
  log "nvidia-container-toolkit already present inside container"
fi
# The CDI spec must be (re)generated on every run, not just at install time —
# it embeds the current nvidia-procfs GPU device paths, which step 7 below
# re-applies on every run because they do not reliably survive a container
# stop/start cycle. A stale CDI spec from a previous boot would silently
# reference now-invalid device paths.
log "regenerating NVIDIA CDI spec"
incus exec "$CONTAINER_NAME" -- nvidia-ctk cdi generate --output=/etc/cdi/nvidia.yaml >/dev/null 2>&1 || true

### 7. Re-apply the nvidia-procfs device. REQUIRED ON EVERY RUN — confirmed
### (bead axon_rust-4m749.2) this does not reliably survive a container
### stop/start cycle, silently breaking nested-Docker GPU passthrough until
### removed and re-added.
log "re-applying nvidia-procfs device (known fragility across restarts)"
incus config device remove "$CONTAINER_NAME" nvidia-procfs >/dev/null 2>&1 || true
incus config device add "$CONTAINER_NAME" nvidia-procfs disk \
  "source=/proc/driver/nvidia/gpus/${GPU_PCI_ADDRESS}" \
  "path=/proc/driver/nvidia/gpus/${GPU_PCI_ADDRESS}" >/dev/null

### 8. Fail-closed GPU verification BEFORE anything GPU-dependent starts.
### Do not start TEI in a broken/CPU-fallback state — refuse and exit instead.
### Pull the verification image once, outside the retry loop — otherwise a
### cold image cache turns each retry into a registry pull-and-run, which
### conflates "GPU/nvidia-procfs isn't ready yet" with "registry is slow,"
### and 3s between retries isn't enough time for a fresh pull anyway.
GPU_VERIFY_IMAGE="nvidia/cuda:12.6.0-base-ubuntu24.04"
if ! incus exec "$CONTAINER_NAME" -- docker pull "$GPU_VERIFY_IMAGE" >/dev/null 2>&1; then
  log "warn: could not pre-pull ${GPU_VERIFY_IMAGE} (registry/network issue?) — if GPU verification below fails, check network/registry access first, not just the GPU/driver"
fi
log "verifying nested-Docker GPU access (fail-closed if this doesn't work)"
gpu_ok=0
for _ in 1 2 3; do
  if incus exec "$CONTAINER_NAME" -- docker run --rm --gpus all \
       "$GPU_VERIFY_IMAGE" nvidia-smi >/dev/null 2>&1; then
    gpu_ok=1
    break
  fi
  sleep 3
done
[ "$gpu_ok" = "1" ] || fatal "GPU verification failed after 3 attempts — refusing to start TEI in a broken state. Check the nvidia-procfs device and host NVIDIA driver."

### Resolve the env file now — needed by the network-name lookup (10), the
### axon-native systemd unit (16), and the actual push (13).
env_file_on_host="${AXON_ENV_FILE:-$HOME/.axon/.env}"
[ -f "$env_file_on_host" ] || fatal "env file not found at $env_file_on_host"

### 9. Sync deploy artifacts (compose files + config/) into the container.
### Only used for the nested qdrant/tei/chrome services now — axon itself is
### built from the full repo tree separately in step 15.
log "syncing compose files + config into ${DEPLOY_PATH}"
incus exec "$CONTAINER_NAME" -- mkdir -p "$DEPLOY_PATH"
incus file push "$REPO_ROOT/docker-compose.prod.yaml" "$CONTAINER_NAME$DEPLOY_PATH/docker-compose.prod.yaml"
incus file push "$REPO_ROOT/docker-compose.external-qdrant.yaml" "$CONTAINER_NAME$DEPLOY_PATH/docker-compose.external-qdrant.yaml"
incus file push -r "$REPO_ROOT/config" "$CONTAINER_NAME$DEPLOY_PATH/"

### 10. Ensure the external Docker network exists (idempotent — compose
### requires this to pre-exist since docker-compose.prod.yaml declares it
### `external: true`). The real network name comes from DOCKER_NETWORK in the
### env file (defaults to "axon") — must match compose's own resolution
### exactly, not be hardcoded, since a deployment can override it (e.g.
### dookie's existing ~/.axon/.env sets DOCKER_NETWORK=jakenet). Reuse the
### repo's own env-file parser (handles quoting/comments/export prefix
### correctly) instead of a one-off grep, in a subshell so it doesn't leak
### the whole env file into this script's environment.
# shellcheck source=/dev/null
docker_network_load_err="$(mktemp)"
docker_network_name="$(
  source "$REPO_ROOT/scripts/lib/axon-env.sh"
  AXON_ENV_FILE="$env_file_on_host" load_axon_env_file "$REPO_ROOT" 2>"$docker_network_load_err"
  printf '%s' "${DOCKER_NETWORK:-axon}"
)"
# This deployment specifically relies on a non-default DOCKER_NETWORK value
# (dookie's ~/.axon/.env sets jakenet) — a silently-swallowed parse failure
# here would fall through to the "axon" default and misroute the whole stack
# onto the wrong bridge with no visible error. Surface it instead.
if [ -s "$docker_network_load_err" ]; then
  log "warn: error loading ${env_file_on_host} while resolving DOCKER_NETWORK: $(cat "$docker_network_load_err")"
fi
rm -f "$docker_network_load_err"
log "ensuring Docker network '$docker_network_name' exists"
incus exec "$CONTAINER_NAME" -- docker network inspect "$docker_network_name" >/dev/null 2>&1 \
  || incus exec "$CONTAINER_NAME" -- docker network create "$docker_network_name" >/dev/null

### 11. Memory-headroom check before starting bundled qdrant. This is the
### mandatory gate — dookie has a real, documented qdrant OOM-crashloop
### history; a config-integrity checksum alone would not have caught it.
if [ "$MODE" = "default" ]; then
  avail_kb="$(incus exec "$CONTAINER_NAME" -- awk '/MemAvailable/{print $2}' /proc/meminfo)"
  avail_gb=$(( avail_kb / 1024 / 1024 ))
  if [ "$avail_gb" -lt 18 ]; then
    fatal "insufficient memory headroom for bundled qdrant (${avail_gb}GB available inside container, need ~18GB) — refusing to start qdrant. Use external-qdrant mode instead, or free memory first."
  fi
  log "memory headroom check passed (${avail_gb}GB available)"
fi

### 12. Push the env-file(s) — --env-file is ALWAYS explicit, no reliance on
### Compose's implicit .env-in-cwd behavior. Push with --mode 0600 directly
### (not push-then-chmod as a separate step) — the latter leaves the secrets
### file at incus file push's default mode for the window between the two
### commands, readable by anything else in the same container in the
### meantime. Two copies are needed: $DEPLOY_PATH/.env for the nested-Docker
### compose services, and /mnt/axon-data/.env for the native axon-native
### systemd unit (step 16) — same secrets file, two paths each consumer reads
### it from.
incus file push --mode 0600 "$env_file_on_host" "$CONTAINER_NAME$DEPLOY_PATH/.env"
incus file push --mode 0600 "$env_file_on_host" "$CONTAINER_NAME/mnt/axon-data/.env"

### 13. Bring up ONLY the nested-Docker services — qdrant (default mode)/tei/
### chrome, never axon itself (see split-model rationale at the top of this
### file). Explicit service names, not a bare `up -d`, so this stays correct
### even if docker-compose.prod.yaml's `axon` service definition changes
### later.
if [ "$MODE" = "default" ]; then
  log "=== bundled qdrant IS starting locally (default mode) ==="
  incus exec "$CONTAINER_NAME" \
    --cwd "$DEPLOY_PATH" \
    -- docker compose --env-file .env -f docker-compose.prod.yaml up -d axon-qdrant axon-tei axon-chrome
else
  log "=== bundled qdrant is NOT starting, using external QDRANT_URL=${AXON_EXTERNAL_QDRANT_URL} ==="
  incus exec "$CONTAINER_NAME" \
    --cwd "$DEPLOY_PATH" \
    -- docker compose --env-file .env -f docker-compose.prod.yaml up -d axon-tei axon-chrome
fi

### 14. Health-check polling for the nested-Docker services — EXPLICIT
### bounded timeout/retry (36 * 10s = 360s max). No open-ended "wait until
### ready" loop. Assumes axon-tei/axon-chrome(/axon-qdrant) define
### healthchecks (true today).
log "polling for nested-Docker services healthy (bounded: up to 360s)"
docker_services="axon-tei axon-chrome"
[ "$MODE" = "default" ] && docker_services="axon-qdrant $docker_services"
healthy=0
for _ in $(seq 1 36); do
  statuses="$(incus exec "$CONTAINER_NAME" -- sh -c \
    "cd $DEPLOY_PATH && docker compose -f docker-compose.prod.yaml ps ${docker_services} --format '{{.Service}}:{{.Health}}'" 2>/dev/null || true)"
  if [ -n "$statuses" ] && ! printf '%s\n' "$statuses" | grep -qv "healthy$"; then
    healthy=1
    break
  fi
  sleep 10
done
if [ "$healthy" != "1" ]; then
  fatal "not all nested-Docker services reported healthy within the bounded window (360s) — inspect 'docker compose ps' inside $CONTAINER_NAME"
fi

### 15. Build axon's own binary directly inside the container, using the
### repo's own Dockerfile builder+runtime stages (matches production exactly
### — same rust:1.94.0-bookworm toolchain, same feature flags, no ad hoc
### `cargo build` invocation to keep in sync separately). Fed via a tar
### stream on stdin rather than pushing the whole repo tree to a persistent
### path first — avoids managing a stale source checkout inside the
### container between runs. This is genuinely slow on a cold nested-Docker
### build-cache (large workspace, no sccache inside the container) — expect
### several minutes on the very first run; subsequent runs reuse Docker's
### layer/BuildKit cache as long as the container itself isn't recreated.
log "building axon runtime image inside the container (this can take a while on a cold cache)"
tar cf - -C "$REPO_ROOT" \
    --exclude=target --exclude=.git --exclude=node_modules \
    --exclude=.worktrees --exclude='apps/*/node_modules' . \
  | incus exec "$CONTAINER_NAME" -- docker build -q -f config/Dockerfile --target runtime -t axon-native:runtime - \
  > /dev/null

log "extracting axon binary from the built image"
incus exec "$CONTAINER_NAME" -- sh -c '
  set -e
  cid="$(docker create axon-native:runtime)"
  docker cp "$cid:/usr/local/bin/axon" /usr/local/bin/axon
  docker rm "$cid" >/dev/null
  chmod 755 /usr/local/bin/axon
'

### 16. Install/refresh the axon-native systemd unit and (re)start it — always
### restart, even if the unit already existed, so a re-run of this script
### picks up a freshly built binary. AXON_HOME/AXON_DATA_DIR are forced to
### the Incus-internal mount path (the shared env file's own values are
### bare-host paths, not valid inside this container — same reasoning as the
### compose AXON_HOME override this replaced). AXON_HTTP_HOST is forced to
### 0.0.0.0 unconditionally: the shared env file was found (2026-07-08)
### carrying the pre-rename AXON_MCP_HTTP_HOST key, which the current binary
### does not recognize, silently falling back to its 127.0.0.1 default and
### making the whole deployment unreachable from outside the container with
### no error at all. Forcing it here is correct for this deployment target
### in every case — a native axon-native unit inside this container always
### needs to listen on all interfaces to be reachable from the host/SWAG.
log "installing axon-native systemd unit"
cat > /tmp/axon-native.service <<'UNIT'
[Unit]
Description=Axon unified server (native binary, Incus-hosted)
After=network-online.target docker.service
Wants=network-online.target

[Service]
Type=simple
EnvironmentFile=/mnt/axon-data/.env
Environment=AXON_HOME=/mnt/axon-data
Environment=AXON_DATA_DIR=/mnt/axon-data
Environment=AXON_HTTP_HOST=0.0.0.0
WorkingDirectory=/mnt/axon-data
ExecStart=/usr/local/bin/axon serve
Restart=always
RestartSec=5
User=root

[Install]
WantedBy=multi-user.target
UNIT
incus file push /tmp/axon-native.service "$CONTAINER_NAME/etc/systemd/system/axon-native.service"
rm -f /tmp/axon-native.service
incus exec "$CONTAINER_NAME" -- systemctl daemon-reload
incus exec "$CONTAINER_NAME" -- systemctl enable axon-native.service >/dev/null 2>&1 || true
incus exec "$CONTAINER_NAME" -- systemctl restart axon-native.service

### 17. Health-check polling for the native axon service — same bounded
### pattern as step 14 (36 * 10s = 360s max).
log "polling for axon-native healthy (bounded: up to 360s)"
axon_healthy=0
for _ in $(seq 1 36); do
  if incus exec "$CONTAINER_NAME" -- curl -fsS --max-time 4 http://127.0.0.1:8001/healthz >/dev/null 2>&1; then
    axon_healthy=1
    break
  fi
  sleep 10
done
if [ "$axon_healthy" != "1" ]; then
  fatal "axon-native did not become healthy within the bounded window (360s) — inspect 'systemctl status axon-native' and 'journalctl -u axon-native' inside $CONTAINER_NAME"
fi

### 18. Optional: manage the Incus `proxy` device that exposes axon's HTTP
### port to the host (for SWAG/Cloudflare to reach). Only touched when
### AXON_INCUS_PUBLISH_LISTEN is set — deliberately scoped to a specific
### interface address (e.g. the host's Tailscale IP), never 0.0.0.0, since
### the point is to expose axon only on the address the reverse proxy
### actually uses, not every interface on the shared host. This device does
### not survive a container recreate (confirmed 2026-07-08 — the prior
### working deployment had it added by hand, outside any script, and losing
### it was the reason "axon.tootie.tv" 502'd after a redeploy), hence
### managing it here on every run.
if [ -n "${AXON_INCUS_PUBLISH_LISTEN:-}" ]; then
  log "ensuring proxy device forwards ${AXON_INCUS_PUBLISH_LISTEN} -> 127.0.0.1:8001"
  incus config device remove "$CONTAINER_NAME" mcp-publish >/dev/null 2>&1 || true
  incus config device add "$CONTAINER_NAME" mcp-publish proxy \
    "listen=tcp:${AXON_INCUS_PUBLISH_LISTEN}" \
    "connect=tcp:127.0.0.1:8001" >/dev/null
else
  log "AXON_INCUS_PUBLISH_LISTEN not set — not managing host port exposure for axon"
fi

### 19. Enable Incus-level autostart. The companion systemd unit
### (axon-incus-bootstrap.service) is what actually re-runs THIS script after
### a host reboot — boot.autostart alone would restart the container but
### skip the nvidia-procfs re-application, GPU re-verification, and
### axon-native restart above.
incus config set "$CONTAINER_NAME" boot.autostart true

log "bootstrap complete (mode: $MODE)"
