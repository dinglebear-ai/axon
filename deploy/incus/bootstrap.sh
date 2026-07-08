#!/usr/bin/env bash
# Idempotent bootstrap for axon's nested-Docker-in-Incus deployment.
#
# Usage:
#   deploy/incus/bootstrap.sh [default|external-qdrant]
#
# default mode:          bundled qdrant starts alongside axon/tei/chrome.
# external-qdrant mode:  requires AXON_EXTERNAL_QDRANT_URL; bundled qdrant is
#                        not started, axon points at the external instance.
#
# Safe to re-run at any time (idempotent). Also the intended entry point for
# the companion systemd unit (axon-incus-bootstrap.service) that re-runs this
# on every host boot, since boot.autostart=true alone does not re-apply the
# known-fragile nvidia-procfs device or re-verify GPU access.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CONTAINER_NAME="${AXON_INCUS_CONTAINER:-axon-bootstrap-temp}"
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

### 5. Install Docker inside the container if missing (idempotent).
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

### 6. Re-apply the nvidia-procfs device. REQUIRED ON EVERY RUN — confirmed
### (bead axon_rust-4m749.2) this does not reliably survive a container
### stop/start cycle, silently breaking nested-Docker GPU passthrough until
### removed and re-added.
log "re-applying nvidia-procfs device (known fragility across restarts)"
incus config device remove "$CONTAINER_NAME" nvidia-procfs >/dev/null 2>&1 || true
incus config device add "$CONTAINER_NAME" nvidia-procfs disk \
  "source=/proc/driver/nvidia/gpus/${GPU_PCI_ADDRESS}" \
  "path=/proc/driver/nvidia/gpus/${GPU_PCI_ADDRESS}" >/dev/null

### 7. Fail-closed GPU verification BEFORE anything GPU-dependent starts.
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

### Resolve the env file now — needed by both the network-name lookup (9)
### and the actual push (12).
env_file_on_host="${AXON_ENV_FILE:-$HOME/.axon/.env}"
[ -f "$env_file_on_host" ] || fatal "env file not found at $env_file_on_host"

### 8. Sync deploy artifacts (compose files + config/) into the container.
log "syncing compose files + config into ${DEPLOY_PATH}"
incus exec "$CONTAINER_NAME" -- mkdir -p "$DEPLOY_PATH"
incus file push "$REPO_ROOT/docker-compose.prod.yaml" "$CONTAINER_NAME$DEPLOY_PATH/docker-compose.prod.yaml"
incus file push "$REPO_ROOT/docker-compose.external-qdrant.yaml" "$CONTAINER_NAME$DEPLOY_PATH/docker-compose.external-qdrant.yaml"
incus file push -r "$REPO_ROOT/config" "$CONTAINER_NAME$DEPLOY_PATH/"

### 9. Ensure the external Docker network exists (idempotent — compose
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

### 10. [removed] A prior version of this step computed a checksum of the
### files this same script had just pushed in step 8 and compared it against
### the last run's checksum — which can never differ, since step 8
### unconditionally overwrites the container's copy with the host's current
### copy on every run. That made it a no-op that logged a reassuring message
### without ever being able to detect anything, which is worse than no check
### at all (a future reader could mistake it for real drift detection). The
### memory-headroom check in step 11 is this script's actual fail-closed
### gate; if genuine tamper-evidence for the deployed config is needed later,
### it needs to compare a HOST-side hash (computed before step 8's push, from
### a value stored outside this script's own write path) — not container-side
### state this script itself just wrote.

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

### 12. Push the env-file and start the stack — --env-file is ALWAYS explicit,
### no reliance on Compose's implicit .env-in-cwd behavior. Push with
### --mode 0600 directly (not push-then-chmod as a separate step) — the
### latter leaves the secrets file at incus file push's default mode for the
### window between the two commands, readable by anything else in the same
### container in the meantime.
incus file push --mode 0600 "$env_file_on_host" "$CONTAINER_NAME$DEPLOY_PATH/.env"
# The axon service's own `env_file:` directive (compose, not the CLI flag
# above) reads from ${AXON_HOME}/.env — which resolves to /mnt/axon-data/.env
# once AXON_HOME is overridden below. Keep that copy in sync too (same
# secrets file, two paths compose needs it at — see README for why the
# split exists).
incus file push --mode 0600 "$env_file_on_host" "$CONTAINER_NAME/mnt/axon-data/.env"

# The shared ~/.axon/.env carries host-native paths meant for bare-host/
# local-dev use (e.g. AXON_HOME=/home/jmagar/.axon, GEMINI_HOME resolving off
# a bare-host $HOME) and may set AXON_IMAGE for a manually-tagged local build.
# None of that is valid *inside* this Incus container, where data actually
# lives at the mounted /mnt/axon-data and /mnt/axon-gemini — force the
# correct values explicitly rather than trust whatever's in the shared file.
# (Also: `incus exec` runs as root by default, so an unset $HOME-relative
# default would resolve against /root, not /home/axon — another reason these
# must be explicit, not left to the compose file's own fallback.)
#
# Passed via `incus exec --env`/`--cwd` flags, not interpolated into an
# `sh -c` string — these values (AXON_INCUS_IMAGE, AXON_EXTERNAL_QDRANT_URL)
# come from a trusted operator-authored env file today, but string
# interpolation into a root-executed shell command is a shape worth avoiding
# even when the current threat model doesn't require it: a stray single
# quote in either value would otherwise splice arbitrary content into the
# command.
axon_incus_image="${AXON_INCUS_IMAGE:-ghcr.io/jmagar/axon:latest}"

if [ "$MODE" = "default" ]; then
  log "=== bundled qdrant IS starting locally (default mode) ==="
  incus exec "$CONTAINER_NAME" \
    --cwd "$DEPLOY_PATH" \
    --env AXON_HOME=/mnt/axon-data \
    --env GEMINI_HOME=/mnt/axon-gemini \
    --env "AXON_IMAGE=${axon_incus_image}" \
    -- docker compose --env-file .env -f docker-compose.prod.yaml up -d
else
  log "=== bundled qdrant is NOT starting, using external QDRANT_URL=${AXON_EXTERNAL_QDRANT_URL} ==="
  incus exec "$CONTAINER_NAME" \
    --cwd "$DEPLOY_PATH" \
    --env AXON_HOME=/mnt/axon-data \
    --env GEMINI_HOME=/mnt/axon-gemini \
    --env "AXON_IMAGE=${axon_incus_image}" \
    --env "AXON_EXTERNAL_QDRANT_URL=${AXON_EXTERNAL_QDRANT_URL}" \
    -- docker compose --env-file .env -f docker-compose.prod.yaml -f docker-compose.external-qdrant.yaml up -d
fi

### 13. Health-check polling — EXPLICIT bounded timeout/retry (36 * 10s =
### 360s max), sized against axon's own 180s start_period plus margin. No
### open-ended "wait until ready" loop.
### Assumes every service in docker-compose.prod.yaml defines a healthcheck
### (true today for axon/axon-tei/axon-chrome/axon-qdrant) — a service added
### later without one reports an empty .Health, which correctly fails this
### check rather than being silently ignored, but will spin the full 360s
### every time until it gets a healthcheck of its own.
log "polling for all services healthy (bounded: up to 360s)"
healthy=0
for _ in $(seq 1 36); do
  statuses="$(incus exec "$CONTAINER_NAME" -- sh -c \
    "cd $DEPLOY_PATH && docker compose -f docker-compose.prod.yaml ps --format '{{.Service}}:{{.Health}}'" 2>/dev/null || true)"
  if [ -n "$statuses" ] && ! printf '%s\n' "$statuses" | grep -qv "healthy$"; then
    healthy=1
    break
  fi
  sleep 10
done
if [ "$healthy" != "1" ]; then
  # Fail closed here, not just warn — this is the one signal the systemd
  # unit's Restart=no is designed to surface (a failed unit after boot means
  # "check systemctl status", per axon-incus-bootstrap.service). Falling
  # through to "bootstrap complete" with exit 0 would silently swallow the
  # most common real-world failure mode (slow model load, transient GPU
  # flake) exactly where the fail-closed design matters most. Services that
  # did start are left running, not torn down — inspect 'docker compose ps'
  # inside the container to see what's actually up.
  fatal "not all services reported healthy within the bounded window (360s) — inspect 'docker compose ps' inside $CONTAINER_NAME"
fi

### 14. Enable Incus-level autostart. The companion systemd unit
### (axon-incus-bootstrap.service) is what actually re-runs THIS script after
### a host reboot — boot.autostart alone would restart the container but
### skip the nvidia-procfs re-application and GPU re-verification above.
incus config set "$CONTAINER_NAME" boot.autostart true

log "bootstrap complete (mode: $MODE)"
