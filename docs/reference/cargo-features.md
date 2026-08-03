# Axon Feature Flags

Optional Cargo features for the webclaw port. TLS fingerprinting is no longer
optional: it is a baseline dependency in every supported Axon binary.

## Feature matrix

| Feature | Description | Deps | CI cost | Status |
|---------|-------------|------|---------|--------|
| TLS fingerprinting (baseline) | wreq+BoringSSL TLS browser emulation | wreq, boring-sys (cmake/clang/perl/go) | +8-12min cold build | Always compiled; release binary smoke enforced |
| `quickjs` | QuickJS sandbox for inline JS extraction | rquickjs | +deps | Placeholder (bead b6xi closed) |
| `social-verticals` | Instagram/LinkedIn social extractors | (none) | 0 | Placeholder (bead 2mrr closed) |

## Runtime env-var gates (no recompile needed)

| Env var | Default | Description |
|---------|---------|-------------|
| `AXON_ENABLE_VERTICALS` | `true` | Enable per-site vertical extractors |
| `AXON_AUTO_DISPATCH_SKIP` | (empty) | Comma-separated extractor names to skip in auto-dispatch |
| `AXON_CHALLENGE_WARMUP` | `true` | Enable Akamai cookie-warmup retry |

## Adding a real optional dependency under a feature

When a placeholder feature gets real code:
1. Add the crate to `[dependencies]` with `optional = true`
2. Add `crate-name = ["dep:crate-name"]` to the feature line
3. Gate code with `#[cfg(feature = "feature-name")]`
4. Add CI build with `--features feature-name` in `.github/workflows/`
5. Document in this file

## CI note

BoringSSL requires `cmake`, `clang`, `perl`, and `go` on every build runner.
The Linux and Windows release jobs verify those prerequisites and assert the
built binary reports `tlsFingerprinting=true` with client initialization `ready`.
