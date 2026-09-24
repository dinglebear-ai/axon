# Axon plugin contributor instructions

The plugin is a client package for an existing Axon runtime. It does not own Axon's binary installer, deployment architecture, auth implementation, or provider runtime.

- `skills/install-axon` is the first-run orchestration skill. Delegate binary/setup/deploy work to root `install.sh`, `axon setup`, `deploy/incus`, and `deploy/systemd`.
- Every user-invocable skill needs `agents/openai.yaml`.
- Keep the production boundary accurate: supported deployments are native Axon under systemd, directly or inside the documented Incus model. Docker Compose is local/reference infrastructure, not the production Axon process.
- Non-loopback HTTP requires OAuth or static bearer. OAuth may also accept the static bearer in dual mode.
- Never claim Authelia as a supported first-class provider merely because custom Google-compatible OIDC endpoints exist; require verified compatibility evidence first.
- Installer/reverse-proxy/Incus/systemd changes require exact runtime verification and existing-config preservation.
- Validate plugin hygiene, skill structure/evaluation, setup CLI option parity, and disposable Skills CLI discovery before publication.
