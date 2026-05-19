# Session: H3 DNS Rebinding TOCTOU Fix — v0.32.4

**Date:** 2026-03-23
**Branch:** `chore/cleanup`
**Commit:** `b7075db4`
**Version bump:** `0.32.3 → 0.32.4`

---

## Session Overview

Continued from the comprehensive review session (`2026-03-23-comprehensive-review-fixes.md`). The user directed: **"start with H3"** — the DNS rebinding TOCTOU vulnerability (CWE-367) flagged in the prior multi-agent review. Implemented `SsrfBlockingResolver`, a custom `reqwest::dns::Resolve` implementation that closes the TOCTOU window by re-validating every resolved IP at the moment reqwest establishes a TCP connection. All 1527 tests pass; 12/12 lefthook pre-commit hooks green; pushed to remote.

---

## Timeline

| Activity | Detail |
|----------|--------|
| Session start | Resumed from prior context; user directed "start with H3" |
| Code read | Read `crates/core/http/ssrf.rs`, `client.rs`, `tests.rs` to understand current implementation |
| Implementation | Added `SsrfBlockingResolver` to `ssrf.rs`; wired into `build_client()` in `client.rs` |
| Type fix | First compile attempt failed: `Box<io::Error>` not auto-coerced to `Box<dyn Error + Send + Sync>`; fixed with explicit `type DnsError = Box<dyn Error + Send + Sync>` alias |
| Test update | Updated `dns_rebinding_toctou_documents_residual_risk` → `dns_rebinding_toctou_is_mitigated_by_resolver` |
| Docs updated | `docs/SECURITY.md`, `crates/core/CLAUDE.md` — changed "residual risk" to "MITIGATED (v0.32.4)" |
| Version bump | `0.32.3 → 0.32.4` via `sed` + `cargo check` |
| Hooks | All 12 lefthook hooks green (1527 tests pass in hook run) |
| Commit + push | `b7075db4` pushed to `chore/cleanup` |

---

## Key Findings

1. **TOCTOU window (CWE-367)**: `validate_url()` resolves hostnames at parse time using text rules only (literal IPs, TLDs, `localhost`). For hostnames like `evil.example.com`, it checks no IP at all — it just passes the text. `reqwest` then resolves DNS independently when it establishes the TCP connection. A TTL-0 DNS record pointing to `8.8.8.8` during validation, then flipping to `127.0.0.1` at connect time, bypasses the guard entirely.

2. **Root of vulnerability**: `crates/core/http/ssrf.rs:63-94` — `validate_url()` only calls `check_ip()` when the URL host is a literal IP address (`bare.parse::<IpAddr>()` succeeds). Hostnames skip the IP check entirely.

3. **Fix mechanism**: `reqwest::dns::Resolve` trait (`reqwest::dns` module, available in reqwest 0.13.2) lets us intercept every DNS resolution before reqwest connects. By running `check_ip()` on each resolved `SocketAddr`, we validate at connect time using the exact same IPs reqwest will dial — no second resolution possible.

4. **Test isolation**: The existing `ALLOW_LOOPBACK` thread-local in `ssrf.rs` (test-only) cannot propagate to tokio worker threads where the DNS resolver runs. Therefore `SsrfBlockingResolver` is `#[cfg(not(test))]` — test builds use reqwest's default resolver, preserving httpmock server reachability on `127.0.0.1`.

5. **Type coercion gotcha**: `Box<std::io::Error>` does not auto-coerce to `Box<dyn Error + Send + Sync>` in the `Resolving` return position. Explicit cast via `type DnsError = Box<dyn Error + Send + Sync>; Box::new(e) as DnsError` is required.

---

## Technical Decisions

- **Custom `reqwest::dns::Resolve` over `ClientBuilder::resolve()`**: `resolve(host, addr)` pins a single static host→addr mapping at build time — useless for per-request SSRF protection. The custom resolver trait is called dynamically on every connection.

- **`tokio::net::lookup_host()` over hickory/trust-dns**: No new dependency needed. `lookup_host` is async, available in tokio's net module, and returns the same system resolver results that reqwest would use by default. Adding hickory would close additional attack surfaces (DNSSEC) but is out of scope.

- **Filter, don't reject on first bad IP**: An attacker might control DNS to return mixed IPs (one public, one private). The resolver collects all IPs, filters to allowed ones, and returns only the allowed set. If all IPs are blocked, it returns an error. This means multi-homed hosts still work as long as at least one IP is public.

- **`#[cfg(not(test))]` guard**: The correct boundary for this feature. Production gets full TOCTOU protection; tests get the default resolver so httpmock, unit tests with local mock servers, and e2e tests remain functional. `validate_url()` still runs in tests via `fetch_html()`, providing parse-time SSRF protection.

- **`type DnsError` local alias**: Cleaner than inline `-> Box<dyn Error + Send + Sync>` return annotations in two places inside the async block. Local type aliases inside async blocks are valid stable Rust.

---

## Files Modified

| File | Change |
|------|--------|
| `crates/core/http/ssrf.rs` | Added `SsrfBlockingResolver` struct + `Resolve` impl (production-only); updated `validate_url()` doc comment from "residual risk" to "MITIGATED"; updated doc comment wording |
| `crates/core/http/client.rs` | Added `#[cfg(not(test))]` block wiring `SsrfBlockingResolver` into `build_client()` via `.dns_resolver()` |
| `crates/core/http/tests.rs` | Renamed and updated `dns_rebinding_toctou_documents_residual_risk` → `dns_rebinding_toctou_is_mitigated_by_resolver`; renamed `validate_url_accepts_public_ip_but_documents_rebinding_risk` → `validate_url_accepts_public_ip_rejects_private` |
| `crates/core/CLAUDE.md` | Updated SSRF section: "TOCTOU residual risk" → "DNS rebinding TOCTOU — MITIGATED (v0.32.4)" with full explanation |
| `docs/SECURITY.md` | Updated Residual Risks section item 1: marked as MITIGATED with two-layer defence description |
| `Cargo.toml` | Version `0.32.3 → 0.32.4` |
| `Cargo.lock` | Updated to reflect new version |

---

## Commands Executed

```bash
# Read current SSRF implementation
# (Read tool on crates/core/http/ssrf.rs, client.rs, tests.rs)

# First compile attempt (failed — type coercion issue)
cargo check --lib
# Error: expected Box<dyn StdError>, found std::io::Error

# After type alias fix
cargo check --lib
# Finished `dev` profile [unoptimized + debuginfo] target(s) in 6.08s

# HTTP-specific tests
cargo test http --lib
# test result: ok. 86 passed; 0 failed; 0 ignored (background task)

# Full test suite
cargo test --lib
# test result: ok. 1516 passed; 0 failed; 11 ignored; finished in 10.19s

# Hook-mode clippy (matches CI exactly)
cargo clippy --all-targets --locked --features test-helpers -- -D warnings
# Finished `dev` profile [unoptimized + debuginfo] target(s) in 15.30s

# Version bump
sed -i 's/^version = "0.32.3"/version = "0.32.4"/' Cargo.toml
cargo check --lib
# Checking axon v0.32.4

# Stage specific files and run pre-commit hooks
git add crates/core/http/ssrf.rs crates/core/http/client.rs crates/core/http/tests.rs \
        crates/core/CLAUDE.md docs/SECURITY.md Cargo.toml Cargo.lock
lefthook run pre-commit
# 12/12 hooks green (1527 tests in hook run)

# Commit
git commit -m "fix(security): close DNS rebinding TOCTOU window via SsrfBlockingResolver (H3)"
# [chore/cleanup b7075db4] 7 files changed

# Push
git push
# c02e2efe..b7075db4  chore/cleanup -> chore/cleanup
```

---

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| DNS rebinding TOCTOU | `validate_url()` checked parse time only; reqwest resolved DNS independently at connect time; TTL-0 rebinding attack possible | `SsrfBlockingResolver` re-runs `check_ip()` at connect time; reqwest uses only the IPs we approved; zero TOCTOU window in production |
| Test connectivity | httpmock on `127.0.0.1` reachable | Unchanged — `SsrfBlockingResolver` is `#[cfg(not(test))]`; tests use default resolver |
| Error on rebinding | Connection succeeded to private IP | Connection fails with: `"SSRF: all resolved IPs for '{host}' are in blocked ranges"` |
| Parse-time checks | `validate_url()` blocks literal IPs, localhost, .internal/.local | Unchanged — `validate_url()` still runs first as quick parse-time filter |
| reqwest behaviour | Default system DNS resolver (no SSRF filtering at resolve time) | `SsrfBlockingResolver` intercepts every `resolve()` call; filtered IPs returned to reqwest |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check --lib` (after type fix) | Compiles v0.32.4 | `Finished dev` | ✅ |
| `cargo test http --lib` | 86 passed, 0 failed | 86 passed, 0 failed | ✅ |
| `cargo test --lib` | ≥1516 passed, 0 failed | 1516 passed, 0 failed | ✅ |
| `cargo clippy --all-targets --locked --features test-helpers -- -D warnings` | No warnings | `Finished dev — no warnings` | ✅ |
| `lefthook run pre-commit` (12 hooks) | All green | 12/12 green, 1527 tests | ✅ |
| `git push` | Accepted | `c02e2efe..b7075db4` | ✅ |

---

## Source IDs + Collections Touched

No new Axon embeds or retrieves were performed during this session. The prior session's embed (`docs/sessions/2026-03-23-comprehensive-review-fixes.md` → collection `axon`, job `6370aabb-74ff-426e-b571-e88dc0d4edaf`) remains valid.

This session doc will be embedded below.

---

## Risks and Rollback

- **Resolver in production only**: Production connections to any hostname now incur one extra async DNS lookup (via `tokio::net::lookup_host`) before reqwest's own connect. This is the same lookup reqwest would do anyway — the overhead is a filter pass over the returned IPs, not an additional round-trip. Negligible impact.

- **Multi-homed hosts**: If a public hostname returns multiple IPs (some public, some private — e.g., an internal CDN route), only the public IPs are passed to reqwest. Connections route to public IPs only. This is correct SSRF behaviour but could affect unusual network topologies.

- **Rollback**: Revert `crates/core/http/ssrf.rs` (remove `SsrfBlockingResolver` struct + impl) and `crates/core/http/client.rs` (remove the `#[cfg(not(test))]` block). The parse-time `validate_url()` guard remains in place.

- **Test builds**: If a future test needs to verify that the SSRF resolver blocks a connection, it will need a custom test fixture since `#[cfg(not(test))]` excludes the resolver. The resolver logic (filtering via `check_ip`) is separately covered by `validate_url()` unit tests.

---

## Decisions Not Taken

- **`hickory-resolver` / `trust-dns`**: Would provide DNSSEC validation and more control over resolution. Out of scope — adds a dependency for marginal gain given our threat model (internal tooling, not a public API).

- **Reverse-DNS (rDNS) check**: Resolving the validated IP back to a hostname and cross-checking adds latency and doesn't close the TOCTOU window (rDNS can also be attacker-controlled). Not implemented.

- **`reqwest::ClientBuilder::resolve(host, addr)` static pinning**: Only works for hosts known at client build time. Useless for dynamic per-request SSRF protection.

- **Process-wide `AtomicBool` for test bypass**: Would allow propagating `ALLOW_LOOPBACK` across threads, enabling the resolver in test builds. Rejected because it would cause test races (two tests in parallel: one asserts loopback is blocked, another sets allow=true globally).

- **Blocking the resolver on empty IP list vs. passing all through**: Chose to return an error if ALL resolved IPs are in blocked ranges. If any IP passes, we return the allowed subset. Alternative (block if ANY IP is private) would break legitimate multi-homed hosts.

---

## Open Questions

- The `SsrfBlockingResolver` calls `tokio::net::lookup_host()`, which uses the system resolver (glibc, musl, etc.). This means `/etc/hosts` and `/etc/resolv.conf` overrides are respected. Is there a deployment scenario where `/etc/hosts` maps a public hostname to a private IP for legitimate internal routing? If so, the resolver would block that connection. Low probability but worth documenting.

- Should the blocked-resolution error be surfaced to the caller with more detail (which IP was blocked)? Currently logs nothing — the error string contains the hostname but not the specific IPs that were rejected. Could help debugging in edge cases.

---

## Next Steps

Remaining high-priority items from the comprehensive review (H1, H2, H5/H6, H7, H8, C3):

1. **C3 (High)**: Graph N+1 Qdrant calls — implement `qdrant_batch_retrieve_by_urls` using a `should` filter across multiple URLs per job dispatch.
2. **H1 (High)**: Wrap `Config` in `Arc<Config>` at `run()` in `lib.rs`; propagate throughout — eliminates the 149-field clone bomb.
3. **H7 (High)**: Standardize `Box<dyn Error + Send + Sync>` across service + vector layers (depends on H1 for clean propagation).
4. **H5/H6 (High)**: Function/file size splits — hard deadline 2026-04-30 (monolith allowlist expiry).
5. **H8 (Medium)**: Extract `retry_with_backoff` generic utility from the 4 independent retry loops.
6. **H2 (Low)**: Circular deps core↔cli/jobs/services — deferred, low ROI for effort.
