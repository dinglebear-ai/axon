# Spider.rs Feature Flags

Last reviewed: 2026-09-29

Spider is an acquisition implementation inside `axon-adapters`. It does not
own Axon source identity, durable jobs, document preparation, or publication.
The former `axon-crawl` crate and its dependency paths are no longer the
source of truth.

## Dependency ownership and verification

| Dependency | Declaration owner | Role |
|---|---|---|
| `spider` | [axon-adapters/Cargo.toml](../../crates/axon-adapters/Cargo.toml) | HTTP/Chrome acquisition, crawl control, and supported output features |
| `spider_transformations` | [axon-adapters/Cargo.toml](../../crates/axon-adapters/Cargo.toml) | Content transformation used by acquisition |
| `spider_agent` | [axon-adapters/Cargo.toml](../../crates/axon-adapters/Cargo.toml) and [axon-services/Cargo.toml](../../crates/axon-services/Cargo.toml) | Search client and service integrations |

The adapter declares `spider` with `default-features = false` and an explicit
feature list, including `chrome_intercept`, `etag_cache`, and `warc`. The
adapter's `spider_agent` declaration enables `search_tavily`; the services
declaration also enables `openai`. Those are dependency features, not Axon
command-line switches or a promise that a configured provider is available.

The complete declared list belongs in the manifests above. Resolved versions
belong in [Cargo.lock](../../Cargo.lock). Inspect the graph for the exact
build target and feature selection rather than copying an upstream feature
count from an older Spider release:

```bash
cargo tree --locked -p axon-adapters -e features -i spider
cargo tree --locked -p axon-services -e features -i spider_agent
cargo metadata --locked --format-version 1
```

`basic` and other dependency features can enable additional features
transitively, and dependencies can share enabled features in a build graph.
An explicit declaration, a compiled feature, and an active runtime behavior
are three different facts. Inspect the locked dependency sources and Axon
call sites before claiming that a feature is unused, harmless, or enabled on
every path. The old inventory of 89 upstream flags and its Spider 2.52.0
reachability claims are not maintained contracts for the current lockfile.

## Behavior wired by Axon

| Area | Axon implementation | Important boundary |
|---|---|---|
| HTTP/Chrome request setup | [engine/runtime.rs](../../crates/axon-adapters/src/web_engine/engine/runtime.rs) | Applies configured limits, request identity, headers, render behavior, retries, and hedging |
| Adaptive crawl concurrency | [engine/adaptive.rs](../../crates/axon-adapters/src/web_engine/engine/adaptive.rs) | Runtime opt-in through effective adaptive-concurrency configuration, not merely a compiled dependency feature |
| Browser access | [browser.rs](../../crates/axon-adapters/src/web_engine/browser.rs) | Browser availability and CDP configuration remain runtime prerequisites |
| Screenshots | [screenshot.rs](../../crates/axon-adapters/src/web_engine/screenshot.rs) | Standalone screenshot behavior must be checked separately from crawl configuration |
| Conditional cache bookkeeping | [engine/etag.rs](../../crates/axon-adapters/src/web_engine/engine/etag.rs) | Helpers and sidecars do not establish safe active conditional reuse |
| WARC output | [engine/runtime.rs](../../crates/axon-adapters/src/web_engine/engine/runtime.rs) | Configured crawl output, not a new indexing or publication authority |
| URL safety | [axon-core HTTP policy](../../crates/axon-core/src/http/ssrf.rs) | Axon owns SSRF enforcement independently of Spider feature names |

### Conditional ETag reuse is disabled

The current runtime explicitly warns when `etag_conditional` is requested:
conditional reuse is disabled because the crawler does not expose an explicit
304 outcome that can be distinguished safely from a failed fetch that emitted
no page. Do not claim that `--etag-conditional` currently skips unchanged
response bodies or guarantees faster recrawls. Compiling `etag_cache` and
retaining `etag.json` helpers do not change this limitation.

An absent page cannot be restored as a successful unchanged page merely
because its URL was visited. Source refresh still relies on the ledger
manifest, stable hashes, complete/partial inventory semantics, and committed
generations. See [source pipeline](../architecture/source-pipeline.md).

### Hedging, caching, and control

Request setup installs Spider's `HedgeConfig::default()`. Hedging can issue an
additional request for a slow response, so capacity planning must not assume
one upstream HTTP request per page. Timing and cancellation details are
properties of the locked implementation, not a fixed latency guarantee.

The `cache` and `cache_http_only` runtime settings control the caching calls
in request setup. They are separate from the disabled ETag reuse behavior.
Spider control supports in-process crawl interruption, including the memory
guard; durable job cancellation remains a scheduler responsibility. A
control signal or partial output is not proof that the source job published.

### Browser and optional features

A build with Chrome-related features still needs the configured browser/CDP
provider for the chosen operation. Do not infer a deployment's browser address
or local-launch behavior from a feature list. Remote engine-specific policies
may not be supported by a generic CDP endpoint; use the operation's actual
diagnostics and [configuration reference](../guides/configuration.md).

`glob` and `firewall` are not explicitly enabled by Axon's Spider declaration.
Adding dependency features requires a locked-graph review and tests for the
web acquisition paths, not simply a row change in this page. Do not substitute
a dependency feature for Axon's authorization or SSRF checks.

## Axon features versus dependency features

[Axon feature flags](cargo-features.md) covers the root package's features and
runtime gates. Axon does contain its own conditional compilation, including
test and platform gates; the former claim that all conditional compilation
lives inside Spider was incorrect. Root placeholder features do not activate
a dependency merely because their names sound related.

When updating Spider, review HTTP and Chrome acquisition, failure/cancellation,
limits, headers, cache/304 semantics, WARC output, and refresh/removal behavior
against the matching build. Update manifests and the lockfile intentionally,
run affected tests, then refresh generated contracts. Preserve dated benchmark
results as historical evidence instead of re-labeling them as current behavior.
