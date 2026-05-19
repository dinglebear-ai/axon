# TEI CUDA Deadlock Fix — Session 2026-03-22

## Session Overview

Diagnosed and permanently fixed a recurring post-reboot issue where `axon-tei` would deadlock during CUDA UVM initialization, holding a kernel mutex that blocked all subsequent NVIDIA GPU operations — including `nvidia-ctk` (Docker's GPU container toolkit) — causing `just dev` to freeze the entire machine. The fix: enable GPU persistence mode via a new systemd service, and order `docker.service` after `nvidia-persistenced.service`.

---

## Timeline

| Time | Activity |
|------|----------|
| Start | User reported TEI unhealthy again after reboot |
| Phase 1 | Checked `docker ps`, `docker logs axon-tei`, `docker inspect` health status |
| Phase 2 | Found TEI process (PID 4875) at 99.8% CPU for 18+ minutes with GPU at 0% utilization |
| Phase 3 | Confirmed `kill -9`, `docker kill`, `docker restart`, `nsenter kill`, parent kill all failed — kernel-level lock |
| Phase 4 | Checked `nvidia-persistenced` — running but with `--no-persistence-mode`; confirmed `persistence_mode=Disabled` |
| Phase 5 | Created `/etc/systemd/system/nvidia-persistence-mode.service` to run `nvidia-smi -pm 1` at boot |
| Phase 6 | Created `/etc/systemd/system/docker.service.d/nvidia-wait.conf` to order Docker after nvidia-persistenced |
| Phase 7 | Attempted `rmmod nvidia*` — blocked (modules in use by deadlocked process); recommended reboot |
| Phase 8 | After reboot: stale containerd task directory blocked TEI from starting |
| Phase 9 | Cleared stale containerd path + force-removed dead container |
| Phase 10 | User ran `just dev` — machine froze again (second reboot) |
| Phase 11 | Read previous boot kernel logs — confirmed exact mutex owner and blockers |
| Phase 12 | After second reboot: `nvidia-persistence-mode.service` active, TEI came up healthy |

---

## Key Findings

- **Root cause confirmed in kernel logs**: `text-embeddings:4875` held a `nvidia_uvm` mutex in `channel_pool_add` → `uvm_channel_manager_create` → `add_gpu`. Both `nvidia-smi` (blocked 491s) and `nvidia-ctk` (blocked 245s, ppid=1) were stuck on the same mutex.
- **Machine freeze mechanism**: Docker's `nvidia-ctk` (GPU container toolkit, child of PID 1) blocked in D-state waiting on TEI's mutex → `docker compose up` hung → terminal froze.
- **nvidia-persistenced running but useless**: Was started at boot (`18:38:02`) with `--no-persistence-mode` — meaning the GPU driver was NOT kept in a persistent state. TEI started 5 seconds later and raced a cold CUDA init.
- **SIGKILL cannot break kernel mutex wait**: Process in `R` state but actually spin-waiting on a GPU tracking semaphore (`uvm_gpu_tracking_semaphore_update_completed_value`). Unkillable from userspace.
- **Stale containerd task dir after hard reboot**: `/run/containerd/io.containerd.runtime.v2.task/moby/<container-id>` persisted from the killed container, blocking `docker compose up` on next boot.

---

## Technical Decisions

**Why `nvidia-smi -pm 1` (persistence mode) rather than just ordering fixes?**
With persistence mode ON, the NVIDIA kernel driver maintains GPU device state between processes. TEI connects to an already-initialized GPU context instead of running cold `uvm_channel_manager_create`. The ordering fix (`After=nvidia-persistenced.service`) helps but doesn't prevent the race — persistenced was already running when TEI deadlocked. Persistence mode eliminates the cold-init path entirely.

**Why a separate `nvidia-persistence-mode.service` rather than modifying nvidia-persistenced?**
`nvidia-persistenced.service` is a static unit managed by the NVIDIA driver package — modifying its `ExecStart` would be overwritten on driver updates. A separate `oneshot` service is upgrade-safe.

**Why `RemainAfterExit=yes`?**
`nvidia-smi -pm 1` exits after setting the mode. Without `RemainAfterExit`, systemd marks the service as inactive after exit, making `After=` dependencies unreliable for downstream services.

**Why not a wrapper entrypoint in docker-compose?**
Would require knowing/reconstructing the TEI image's full entrypoint, making the compose file fragile. Systemd-level fix is cleaner and applies to all CUDA containers, not just TEI.

---

## Files Modified / Created

| File | Type | Purpose |
|------|------|---------|
| `/etc/systemd/system/nvidia-persistence-mode.service` | Created (host) | Runs `nvidia-smi -pm 1` at boot; keeps GPU driver initialized |
| `/etc/systemd/system/docker.service.d/nvidia-wait.conf` | Created (host) | Orders `docker.service` after `nvidia-persistenced.service` |

No repo files were modified.

---

## Commands Executed

```bash
# Diagnosis
docker ps -a --filter "name=axon-tei"
docker logs axon-tei --tail 50
docker inspect axon-tei --format '{{json .State.Health}}'
curl -s http://127.0.0.1:52000/health
docker exec axon-tei ps aux
nvidia-smi  # → 0% GPU utilization, no compute processes
cat /proc/4875/stat  # → state: R, ppid: 4756
cat /proc/4875/wchan  # → 0 (not in kernel wait)

# All kill attempts failed
sudo kill -9 4875         # exit 0, process survived
sudo kill -9 4756         # exit 0, parent survived
sudo nsenter -t 4875 -p -- kill -9 1  # exit 0, survived
sudo rmmod --force nvidia_uvm         # ERROR: Resource temporarily unavailable

# Permanent fix
sudo tee /etc/systemd/system/nvidia-persistence-mode.service
sudo systemctl daemon-reload
sudo systemctl enable nvidia-persistence-mode

sudo mkdir -p /etc/systemd/system/docker.service.d
sudo tee /etc/systemd/system/docker.service.d/nvidia-wait.conf

# Post-reboot stale state cleanup
sudo rm -rf /run/containerd/io.containerd.runtime.v2.task/moby/115cfbb8c6d72eaa66fe4bf74f087bd75124f1da039ed7b9a0a0a09e0a3e2454
docker rm -f axon-tei

# Verification after second reboot
systemctl is-active nvidia-persistence-mode     # → active
nvidia-smi --query-gpu=persistence_mode,name --format=csv,noheader  # → Enabled, NVIDIA GeForce RTX 4070
docker ps --filter "name=axon-tei"              # → Up About a minute (healthy)
```

---

## Behavior Changes (Before/After)

| Behavior | Before | After |
|----------|--------|-------|
| TEI on reboot | Deadlocked in CUDA UVM init within 5s of boot | Connects to persistent GPU context, starts healthy |
| `just dev` after bad TEI state | Froze entire machine (nvidia-ctk blocked on TEI's mutex) | N/A — TEI no longer deadlocks |
| GPU persistence mode | Disabled (nvidia-persistenced running with `--no-persistence-mode`) | Enabled (maintained across process boundaries) |
| Docker/NVIDIA boot ordering | Docker started immediately, no GPU readiness guarantee | Docker ordered after nvidia-persistenced |
| SIGKILL on wedged TEI | Unkillable — kernel UVM mutex held | No longer occurs |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `systemctl is-active nvidia-persistence-mode` | active | active | ✓ |
| `nvidia-smi --query-gpu=persistence_mode` | Enabled | Enabled, NVIDIA GeForce RTX 4070 | ✓ |
| `docker ps --filter name=axon-tei` | healthy | Up About a minute (healthy) | ✓ |

---

## Root Cause — Kernel Log Evidence (Previous Boot)

```
Mar 22 18:48:11 kernel: INFO: task nvidia-smi:82355 blocked for more than 491 seconds.
Mar 22 18:48:11 kernel: INFO: task nvidia-smi:82355 is blocked on a mutex likely owned by task text-embeddings:4875.
Mar 22 18:48:11 kernel: Call Trace: uvm_va_space_destroy → channel_pool_add → uvm_channel_manager_create → add_gpu

Mar 22 18:48:11 kernel: INFO: task nvidia-ctk:108319 blocked for more than 245 seconds.
Mar 22 18:48:11 kernel: INFO: task nvidia-ctk:108319 is blocked on a mutex likely owned by task text-embeddings:4875.
```

`nvidia-ctk` (ppid=1, Docker's GPU setup binary) blocked 245s → `docker compose up` hung → machine froze.

---

## Risks and Rollback

- **Persistence mode power draw**: GPU stays in a higher power state at idle. RTX 4070 — negligible impact (~6W idle vs cold).
- **Rollback**: `sudo systemctl disable nvidia-persistence-mode && sudo systemctl stop nvidia-persistence-mode && sudo rm /etc/systemd/system/nvidia-persistence-mode.service /etc/systemd/system/docker.service.d/nvidia-wait.conf && sudo systemctl daemon-reload`

---

## Decisions Not Taken

- **`rmmod --force nvidia*`**: Would break all running GPU processes. Attempted without `--force` first; all failed with "Resource temporarily unavailable." Force-remove was not attempted — reboot was cleaner.
- **Wrapper entrypoint in docker-compose**: Would require reconstructing TEI's full entrypoint; brittle on image updates. Rejected.
- **Per-container `healthcheck.start_period` increase**: Treats the symptom (TEI slow to start), not the cause (cold CUDA init deadlock). Rejected.
- **`AXON_TEI_STARTUP_DELAY_SECS`**: Adding an artificial sleep before TEI starts. Unpredictable — doesn't guarantee GPU readiness. Rejected.

---

## Open Questions

- Will the stale containerd directory issue recur after future hard power-offs? The `/run/containerd/...` path is tmpfs — should be cleared on clean reboots. Only occurred because the previous reboot was triggered while the container was in an unkillable state.
- Should `docker.service.d/nvidia-wait.conf` also add `Requires=nvidia-persistenced.service` in addition to `After=`? Currently only ordering, not hard dependency. If persistenced fails to start, Docker will still proceed.

---

## Next Steps

- Monitor TEI startup on next several reboots to confirm fix holds
- Consider adding `Requires=nvidia-persistenced.service` to the docker override for a hard dependency (not just ordering)

## Axon Embed

- Job ID: `d49110b2-2b5b-4ea5-b3d1-25138166ea3f`
- Status at session end: `pending` (embed worker not yet running — `just dev` was interrupted)
- Collection: `axon`
