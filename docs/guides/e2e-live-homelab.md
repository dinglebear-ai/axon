---
title: Trusted homelab live E2E
created: 2026-08-30
updated: 2026-09-14
---

# Trusted homelab live E2E

`E2E Live Homelab (non-required)` is a serialized, protected-environment lane.
It only runs the exact protected `main` commit on push, schedule, or explicit
manual dispatch. It must never be enabled for pull requests, forks,
`pull_request_target`, reusable workflows, merge refs, or privileged
`workflow_run` checkout.

Operators configure Tailscale workload identity federation from
`config/tailscale/axon-ci-wif.json`. The credential has only writable
`auth_keys`, only for the ephemeral `tag:axon-ci-e2e`; it cannot manage devices,
policy, SSH, DNS, API tokens, or other tags. GitHub stores the client ID and
audience as environment variables, not a Tailscale OAuth secret. The protected
`axon-live-e2e-manual` environment requires approval for manual diagnostics;
the separate `axon-live-e2e` credential environment remains unattended for
protected-main pushes and schedules.

The trust credential matches immutable GitHub owner ID `301925356`, repository
ID `1160613618`, the exact `job_workflow_ref`, generated audience, subject,
environment, ref, and event. It rejects future, expired, old, or overlong tokens;
when validating more than one token in the same evaluation session it also
rejects repeated `jti` values. Cross-invocation replay prevention belongs to
the Tailscale token exchange, not to a local untrusted CI ledger.
`validate-wif-claims.py` is the operator-policy oracle; Tailscale enforces the
exchange before peer discovery.

Apply the deny-by-default grants in
`config/tailscale/axon-ci-live-policy.hujson` after review. Destinations are
dedicated tagged HTTPS enforcement gateways on TCP 443. Never point CI at raw
shared Qdrant or Chrome ports: each gateway must require the application bearer,
return its exact peer/tag/service identity, create an unpredictable disposable
tenant lease, enforce namespace ownership/QPS/concurrency, and delete plus audit
that lease. TEI and LLM use the same boundary.

The gateway implementation and deployment contract live under
`scripts/e2e/gateway/` and `config/e2e/gateway/`. Deploy the Compose project on
the provider host with a mode-0600 environment file outside the checkout. Each
provider gets its own Tailscale state and lease database volume. Qdrant and TEI
proxy only their tenant-safe API subsets, LLM proxies only the OpenAI models and
chat-completions routes, and Chrome starts a private browser process and profile
for the active lease. The Tailscale sidecars terminate HTTPS with
`serve.json`; the Python gateways bind only to the shared loopback interface.
Chromium runs as a dedicated UID. Kernel owner rules deny every direct IPv4 and
IPv6 connection from that UID except the loopback filtering proxy, including
when a CDP caller requests a direct browser context. The proxy accepts only web
ports, rejects any DNS answer set containing a non-global address, and connects
to the already validated numeric address. The Chrome service therefore needs
`NET_ADMIN`; it fails startup if either address-family policy cannot be installed.

Required deployment settings are the four exact `*_PEER` MagicDNS identities,
four independent `*_JOIN_KEY` enrollment credentials, four independent
`*_TOKEN` management credentials, and the Qdrant, TEI, and LLM `*_UPSTREAM`
URLs. Set `*_UPSTREAM_TOKEN` only when the upstream itself requires auth. Keep
all of these values in the host secret store; never commit the deployment
environment file. The CI environment receives the gateway URLs and peers as
variables and the four management credentials as protected secrets. Lease
creation returns a short-lived, lease-specific data credential, so the tested
binary never receives a gateway management credential.

Every scenario renews every provider-visible heartbeat. A mismatched or expired
lease trips the provider circuit breaker before later mutation. Queue governance
runs before discovery: manual runs outrank pushes, pushes outrank schedules,
same-priority older work coalesces, and stale work is refused. Once mutation
starts, supersession waits because `cancel-in-progress` is false.

Repository variables hold exact MagicDNS peer names and gateway URLs; protected
environment secrets hold application bearer tokens. Values are masked before
use and never passed on argv. Provider outage/auth/network failure is classified
as infrastructure; a tested-binary invariant failure is product. The gateway
owns stale-lease expiry and janitor recovery, while every workflow `finally`
deletes all acquired leases and requires zero residuals. Tailscale action cleanup
then logs out the ephemeral node. Do not add a tailnet admin credential merely
to poll node disappearance.

The lane is initially non-required. A deployed-Axon compatibility smoke belongs
in a separate read-only job and cannot substitute for validation of the locally
built tested commit.
