---
date: 2026-05-04 15:03:16 EDT
repo: git@github.com:jmagar/axon.git
branch: bd-1d2.3/ssh-remote-deployment
head: 72e546e0
agent: Codex
session id: unknown
transcript: not found
working directory: /home/jmagar/workspace/axon_rust
worktree: /home/jmagar/workspace/axon_rust  72e546e0 [bd-1d2.3/ssh-remote-deployment]
pr: none
---

# Dolt RemotesAPI on squirts

## User Request

Enable the Dolt `remotesapi` endpoint on the remote host `squirts`, where the Docker compose project lives at `/mnt/compose/dolt` and appdata lives at `/mnt/appdata/dolt`. Avoid common exposed ports.

## Session Overview

Enabled Dolt's HTTP remotes API for the existing Dolt SQL server container on `squirts`, exposed it on non-common host port `33110`, and verified it with a real `dolt clone` from the local machine.

## Sequence of Events

1. Inspected `/mnt/compose/dolt`, `/mnt/appdata/dolt`, and the running `dolt` container on `squirts`.
2. Found the existing compose service published MySQL only: host `3311` to container `3306`.
3. Found `/mnt/appdata/dolt/config.yaml` had the `remotesapi` stanza commented out.
4. Initially considered `8000`, then changed course after user requested avoiding common ports.
5. Selected host port `33110`, checked it was not listening, and configured `33110:8000`.
6. Discovered the Dolt image only reads server YAML files from `/etc/dolt/servercfg.d`, not directly from `/var/lib/dolt/config.yaml`.
7. Mounted `/mnt/appdata/dolt/config.yaml` into `/etc/dolt/servercfg.d/config.yaml:ro`, recreated the `dolt` service, and verified startup.

## Key Findings

- `squirts` resolves to Tailscale address `100.75.111.118`.
- The running container is `dolthub/dolt-sql-server:latest`.
- The image entrypoint checks `/etc/dolt/servercfg.d` for a single YAML server config and starts `dolt sql-server --config=<file>` when present.
- Before the config mount was added, Docker published `33110`, but the container refused `10.6.0.34:8000` because `remotesapi` was not actually running.
- After mounting the config in the expected path, logs showed `Starting http server on :8000`.

## Technical Decisions

- Host port `33110` was used instead of common ports like `8000` or `8080`.
- Container port stayed `8000` because that is the Dolt server `remotesapi.port` value inside the container.
- The appdata config remained the single source of truth and was mounted read-only into the path the image expects.
- MySQL stayed unchanged on host `3311`.

## Files Modified

- Remote: `/mnt/appdata/dolt/config.yaml`
  - Enabled:
    ```yaml
    remotesapi:
      port: 8000
      read_only: false
    ```
- Remote: `/mnt/compose/dolt/docker-compose.yaml`
  - Added `33110:8000`.
  - Added read-only server config mount:
    `/mnt/appdata/dolt/config.yaml:/etc/dolt/servercfg.d/config.yaml:ro`
  - Added Dockge URL entry:
    `remotesapi://localhost:33110`
- Local: `docs/sessions/2026-05-04-dolt-remotesapi-squirts.md`
  - This session note.

## Commands Executed

- `ssh squirts 'docker ps --format ... | grep -i dolt || true'`
  - Confirmed container `dolt` was running and initially only published `3311->3306`.
- `ssh squirts 'sed -n ... /mnt/compose/dolt/docker-compose.yaml'`
  - Confirmed compose layout and volume mount.
- `ssh squirts 'sed -n ... /mnt/appdata/dolt/config.yaml'`
  - Confirmed `remotesapi` was commented out.
- `ssh squirts 'docker compose -f /mnt/compose/dolt/docker-compose.yaml config'`
  - Verified rendered compose after edits.
- `ssh squirts 'cd /mnt/compose/dolt && docker compose up -d dolt'`
  - Recreated the service.
- `dolt clone --user root http://100.75.111.118:33110/axon_rust ...`
  - Verified the remotes API worked at the Dolt protocol level.

## Errors Encountered

- Attempted to back up `/mnt/appdata/dolt/config.yaml` without `sudo`; failed because the appdata file is root-owned.
  - Resolution: used `sudo cp` and `sudo python3` for the appdata config edit.
- A broad remote grep over `/mnt/compose` and `/mnt/appdata` wandered into `node_modules` and was stopped.
  - Resolution: stopped using broad grep for port discovery and used `ss` plus targeted compose inspection.
- Publishing `33110:8000` alone did not start `remotesapi`.
  - Root cause: the Dolt image reads server YAML from `/etc/dolt/servercfg.d`, not from `/var/lib/dolt/config.yaml`.
  - Resolution: mounted the appdata config into `/etc/dolt/servercfg.d/config.yaml:ro`.

## Behavior Changes

Before:
- Dolt SQL was reachable on `100.75.111.118:3311`.
- No Dolt remotes API endpoint was active.
- `dolt clone http://100.75.111.118:33110/axon_rust` failed with connection refused.

After:
- Dolt SQL remains reachable on `100.75.111.118:3311`.
- Dolt remotes API is reachable on `100.75.111.118:33110`.
- `dolt clone --user root http://100.75.111.118:33110/axon_rust` succeeds.

## Verification Evidence

| Command | Expected | Actual | Status |
| --- | --- | --- | --- |
| `docker compose config` on `squirts` | `33110:8000` and config mount present | Rendered compose showed both | Pass |
| `docker logs --tail 80 dolt` | Remotes API startup line | `Starting http server on :8000` | Pass |
| `nc -vz -w2 100.75.111.118 33110` | TCP open | Connection succeeded | Pass |
| `nc -vz -w2 100.75.111.118 3311` | Existing MySQL still open | Connection succeeded | Pass |
| `dolt clone --user root http://100.75.111.118:33110/axon_rust ...` | Clone succeeds | `rc=0`, `remotes/origin/main` present | Pass |

## Risks and Rollback

- `remotesapi.read_only` is `false`, so pushes are allowed through the endpoint.
- Rollback:
  - Restore compose from `/mnt/compose/dolt/docker-compose.yaml.bak-20260504-150000`.
  - Restore appdata config from `/mnt/appdata/dolt/config.yaml.bak-20260504-145711`.
  - Run `cd /mnt/compose/dolt && docker compose up -d dolt`.

## Decisions Not Taken

- Did not expose host ports `8000` or `8080`; the user explicitly rejected common ports.
- Did not change the MySQL SQL port `3311`.
- Did not add firewall rules manually; Docker NAT was sufficient once the remotes API listener actually started.

## References

- Dolt server config docs: https://docs.dolthub.com/sql-reference/server/configuration
- Dolt remote docs: https://docs.dolthub.com/sql-reference/version-control/remotes

## Open Questions

- Whether `remotesapi.read_only` should remain `false` long term or be tightened after initial remote setup.

## Next Steps

Started but not completed:
- None.

Follow-on tasks:
- Configure Beads/Axon to use `http://100.75.111.118:33110/axon_rust` as the Dolt remote if desired.
- Decide whether the endpoint should be push-capable or read-only.
