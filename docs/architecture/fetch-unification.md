# Web acquisition: one fetch path

Last verified: 2026-07-29

> **Status: partial.** One caller is migrated. A five-agent review on 2026-07-29
> found this document previously overstated the result — see
> [Honest scope](#honest-scope) before relying on any claim here.

## The rule

Everything that pulls bytes off the public web for acquisition goes through
**`axon_core::http::fetch_web`** ([`crates/axon-core/src/http/acquire.rs`](../../crates/axon-core/src/http/acquire.rs)).

Enforced by `cargo xtask check-fetch-divergence`, wired into the lefthook
pre-commit/pre-push hooks and `just verify`. The check fails the build when an
acquisition crate constructs an HTTP client outside the allowlist, **and** when
an allowlist entry goes stale — so this document cannot quietly drift out of
sync with the code.

## Why this exists

`#298` unified the job and ledger model but left acquisition fragmented. Each
surface built its own client with its own user-agent, redirect policy, retry
rules, and (non-)handling of bot walls. That was not a theoretical problem:

On 2026-07-29, seven SC county/city `.gov` roots mapped **zero** URLs. Fixing
the map path recovered all seven (0 → 1,141 URLs). The same fix reached **no
other surface** — `axon scrape` against the same hosts still fetched the
380-byte Akamai `Access Denied` page and dropped it as thin content while
reporting `status: completed`:

```
axon map    https://www.dorchestercountysc.gov/  ->  311 URLs
axon scrape https://www.dorchestercountysc.gov/  ->  documents_prepared=1, chunks_prepared=0
```

One fix, applied once, reached exactly one of eight paths.

## The escalation ladder

`fetch_web` owns the whole sequence, so adding a capability reaches every
surface at once:

1. Fetch with the shared SSRF-guarded client — browser UA, 10-hop redirect cap
   with per-hop SSRF revalidation, connect-time DNS-rebinding guard.
2. Classify the response. A **block-like status** (`401/403/406/429/503`) or a
   body matching a WAF fingerprint (`detect_challenge`) is a *wall*, not
   content — deliberately distinct from an ordinary 404 or timeout.
3. On a wall, and only on a wall, retry through the browser TLS/HTTP2
   impersonating client (feature `tls-fingerprinting`, default OFF).
   The impersonating client revalidates SSRF on **every redirect hop** — see
   [SSRF on the escalation path](#ssrf-on-the-escalation-path).
4. Re-classify. A wall that survives escalation returns `FetchError::Challenge`,
   carrying an `EscalationOutcome` that distinguishes three cases a caller must
   not confuse: `StillWalled` (a real block), `Failed(reason)` (escalation broke
   — retrying is reasonable), and `Unavailable` (built without the feature).
   Collapsing these is how a transient DNS timeout gets reported as a permanent
   bot wall and an operator abandons a working domain.

### SSRF on the escalation path

`wreq::redirect::Policy::limited` is **count-only**, wreq's per-hop check
validates only the URI *scheme*, and wreq's connector skips DNS resolution
entirely for IP-literal hosts ("skip resolving the dns and start connecting
right away") — so the SSRF resolver is never consulted for
`http://169.254.169.254/`. An attacker controlling a site axon was asked to
fetch could answer the escalated request with a redirect to link-local or
loopback space and read the response body.

The impersonating client therefore uses a `Policy::custom` that runs
`validate_url` on every hop, mirroring the shared reqwest client. Its DNS
resolver also records denials through `record_resolver_denial`, and its cookie
jar is per-client rather than a process-wide singleton (wreq does not validate
`Set-Cookie` `Domain=` against the responding host and has no public-suffix
list, so a shared jar lets one host inject cookies onto requests to another).

Escalating only on a wall matters: escalating on any error would fire a second
BoringSSL request for every dead link in a crawl.

### Why status *and* body classification

The Akamai denial page these sites serve carries **no vendor sensor token**, so
`detect_challenge` returns `None` for it — status is the only signal. Other
vendors (Cloudflare, DataDome) serve an HTTP 200 challenge page where the body
is the only signal. Both checks are load-bearing; a regression test pins this
(`akamai_denial_body_has_no_fingerprint_so_status_must_carry_it`).

## Honest scope

What is actually true today, after review:

| Claim | Reality |
|---|---|
| "Web acquisition is unified" | **One** non-test caller: `map/strategy.rs` `discover_root_anchors`. |
| The escalation ladder fixes the Akamai sites | **Only in a build with `--features tls-fingerprinting`.** That feature is in no default set, is not passed by `config/Dockerfile`, and never runs in CI. A stock binary returns `FetchError::Challenge` for those sites. |
| The xtask check enforces unification | It enforces "no *unlisted* acquisition client." It is a source-text scan: blind to a new crate, to `Client::default()`, and to renamed imports. |

Two divergences the first version of this document omitted entirely:

- **20 fetchers take the shared `http_client()` singleton and do their own
  `.get().send()`** with no wall classification — 18 `axon-extract` verticals plus
  `engine/map.rs` (`resolve_map_seed_url`, *inside the map flow this work
  fixed*) and `engine/runtime.rs`. The check originally matched only client
  *construction* and could not see them; it now does, and they are listed in
  `TRACKED_SHARED_CLIENT_FETCHERS`.
- **`ebay.rs` already reimplements a narrower wall check** (`403 | 503`, status
  only, no body fingerprint, no escalation). A second, worse copy of what
  `fetch_web` exists to own — evidence the need is real and that leaving these
  unmigrated invites more copies.

### Open decisions (need an owner's call)

1. **Ship `tls-fingerprinting` on by default?** It adds BoringSSL
   (cmake/clang/perl/go, +8-12 min cold CI) to every build. Until decided, the
   headline fix does not run in production. `EscalationOutcome::Unavailable`
   now says so explicitly in the error rather than degrading silently.
2. **Move `acquire.rs` + `impersonate.rs` to `axon-adapters`?**
   `crates/axon-core/src/CLAUDE.md` forbids "provider clients" and "pipeline
   orchestration" in `axon-core`; a vendor-tuned Chrome TLS profile and a retry
   ladder arguably are both.
3. **Reconcile with `FetchProvider`.** `crates/axon-adapters/src/boundary.rs`
   already defines a fetch seam wired into health, cooldown, and capability
   reporting. `fetch_web` is a free function with none of that — two competing
   abstractions that do not compose.

## Approved exceptions

Every entry below is in `APPROVED_EXCEPTIONS` in
[`xtask/src/checks/fetch_divergence.rs`](../../xtask/src/checks/fetch_divergence.rs).
Two categories:

### Settled — these should not migrate

| Path | Reason |
|---|---|
| `adapters/src/reddit/acquire.rs` | Reddit's API terms **require** a descriptive bot-identifying UA — the opposite of the impersonating ladder. Fixed `oauth.reddit.com` JSON endpoints, not arbitrary hosts. |
| `adapters/src/registry_sources/acquire.rs` | crates.io/npm/PyPI policy requires bot identification. Fixed JSON endpoints; a bot wall is not a failure mode. |
| `adapters/src/providers/searxng_search.rs` | Search-backend API client, not page acquisition. |
| `engine/sitemap/discover.rs`, `sitemap/backfill.rs`, `llms_txt.rs` | Build the client handed to `fetch_text_with_retry`, a byte-capped **streaming** reader (50 MB cap, mid-stream abort). The shared ladder returns a `String` and cannot express that cap. All three now pass `axon_ua()` and inherit the shared redirect/SSRF policy. |

### Tracked — divergence that should be removed (`axon_rust-w612x`)

| Path | Risk today |
|---|---|
| `adapters/src/web_engine/scrape.rs` | **The path that silently captured the Akamai denial page.** No wall handling; only a `(200..300)` status check. |
| `adapters/src/providers/http_fetch.rs` | `FetchProvider` acquire-lane boundary; owns per-request header/proxy config the ladder does not yet expose. |
| `adapters/src/feed/acquire.rs` | Hits arbitrary user-supplied hosts with the bot-identifying UA `"axon-feed"` and no wall handling. A Cloudflare-fronted feed fails the same silent way scrape did. |

## Known divergence NOT covered by this check

Recorded here because the checker cannot see it, not because it is acceptable:

- **The Spider crawl path** builds its HTTP client *inside* the `spider` crate.
  It cannot be routed through `fetch_web` without replacing Spider's fetch
  layer. `detect_challenge` does run in the collector
  (`engine/collector/page.rs:101`), but it only **skips** the page — there is no
  cookie-warmup or impersonated retry, and the header-lookup argument is stubbed
  `|_| None` because Spider's `Page` headers are not threaded through.
- **`chrome_refetch_thin_pages` and `cdp_render` are dead code** — defined,
  re-exported, never called. They carry a third fetch implementation with its
  own UA handling. Delete or wire them; leaving them is how divergence returns.
- **`antibot_cookie_warmup`** (default `true`, env `AXON_CHALLENGE_WARMUP`) is
  defined, defaulted, and parsed, but **never read**. Dead config advertising a
  feature that does not exist.

## Changing this

1. Prefer routing the call through `fetch_web`.
2. If it genuinely cannot be, add an `APPROVED_EXCEPTIONS` entry with a reason a
   reviewer can evaluate — the check rejects entries under 40 characters, and
   "it was easier" is not a reason.
3. Update this document. A stale entry fails the check.
