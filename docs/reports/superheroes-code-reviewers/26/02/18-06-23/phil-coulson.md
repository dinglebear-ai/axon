# Phil Coulson — Security & Testing Review

**Partner:** Natasha Romanoff
**Assigned Issues:** P0-04, P1-06, P1-12, P2-12
**Status:** ALL 4 ISSUES RESOLVED -- cargo check clean, 43 tests passing

---

## P0-04 (Critical): `validate_url()` Not Called in `fetch_html()` -- Main SSRF Gap

**Status:** RESOLVED (was already fixed + hardened with localhost/IPv6 fixes)

### Root Cause Analysis

The original report stated that `fetch_html()` in `crates/core/http.rs` accepted any URL without SSRF protection.

**Finding:** The core issue was already fixed in a prior session. `validate_url(&normalized)?;` is called at `crates/core/http.rs:101`.

**Additional hardening applied:**
1. **localhost bypass fixed** -- Added `localhost` and `.localhost` hostname blocking (was missing from TLD checks)
2. **IPv6 parsing fixed** -- Switched from `host_str().parse::<IpAddr>()` (silently failed for IPv6) to `parsed.host()` typed enum extraction, which correctly handles `[::1]`, `[fc00::1]`, `[fe80::1]`

### Verification -- All Entry Points Checked

| Entry Point | File:Line | Protected? | How |
|------------|-----------|------------|-----|
| `fetch_html()` | `crates/core/http.rs:101` | YES | Direct `validate_url()` call |
| `fetch_text_with_retry()` | `crates/crawl/engine.rs:89` | YES | `validate_url(url).is_err()` check |
| `run_scrape()` | `crates/cli/commands/scrape.rs:20` | YES | Via `fetch_html()` |
| `run_batch()` | `crates/cli/commands/batch.rs:238` | YES | Via `fetch_html()` |
| `run_search()` | `crates/cli/commands/search.rs:25` | YES | Via `fetch_html()` (URL is hardcoded duckduckgo.com) |
| `run_crawl()` | `crates/cli/commands/crawl.rs:237` | YES | Direct `validate_url()` call |
| `run_map()` | `crates/cli/commands/map.rs:11` | YES | Direct `validate_url()` call |
| `embed_path_native()` | `crates/vector/ops.rs:310` | YES | Via `fetch_html()` |
| `batch_jobs worker` | `crates/jobs/batch_jobs.rs:272` | YES | Via `fetch_html()` |
| `doctor` | `crates/cli/commands/doctor.rs:36` | N/A | Infrastructure health checks (internal URLs intentional) |

### Gate Log

- Gate 2: Root cause identified -- issue was pre-fixed, plus two bypasses found (localhost, IPv6)
- Gate 3: Fix implemented -- localhost block added, IPv6 parsing via typed Host enum
- Gate 4: `cargo check` clean, all 21 SSRF tests pass

---

## P1-06 (High): Container Runs as Root -- No `USER` Instruction

**Status:** RESOLVED

### Root Cause Analysis

**File:** `docker/Dockerfile`
**Line:** Entire runtime stage (lines 9-54)

The Dockerfile had no `USER` instruction. `ENTRYPOINT ["/init"]` runs s6-overlay as root, which starts all four workers as root. With arbitrary URL fetching and bind-mounted host directories, this is a privilege escalation risk.

### Fix

**Files modified:**
- `docker/Dockerfile` -- Added `groupadd`/`useradd` for `axon` user (UID 1001), `chown` of writable dirs
- `docker/s6/services.d/crawl-worker/run` -- Added `s6-setuidgid axon`
- `docker/s6/services.d/batch-worker/run` -- Added `s6-setuidgid axon`
- `docker/s6/services.d/embed-worker/run` -- Added `s6-setuidgid axon`
- `docker/s6/services.d/extract-worker/run` -- Added `s6-setuidgid axon`

**Approach:** s6-overlay v3 requires root for `/init` (PID 1) to manage process supervision. Workers drop privileges via `s6-setuidgid axon` in their run scripts. This is the documented best practice for s6-overlay.

```dockerfile
# In Dockerfile:
RUN groupadd -r axon && useradd -r -g axon -u 1001 axon
RUN ... && chown -R axon:axon /var/log/axon /app
```

```bash
# In each service run script:
exec s6-setuidgid axon /usr/local/bin/cortex crawl worker
```

### Gate Log

- Gate 2: Root cause -- no USER instruction, all processes run as UID 0
- Gate 3: Fix implemented -- user created, dirs chown'd, s6-setuidgid in all 4 run scripts
- Gate 4: Docker-only change, no cargo check needed

---

## P1-12 (High): `validate_url` SSRF Protection Untested -- 20+ Test Cases

**Status:** RESOLVED (21 test cases, all passing)

### Root Cause Analysis

**File:** `crates/core/http.rs`
**Function:** `validate_url()` (lines 33-101)

The SSRF defense logic had zero test coverage. Critical: the IPv6 bitmask check and range boundaries were unverified.

### Fix

Added `#[cfg(test)] mod tests` to `crates/core/http.rs` with 21 test cases:

| # | Test Case | Expected | Result |
|---|-----------|----------|--------|
| 1 | `https://example.com/` | ALLOW | PASS |
| 2 | `http://example.com/page` | ALLOW | PASS |
| 3 | `http://127.0.0.1/` | BLOCK | PASS |
| 4 | `http://localhost/` | BLOCK | PASS |
| 5 | `http://[::1]/` | BLOCK | PASS |
| 6 | `http://169.254.169.254/latest/meta-data/` | BLOCK | PASS |
| 7 | `http://169.254.169.253/` | BLOCK | PASS |
| 8 | `http://10.0.0.1/` | BLOCK | PASS |
| 9 | `http://10.255.255.255/` | BLOCK | PASS |
| 10 | `http://172.16.0.1/` | BLOCK | PASS |
| 11 | `http://172.15.255.255/` | ALLOW | PASS |
| 12 | `http://172.32.0.0/` | ALLOW | PASS |
| 13 | `http://192.168.0.1/` | BLOCK | PASS |
| 14 | `ftp://example.com/` | BLOCK | PASS |
| 15 | `file:///etc/passwd` | BLOCK | PASS |
| 16 | `data:text/plain,hello` | BLOCK | PASS |
| 17 | `http://host.internal/` | BLOCK | PASS |
| 18 | `http://host.local/` | BLOCK | PASS |
| 19 | `http://HOST.INTERNAL/` | BLOCK | PASS |
| 20 | Invalid URL string | BLOCK | PASS |
| 21 | `http://[fc00::1]/` | BLOCK | PASS |
| 22 | `http://[fe80::1]/` | BLOCK | PASS |

**Bugs found and fixed by tests:**
1. IPv6 addresses were silently bypassing all checks because `host_str().parse::<IpAddr>()` failed -- fixed by using `parsed.host()` typed enum
2. `localhost` hostname was not blocked -- fixed by adding explicit localhost check

### Verification

```
test result: ok. 43 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

### Gate Log

- Gate 2: Root cause -- zero test coverage for SSRF validation
- Gate 3: 21 tests written, discovered 2 bypasses (IPv6 parsing, localhost), fixed both
- Gate 4: All tests green, `cargo check` clean

---

## P2-12 (Medium): `should_fallback_to_chrome` Untested

**Status:** RESOLVED (8 test cases, all passing)

### Root Cause Analysis

**File:** `crates/crawl/engine.rs`
**Function:** `should_fallback_to_chrome()` (lines 117-128)

Pure function with critical business logic. The 60% thin-page ratio and 10% coverage thresholds determine when HTTP crawl auto-retries with Chrome. Zero tests.

### Fix

Added `#[cfg(test)] mod tests` to `crates/crawl/engine.rs` with 8 boundary-condition tests:

| # | Test Case | Expected | Result |
|---|-----------|----------|--------|
| 1 | No markdown files (0/100) | FALLBACK | PASS |
| 2 | Thin ratio 61/100 (>60%) | FALLBACK | PASS |
| 3 | Thin ratio 60/100 (=60%) | NO FALLBACK | PASS |
| 4 | Low coverage (5/200 < 20) | FALLBACK | PASS |
| 5 | Zero pages_seen (0/0/0) | FALLBACK | PASS |
| 6 | Healthy crawl (10/200 thin, 150 files) | NO FALLBACK | PASS |
| 7 | Low max_pages (8/50 < 10) | FALLBACK | PASS |
| 8 | Small crawl sufficient (15/50 >= 10) | NO FALLBACK | PASS |

### Verification

```
test result: ok. 43 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

### Gate Log

- Gate 2: Root cause -- pure business logic with zero test coverage
- Gate 3: 8 boundary-condition tests written
- Gate 4: All tests green, `cargo check` clean

---

## Discoveries Affecting Other Heroes

1. **P0-04 was already fixed** -- but the fix had 2 bypasses (localhost hostname, IPv6 bracket parsing). Tests revealed and fixed both.
2. **IPv6 SSRF bypass via `host_str().parse::<IpAddr>()`** -- This is a systemic issue. Any code using `Url::host_str()` + `str::parse::<IpAddr>()` will silently fail on IPv6. Use `Url::host()` typed enum instead.
3. **s6-overlay non-root limitation** -- `/init` requires root PID 1. Gwen Stacy (P2-16, P2-17, P2-18) should be aware of this.
4. **`unused_imports` warning in engine.rs** -- `use std::fs::{self, File}` has unused `self` after Gwen's P0-08 async fs migration. Not my issue but noting it.

---

## Summary

| Issue | Priority | Status | Tests Added | Bugs Found |
|-------|----------|--------|-------------|------------|
| P0-04 | Critical | RESOLVED | 0 (verified) | 2 bypasses fixed |
| P1-06 | High | RESOLVED | 0 (Docker) | - |
| P1-12 | High | RESOLVED | 21 | 2 (IPv6, localhost) |
| P2-12 | Medium | RESOLVED | 8 | 0 |
| **Total** | | **4/4 RESOLVED** | **29 tests** | **2 security bugs** |
