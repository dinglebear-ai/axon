# Axon end-to-end test operations

This runbook operates the catalog-driven E2E system. The authoritative coverage
denominator is `tests/e2e/catalog/catalog.json`, the fixed input contract is
`tests/e2e/corpus/manifest.json`, and `scripts/e2e/lib/reporting.py` owns the
canonical report. A green scenario is not a passing run when evidence validation
or teardown fails.

## Trust boundaries and execution lanes

| Lane | Boundary | Command or workflow |
|---|---|---|
| Inventory | network-free catalog drift | `just e2e-catalog-check` |
| Hermetic local | loopback-only owned doubles | `just e2e-hermetic` |
| Required | fork-safe, no private credentials | `.github/workflows/e2e-hermetic.yml` |
| Live | protected main/schedule/approved manual, authenticated gateways | `.github/workflows/e2e-live.yml` |
| Platform | bounded Linux/macOS/Windows contract | `just e2e-platform-smoke` |
| Performance | measured, report-only until promoted | `just e2e-performance` |
| Upgrade | digest-pinned schema-epoch fixture | `just e2e-upgrade` |
| Mutation | oracle sensitivity | `just e2e-mutations representative` |
| Qualification | pure saved-evidence projection | `just e2e-qualification INDEX EVIDENCE_ROOT` |

The MacBook is the primary development machine; local operation must not depend
on the retiring `dookie` VM. Use the repository toolchain and locally built Axon.
`scripts/e2e/run-hermetic-local.sh` handles portable macOS OpenSSL discovery.
Live services remain external behind gateways. A deployed-Axon compatibility
smoke is a distinct read-only family and cannot replace tested-binary live proof.

## Local setup and ordinary operation

Run from the worktree:

```sh
just e2e-catalog-check
just e2e-isolation-check
just e2e-hermetic
just e2e-platform-smoke target/e2e/platform-smoke-local.json
just e2e-flake-governance target/e2e/platform-smoke-local.json local target/e2e/reliability.json
just e2e-performance 5 target/e2e/performance/report.json --allow-contended
just e2e-upgrade
just e2e-mutations representative
```

Hermetic E2E owns its doubles, allocation, ports, data root, processes, and
teardown. Never aim it at a shared Qdrant, Chrome profile, SQLite database, or
Axon daemon. Custom plans must use the supervised route so `finally` teardown
and the canonical cleanup projection cannot be skipped:

```sh
just e2e-supervised-report path/to/plan.json target/e2e/report.json target/e2e/junit.xml
```

## Required hermetic check

The stable branch-protection context is **E2E Hermetic Required**. Policy lives
in `config/e2e/hermetic-required-policy.json`. Required runs reject non-loopback
routing, carry no live credentials, use `cancel-in-progress: false`, and retain
cleanup evidence on failure. The candidate never counts toward its own history;
prior-main reports are bound to GitHub API run identity and artifact digests.
Repository code does not mutate branch protection. Details are in
`docs/guides/e2e-hermetic-ci.md`.

## Live WIF, grants, and provider gateways

Operators, not repository automation, deploy:

- `config/tailscale/axon-ci-wif.json`, which binds immutable GitHub owner,
  repository, workflow, audience, environment/ref, token lifetime, and replay.
- `config/tailscale/axon-ci-live-policy.hujson`, whose deny-by-default grants
  allow dedicated ephemeral `tag:axon-ci-e2e` only to exact tagged TCP 443
  gateways.
- `config/e2e/live-services.json`, the peer, lease, heartbeat, rate, ownership,
  and cleanup contract.

The workflow pins Tailscale action
`780049a30b6ff5c378a9e7b389d15ece7a204888` and client `1.94.0`, uses WIF rather
than a long-lived OAuth secret, and has only `contents: read` and
`id-token: write`. Trusted triggers are protected main, schedule, and approved
manual dispatch—never forks, pull requests, `pull_request_target`, or merge refs.

Repository variables hold peer names, gateway URLs, expected peer set, WIF
client ID, and audience. Protected-environment secrets hold application bearer
values. Never track, print, or pass values on argv. Each gateway must authenticate
requests; validate owner/run/attempt and heartbeat; enforce TTL, QPS, concurrency,
and tenant isolation; and block raw shared Qdrant/Chrome enumeration or mutation.
Preflight binds configured DNS names to real `tailscale status` node identity,
tag, address, and online state, rather than trusting gateway self-report alone.

Execution order is admission, exact checkout/build, WIF attestation, ephemeral
join, masking, peer/gateway preflight, tested-binary scenarios, canonical
teardown, report verification, and artifact retention. See
`docs/guides/e2e-live-homelab.md`.

## Failure taxonomy and triage

- `product`: tested Axon behavior or invariant failed.
- `fixture`: owned corpus or deterministic fixture is invalid.
- `provider`: authenticated provider is unavailable or breaks its contract.
- `auth/network`: WIF, peer identity, gateway auth, or routing failed.
- `cleanup`: teardown refused an identity or found an owned residual.
- `harness`: orchestration, schema, evidence, or runner logic failed.

`timed_out` and `canceled` remain terminal attempts requiring classification and
teardown. Start with canonical JSON, preserve the first-attempt failure, then
inspect bounded evidence references and cleanup phases. Never relabel a product
failure as infrastructure or a provider outage as product.

## Evidence, redaction, and qualification

Evidence is UTF-8, bounded, sanitized, digest-addressed, and producer-bound.
Never retain `.env`, `jobs.db`, `config.toml`, Chrome profiles, raw private
content, credentials, private endpoints, or database snapshots. Reports bind
tested SHA, versions, attempts, invariants, failure taxonomy, evidence digests,
and the authoritative teardown audit.

Qualification policy is `config/e2e/qualification-policy.json`:

```sh
just e2e-qualification target/e2e/qualification-index.json target/e2e/evidence \
  target/e2e/qualification.json target/e2e/qualification.md target/e2e/SHA256SUMS
```

It consumes saved hermetic, live, platform, upgrade, sensitivity, reliability,
observability, performance, security, and cleanup evidence without rerunning.
Missing, stale, future, mismatched, unredacted, or checksum-bad required evidence
is incomplete or failed. Output is unsigned and non-release-eligible; signing is
a separate minimal-permission approved identity with no live credentials. See
`docs/testing/release-qualification.md`.

## Teardown, cancellation, and stale recovery

Register resources before mutation in the signed ownership manifest. Provider
cleanup precedes local process/data cleanup; intents and receipts are signed;
success requires no refused identities and an exhaustive empty residual audit.

On Ctrl-C, timeout, cancellation, or product failure, allow the supervisor to
finish. If it died, recover only from the saved exact manifest:

```sh
python3 scripts/e2e/lib/teardown.py target/e2e/run/manifest.json \
  --report target/e2e/run/teardown-recovery.json \
  --provider-config target/e2e/run/provider-config.json
```

The stale janitor is preview-first. Add `--execute` only after exact lease,
expired TTL, stale heartbeat, and ownership are proven:

```sh
python3 scripts/e2e/lib/stale-janitor.py target/e2e/registry.json \
  --lease EXPECTED_LEASE --report target/e2e/janitor-preview.json
```

Never broadly delete ports, processes, directories, collections, or an active or
unowned resource. Require teardown success and an empty residual audit before a
rerun.

## Troubleshooting

| Symptom | Safe response |
|---|---|
| Catalog drift | Run `just e2e-catalog-check`; reconcile authoritative inventories, never hand-edit counts. |
| Hermetic non-loopback refusal | Remove inherited live/shared URLs and rerun the hermetic wrapper. |
| Live admission refusal | Use protected main, schedule, or approved manual dispatch; do not weaken admission. |
| WIF failure | Correct deployed immutable claims/audience; never add OAuth-secret fallback. |
| Peer mismatch | Fix DNS/tag/address inventory; gateway JSON alone is not proof. |
| Provider circuit open | Stop later mutation, preserve the first failure, complete teardown, restore gateway health. |
| Cleanup failure | Recover from the exact manifest and receipts; never use broad deletion. |
| Performance incomparable | Retain as infrastructure evidence; do not promote it or call it regression. |
| Qualification incomplete | Produce fresh matching evidence; do not disguise missing work as not-applicable. |

## Outage bypass, rollback, quarantine, and release

Bypass is repository-administrator-only during a declared incident, with a merge
record link and mandatory successful post-merge rerun. Flakes, queue delay, and
convenience are not exceptions. Rollback removes only the external required
context and preserves workflow/evidence collection.

Quarantine is governed by `config/e2e/quarantine.json` and
`config/e2e/quarantine.schema.json`. Security, authentication, trust-boundary,
redaction, and cleanup cases cannot be quarantined. An entry remains executed
and visible, contributes no healthy coverage, and requires owner, issue,
rationale, restoration metric, and bounded expiry. Validate it with
`just e2e-flake-governance`.

For release, assemble exact-candidate/corpus evidence, select the
release-candidate profile, validate JSON and checksums, require every mandatory
family and empty teardown, then hand the exact unsigned artifact to the separate
authorized signer. Approved outage exceptions must exactly match policy. Never
release an incomplete, unsigned, stale, mismatched, cleanup-failed, or
quarantined result.
