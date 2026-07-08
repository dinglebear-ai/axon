# Incus Zabbly Feature-Channel Upgrade Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Switch dookie's Incus install from Ubuntu-distro-packaged 6.0.5-8 (LTS, no OCI support) to Zabbly's `stable` (monthly feature) channel and upgrade to 6.3+, unlocking native OCI application-container support, without breaking the live production `labby` container or any other existing container/profile on the host.

**Architecture:** This is a host operations task, not application code — there are no source files to create. Each task is a sequenced, verified shell operation: capture a baseline, verify the trust root, rehearse rollback, add the repo, upgrade, re-verify every pre-existing container/profile against the baseline, prove OCI actually works, then pin the repo against silent future drift. "Tests" in this plan are verification commands run against real host state, not unit tests against source code.

**Tech Stack:** `incus`/`incusd` (LXC-based), `apt`/`dpkg`, Zabbly's `pkgs.zabbly.com` APT repository, `gpg`.

## Global Constraints

- Zabbly's repo GPG key fingerprint MUST match `4EFC 5906 96CB 15B8 7C73 A3AD 82CC 8797 C838 DCFD` before it is trusted — verify before every step that depends on the key, not just once.
- `labby` (the production Labby MCP gateway, currently RUNNING) MUST still be running (or cleanly restartable) with unchanged config, MCP routing, and Tailscale identity after every mutating step.
- No destructive recovery action (`incus delete`, storage pool recreation, forced re-init) may be taken without explicit user confirmation, per bead `axon_rust-4m749.8`'s locked decision — if anything breaks in a way that isn't cleanly fixable, STOP and report.
- This bead produces no standalone doc file — all evidence for the close-gate lives in the final task's bd close-comment (a real, pasted command transcript), per the epic-wide close-gate: **no bead may be closed without citing a real command transcript / file path a reviewer can independently verify.**
- Do not run any command that installs, removes, or reconfigures anything under `labby`, `labby-golden`, `incus-web*`, `agent-run-*`, or `axon-bootstrap-temp` directly — this plan only touches the host-level `incus`/`incusd` packages and the APT repo configuration.

---

### Task 1: Capture the pre-upgrade baseline

**Files:**
- Create: `/tmp/incus-upgrade-baseline/` (scratch directory, NOT committed to git — host-local evidence only, referenced in the close-comment)

**Interfaces:**
- Consumes: nothing (first task)
- Produces: `/tmp/incus-upgrade-baseline/{incus-list.txt,profile-list.txt,profile-axon-container-profile.txt,profile-labby-gateway.txt,dpkg-incus.txt,incus-version.txt}` — every later task's "confirm unchanged" checks diff against these files

- [ ] **Step 1: Create the scratch baseline directory**

Run:
```bash
mkdir -p /tmp/incus-upgrade-baseline
```
Expected: no output, directory exists.

- [ ] **Step 2: Capture full instance list, profile list, and current package/version state**

Run:
```bash
incus list -a > /tmp/incus-upgrade-baseline/incus-list.txt
incus profile list > /tmp/incus-upgrade-baseline/profile-list.txt
incus profile show axon-container-profile > /tmp/incus-upgrade-baseline/profile-axon-container-profile.txt
incus profile show labby-gateway > /tmp/incus-upgrade-baseline/profile-labby-gateway.txt
incus config show labby --expanded > /tmp/incus-upgrade-baseline/config-labby.txt
dpkg -l | grep -i incus > /tmp/incus-upgrade-baseline/dpkg-incus.txt
incus version > /tmp/incus-upgrade-baseline/incus-version.txt
cat /tmp/incus-upgrade-baseline/*.txt
```
Expected: `incus-list.txt` shows `labby` as RUNNING with its two IPs (`100.80.57.104` tailscale0, `10.47.200.10` eth0), plus the stopped containers (`labby-golden`, `incus-web`, `incus-web-agent-golden`, `agent-run-*`, `axon-bootstrap-temp`); `incus-version.txt` shows `Client version: 6.0.5` / `Server version: 6.0.5`; `dpkg-incus.txt` shows all `incus*` packages at `6.0.5-8`.

- [ ] **Step 3: Confirm `labby`'s MCP gateway currently responds (pre-upgrade health baseline)**

Run:
```bash
incus exec labby -- curl -fsS http://127.0.0.1:8765/ready
```
Expected: HTTP 200 / a ready response (per `lab`'s own `docs/runtime/INCUS.md` runbook pattern). Record the raw output in `/tmp/incus-upgrade-baseline/labby-ready-pre.txt`:
```bash
incus exec labby -- curl -fsS http://127.0.0.1:8765/ready | tee /tmp/incus-upgrade-baseline/labby-ready-pre.txt
```

- [ ] **Step 4: No commit needed — this is a read-only baseline capture step**

This task creates no repo changes. Proceed directly to Task 2.

---

### Task 2: Verify Zabbly's GPG key fingerprint before trusting it

**Files:**
- Create: `/etc/apt/keyrings/zabbly.asc` (host file, not repo-tracked)

**Interfaces:**
- Consumes: none
- Produces: a verified, saved GPG key at `/etc/apt/keyrings/zabbly.asc` that Task 3 references by path

- [ ] **Step 1: Fetch and print the key's fingerprint**

Run:
```bash
curl -fsSL https://pkgs.zabbly.com/key.asc | gpg --show-keys --fingerprint
```
Expected: output includes a fingerprint line matching exactly `4EFC 5906 96CB 15B8 7C73 A3AD 82CC 8797 C838 DCFD` (spacing may vary, digits must match).

- [ ] **Step 2: STOP if the fingerprint does not match**

If the printed fingerprint does not match `4EFC590696CB15B87C73A3AD82CC8797C838DCFD` (compare digit-by-digit, ignore whitespace), do not proceed to Step 3. Report back with the mismatched fingerprint instead of continuing.

- [ ] **Step 3: Save the verified key**

Run:
```bash
mkdir -p /etc/apt/keyrings/
curl -fsSL https://pkgs.zabbly.com/key.asc -o /etc/apt/keyrings/zabbly.asc
gpg --show-keys --fingerprint /etc/apt/keyrings/zabbly.asc
```
Expected: the fingerprint printed from the saved file matches the one verified in Step 1.

- [ ] **Step 4: No commit needed — host-local key file, not repo-tracked**

Proceed to Task 3.

---

### Task 3: Prepare and dry-run the rollback procedure BEFORE touching the live upgrade

**Files:**
- Create: `/tmp/incus-upgrade-baseline/rollback-procedure.txt` (scratch, referenced in close-comment)

**Interfaces:**
- Consumes: `dpkg-incus.txt` from Task 1 (the exact pre-upgrade package/version string to roll back to)
- Produces: a verified-workable rollback command sequence, proven via `apt-get install --dry-run` (not actually executed) before any real upgrade happens

- [ ] **Step 1: Confirm the exact pre-upgrade package versions to roll back to**

Run:
```bash
cat /tmp/incus-upgrade-baseline/dpkg-incus.txt
```
Expected: all `incus*` packages at version `6.0.5-8` (matches Task 1's capture).

- [ ] **Step 2: Dry-run the downgrade command (does NOT execute anything — proves the rollback path exists before you need it)**

Run:
```bash
apt-get install --dry-run \
  incus=6.0.5-8 incus-base=6.0.5-8 incus-client=6.0.5-8 incus-agent=6.0.5-8
```
Expected: apt reports a plan to install/downgrade to these exact versions (from the currently-configured Ubuntu universe repo, since Zabbly's repo isn't added yet at this point in the plan) with no errors about unavailable versions. If apt reports the exact versions are unavailable, note this — it means once Zabbly's repo is added in Task 4, the downgrade dry-run must be re-verified against Zabbly's own available version list (Ubuntu's universe repo may age out old point releases).

- [ ] **Step 3: Record the rollback command sequence for real use if needed**

Write the verified sequence to a scratch file:
```bash
cat > /tmp/incus-upgrade-baseline/rollback-procedure.txt <<'ROLLBACK'
# Verified rollback procedure (dry-run tested 2026-07-07, before live upgrade)
# Use ONLY if the Zabbly upgrade breaks labby or another existing container/profile
# in a way that isn't cleanly fixable. Do NOT run without explicit confirmation.

# 1. Downgrade packages back to the pre-upgrade baseline version:
apt-get install incus=6.0.5-8 incus-base=6.0.5-8 incus-client=6.0.5-8 incus-agent=6.0.5-8

# 2. Remove the Zabbly repo source so it can't be picked up again by a future apt upgrade:
rm -f /etc/apt/sources.list.d/zabbly-incus-stable.sources
apt-get update

# 3. Verify labby comes back:
incus exec labby -- curl -fsS http://127.0.0.1:8765/ready
incus list -a
ROLLBACK
cat /tmp/incus-upgrade-baseline/rollback-procedure.txt
```
Expected: file is written and its contents match the heredoc above.

- [ ] **Step 4: No commit needed — host-local scratch file, referenced (not committed) in the final close-comment**

Proceed to Task 4.

---

### Task 4: Add and scope the Zabbly repo (does not upgrade anything yet)

**Files:**
- Create: `/etc/apt/sources.list.d/zabbly-incus-stable.sources` (host file, not repo-tracked)

**Interfaces:**
- Consumes: `/etc/apt/keyrings/zabbly.asc` from Task 2
- Produces: an APT source pointed at Zabbly's `stable` (feature) channel, ready for Task 5's upgrade

- [ ] **Step 1: Write the Zabbly stable-channel source file**

Run (as root):
```bash
cat <<EOF > /etc/apt/sources.list.d/zabbly-incus-stable.sources
Enabled: yes
Types: deb
URIs: https://pkgs.zabbly.com/incus/stable
Suites: $(. /etc/os-release && echo ${VERSION_CODENAME})
Components: main
Architectures: $(dpkg --print-architecture)
Signed-By: /etc/apt/keyrings/zabbly.asc
EOF
cat /etc/apt/sources.list.d/zabbly-incus-stable.sources
```
Expected: file contains `Enabled: yes`, `URIs: https://pkgs.zabbly.com/incus/stable`, a `Suites:` line matching this host's actual Ubuntu codename (e.g. `noble` or `resolute` — whatever `/etc/os-release`'s `VERSION_CODENAME` reports), `Signed-By: /etc/apt/keyrings/zabbly.asc`.

- [ ] **Step 2: Refresh apt and confirm the new repo is reachable and offers a newer Incus version**

Run:
```bash
apt-get update
apt-cache policy incus
```
Expected: `apt-get update` completes with no GPG or 404 errors for the Zabbly source; `apt-cache policy incus` now shows a candidate version ≥ 6.3 from `pkgs.zabbly.com`, alongside the currently-installed `6.0.5-8` from Ubuntu's universe repo.

- [ ] **Step 3: No commit needed — host-local repo config file, not repo-tracked**

Proceed to Task 5 (the actual upgrade).

---

### Task 5: Perform the upgrade and immediately re-verify `labby`

**Files:**
- Create: `/tmp/incus-upgrade-baseline/post-upgrade-labby-ready.txt` (scratch, referenced in close-comment)

**Interfaces:**
- Consumes: the baseline files from Task 1
- Produces: an upgraded `incus`/`incusd` (6.3+) with `labby` verified still healthy — the precondition every later task assumes

- [ ] **Step 1: Perform the upgrade**

Run:
```bash
apt-get install incus incus-base incus-client incus-agent
```
Expected: apt installs a new version ≥ 6.3 for all four packages from the Zabbly source, restarting `incusd` as part of the package's postinst. Watch for any error output — if the command fails or reports broken dependencies, STOP here and do not proceed; report back rather than attempting an ad hoc fix.

- [ ] **Step 2: Confirm the new version**

Run:
```bash
incus version | tee /tmp/incus-upgrade-baseline/post-upgrade-incus-version.txt
```
Expected: `Client version:` and `Server version:` both show 6.3 or higher.

- [ ] **Step 3: Immediately confirm `labby` is still running and its gateway responds**

Run:
```bash
incus list labby
incus exec labby -- curl -fsS http://127.0.0.1:8765/ready | tee /tmp/incus-upgrade-baseline/post-upgrade-labby-ready.txt
```
Expected: `incus list labby` shows `RUNNING`; the `/ready` curl returns the same successful response captured in Task 1 Step 3. If `labby` is not running, run `incus restart labby` once and re-check — if it still doesn't come up cleanly, STOP and report per the Global Constraints (no destructive recovery without confirmation).

- [ ] **Step 4: No git commit for this step** — this is a live host state change, not a repo change. Evidence (the version + ready-check output) is captured in the scratch files above for the final task's close-comment.

Proceed to Task 6.

---

### Task 6: Verify every pre-existing container and profile survived unchanged

**Files:**
- Create: `/tmp/incus-upgrade-baseline/post-upgrade-diff.txt`

**Interfaces:**
- Consumes: all baseline files from Task 1
- Produces: a diff report proving no pre-existing container/profile config drifted — the evidence Task 8's close-comment cites for "all pre-existing containers/profiles/storage survive with unchanged config"

- [ ] **Step 1: Re-capture the same state Task 1 captured, and diff against the baseline**

Run:
```bash
incus list -a > /tmp/incus-upgrade-baseline/post-incus-list.txt
incus profile list > /tmp/incus-upgrade-baseline/post-profile-list.txt
incus profile show axon-container-profile > /tmp/incus-upgrade-baseline/post-profile-axon-container-profile.txt
incus profile show labby-gateway > /tmp/incus-upgrade-baseline/post-profile-labby-gateway.txt
incus config show labby --expanded > /tmp/incus-upgrade-baseline/post-config-labby.txt

diff /tmp/incus-upgrade-baseline/profile-axon-container-profile.txt /tmp/incus-upgrade-baseline/post-profile-axon-container-profile.txt
diff /tmp/incus-upgrade-baseline/profile-labby-gateway.txt /tmp/incus-upgrade-baseline/post-profile-labby-gateway.txt
diff /tmp/incus-upgrade-baseline/config-labby.txt /tmp/incus-upgrade-baseline/post-config-labby.txt
```
Expected: all three `diff` commands produce **no output** (or only diffs in fields that are expected to change across a restart, such as `volatile.last_state.power` timestamps — inspect any non-empty diff line-by-line and confirm it is not a config/device/limits change). Save the full diff transcript:
```bash
{
  echo "=== axon-container-profile diff ==="
  diff /tmp/incus-upgrade-baseline/profile-axon-container-profile.txt /tmp/incus-upgrade-baseline/post-profile-axon-container-profile.txt
  echo "=== labby-gateway profile diff ==="
  diff /tmp/incus-upgrade-baseline/profile-labby-gateway.txt /tmp/incus-upgrade-baseline/post-profile-labby-gateway.txt
  echo "=== labby config diff ==="
  diff /tmp/incus-upgrade-baseline/config-labby.txt /tmp/incus-upgrade-baseline/post-config-labby.txt
} > /tmp/incus-upgrade-baseline/post-upgrade-diff.txt
cat /tmp/incus-upgrade-baseline/post-upgrade-diff.txt
```

- [ ] **Step 2: Confirm every container from the baseline list is still present with the same state**

Run:
```bash
diff <(awk '{print $2, $4}' /tmp/incus-upgrade-baseline/incus-list.txt) \
     <(awk '{print $2, $4}' /tmp/incus-upgrade-baseline/post-incus-list.txt)
```
Expected: no output — every container name + state (RUNNING/STOPPED) column matches between pre- and post-upgrade. (This awk-column comparison is approximate given `incus list`'s table formatting; if the diff is noisy, visually cross-check `post-incus-list.txt` against `incus-list.txt` line-by-line instead — the goal is confirming no container vanished and no RUNNING container became STOPPED unexpectedly.)

- [ ] **Step 3: STOP if any unexpected diff is found**

If any diff shows a real config/device/limits/state change (not just an expected timestamp/volatile field), STOP and report the specific diff rather than proceeding to Task 7. Do not attempt to silently "fix" a drifted profile.

- [ ] **Step 4: No commit needed** — scratch evidence only.

Proceed to Task 7.

---

### Task 7: Pin the Incus packages against silent future drift

**Files:**
- Modify: apt's package-hold state (not a file in this repo)

**Interfaces:**
- Consumes: nothing new
- Produces: a host where a routine `apt upgrade` cannot silently move Incus to a different version without an explicit `apt-mark unhold` first

- [ ] **Step 1: Hold the four Incus packages at their newly-installed version**

Run:
```bash
apt-mark hold incus incus-base incus-client incus-agent
apt-mark showhold
```
Expected: `apt-mark showhold` lists all four package names.

- [ ] **Step 2: Confirm a routine `apt upgrade` no longer touches Incus**

Run:
```bash
apt-get upgrade --dry-run 2>&1 | grep -i incus
```
Expected: no output (or explicitly shows the held packages are excluded from the upgrade plan) — proving future unattended/routine upgrades won't silently move Incus versions.

- [ ] **Step 3: No commit needed** — host package-manager state, not repo-tracked.

Proceed to Task 8.

---

### Task 8: Minimal OCI smoke test, cleanup, and final close-comment evidence

**Files:**
- None created in the repo (per bead `.8`'s Files section: no standalone doc — evidence lives in the bd close-comment)

**Interfaces:**
- Consumes: the upgraded, verified, pinned Incus install from Tasks 5–7
- Produces: proof the upgrade actually unlocked OCI support, satisfying `.8`'s Validation criteria — the final gate before this bead can close

- [ ] **Step 1: Register the Docker Hub OCI remote**

Run:
```bash
incus remote add docker https://docker.io --protocol=oci
incus remote list
```
Expected: `incus remote list` shows a `docker` entry with protocol `oci`.

- [ ] **Step 2: Launch a minimal, ephemeral OCI instance**

Run:
```bash
incus launch docker:hello-world oci-smoke-test --ephemeral
```
Expected: the instance launches and (being an ephemeral, run-to-completion image) either exits and self-deletes, or runs briefly — confirm with:
```bash
incus list oci-smoke-test
```
Expected: either the instance is gone already (ephemeral cleanup happened) or shows as STOPPED/exited; this is normal for `hello-world`-style images. If the instance is still present, clean it up explicitly:
```bash
incus delete oci-smoke-test --force 2>/dev/null || true
incus list oci-smoke-test
```
Expected final state: `incus list oci-smoke-test` returns no matching instance.

- [ ] **Step 3: Confirm no orphaned instances/profiles remain from this entire task sequence**

Run:
```bash
incus list -a
incus profile list
```
Expected: identical to the post-upgrade baseline from Task 6 (Step 2's diff target), plus no `oci-smoke-test` instance and no new profiles created by this plan.

- [ ] **Step 4: Assemble the close-comment evidence and update the bead**

Run:
```bash
cd /home/jmagar/workspace/axon
bd comments add axon_rust-4m749.8 "$(cat <<'EOF'
FACT: Zabbly stable-channel upgrade completed and verified on dookie (2026-07-07). Evidence (full transcripts in /tmp/incus-upgrade-baseline/, host-local scratch — pasted key excerpts below per the epic's close-gate requiring a citable, independently-verifiable transcript, not just prose):

$(cat /tmp/incus-upgrade-baseline/incus-version.txt)
--- upgraded to ---
$(cat /tmp/incus-upgrade-baseline/post-upgrade-incus-version.txt)

labby post-upgrade health check:
$(cat /tmp/incus-upgrade-baseline/post-upgrade-labby-ready.txt)

Profile/config diff (pre vs post upgrade) — empty or timestamp-only, no config drift:
$(cat /tmp/incus-upgrade-baseline/post-upgrade-diff.txt)

Packages held at new version: $(apt-mark showhold | tr '\n' ' ')

OCI smoke test: incus launch docker:hello-world succeeded, cleaned up, no orphaned instances/profiles remain (incus list -a / incus profile list checked against baseline).

Rollback procedure verified via dry-run BEFORE the live upgrade (not needed, upgrade succeeded): see /tmp/incus-upgrade-baseline/rollback-procedure.txt on dookie.
EOF
)"
bd update axon_rust-4m749.8 -s closed --notes "Closed per plan docs/superpowers/plans/2026-07-07-incus-zabbly-upgrade.md, all 8 tasks completed and verified."
```
Expected: `bd comments add` succeeds; `bd update ... -s closed` reports the bead is now closed.

- [ ] **Step 5: No git commit** — this task produced no repo file changes (per `.8`'s Files section: "None as a standalone deliverable"). The evidence lives in the bd comment (Step 4) and the host-local scratch directory `/tmp/incus-upgrade-baseline/` it quotes from. If you want a permanent record beyond bd + host scratch, that's `.7`'s consolidated doc's job later, not this bead's.

---

## Self-Review Notes

**Spec coverage check against `axon_rust-4m749.8`'s locked decisions:**
- GPG fingerprint verification → Task 2 ✓
- Baseline snapshot before upgrade → Task 1 ✓
- Rehearsed rollback prepared/dry-run BEFORE live upgrade → Task 3 ✓
- Post-upgrade `labby` health/config verification → Tasks 5–6 ✓
- Post-upgrade verification of all pre-existing containers/profiles → Task 6 ✓
- Exact validated Incus version recorded → Tasks 5, 8 ✓
- Stop-and-report (no silent rollback/destructive recovery) → stated as a Global Constraint and repeated at each risk point (Tasks 5, 6) ✓
- Repo pinning against silent future drift → Task 7 ✓
- Minimal OCI smoke test → Task 8 ✓
- No standalone doc file; evidence via close-comment citing a real transcript → Task 8 Step 4 ✓
- Epic-wide close-gate (citable commit/file path, not prose) → satisfied by quoting real captured file contents directly into the close-comment in Task 8 Step 4, not a paraphrased summary

**Placeholder scan:** no TBD/TODO/"add error handling" language found — every step has a concrete command and expected output.

**Type/name consistency:** the baseline directory path (`/tmp/incus-upgrade-baseline/`) and its exact filenames are introduced in Task 1 and referenced identically (same names) in every later task — no renamed variables.
