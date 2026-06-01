# Axon Feature Flags

Optional Cargo features for the webclaw port. All are **disabled by default**.

## Feature matrix

| Feature | Description | Deps | CI cost | Status |
|---------|-------------|------|---------|--------|
| `tls-fingerprinting` | wreq+BoringSSL TLS browser emulation | wreq, boring-sys (cmake/clang/perl/go) | +8-12min cold build | Placeholder (bead wf4s closed) |
| `quickjs` | QuickJS sandbox for inline JS extraction | rquickjs | +deps | Placeholder (bead b6xi closed) |
| `social-verticals` | Instagram/LinkedIn social extractors | (none) | 0 | Placeholder (bead 2mrr closed) |

## Runtime env-var gates (no recompile needed)

| Env var | Default | Description |
|---------|---------|-------------|
| `AXON_ENABLE_VERTICALS` | `true` | Enable per-site vertical extractors |
| `AXON_AUTO_DISPATCH_SKIP` | (empty) | Comma-separated extractor names to skip in auto-dispatch |
| `AXON_CHALLENGE_WARMUP` | `true` | Enable Akamai cookie-warmup retry |
| `AXON_VERTICAL_SCRAPE_AUTO_DISPATCH` | `true` | Auto-dispatch verticals on crawl URLs |
| `AXON_JS_EVAL_ENABLED` | `false` | Enable QuickJS JS evaluation (requires `quickjs` feature) |
| `AXON_ENABLE_SOCIAL_VERTICALS` | `false` | Enable Instagram/LinkedIn extractors (requires `social-verticals` feature) |

## Adding a real dependency under a feature

When a placeholder feature gets real code:
1. Add the crate to `[dependencies]` with `optional = true`
2. Add `crate-name = ["dep:crate-name"]` to the feature line
3. Gate code with `#[cfg(feature = "feature-name")]`
4. Add CI build with `--features feature-name` in `.github/workflows/`
5. Document in this file

## CI note

Adding BoringSSL (`tls-fingerprinting`) requires: `cmake`, `clang`, `perl`, and `go`
on the CI runner. Consider a dedicated runner or Docker image rather than polluting the
default runner environment.
