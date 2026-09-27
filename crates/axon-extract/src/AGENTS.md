# src/extract — Vertical Extractor Framework
Last Modified: 2026-06-19

Per-site/per-API "vertical" extractors that produce richer, more structured docs than generic HTML-to-markdown web acquisition. Ships 18 extractors, each a plain module. **This crate owns only the extractor implementations + narrow shared types** — URL/name matching order and dispatch policy live in `axon-adapters::vertical_registry` (see Dispatch Model below), so this implementation crate has no pipeline ownership.

## Module Layout

```
extract/
├── lib.rs              # public API root (re-exports VerticalContext, VerticalError, ExtractorInfo, ScrapedDoc)
├── context.rs          # VerticalContext — narrowed ServiceContext view passed to every extractor
├── error.rs            # VerticalError enum
├── git_payload.rs      # git-source structured payload shaping
├── types.rs            # ScrapedDoc, ExtractorInfo
├── verticals.rs        # declares all vertical sub-modules (`pub mod <name>;`)
└── verticals/          # one file per extractor
    ├── amazon.rs            # auto_dispatch: false (ToS-risky)
    ├── arxiv.rs
    ├── crates_io.rs
    ├── dev_to.rs
    ├── docker_hub.rs
    ├── docs_rs.rs
    ├── ebay.rs              # auto_dispatch: false (ToS-risky)
    ├── github_issue.rs
    ├── github_pr.rs
    ├── github_release.rs
    ├── github_repo.rs
    ├── hackernews.rs
    ├── huggingface_model.rs
    ├── npm.rs
    ├── pypi.rs
    ├── reddit.rs
    ├── shopify.rs
    └── stackoverflow.rs
```

## Dispatch Model

Dispatch is **not in this crate** — it lives in `crates/axon-adapters/src/vertical_registry.rs`, which composes the per-module `INFO`/`matches()`/`extract()` functions. Two entry points there:

| Function (in `axon-adapters::vertical_registry`) | When it fires | Used by |
|----------|---------------|---------|
| `dispatch_by_url(url, ctx)` | `auto_dispatch: true` extractors only; first matching `matches(url)` wins | the web/git acquisition adapters (`axon-adapters::web::vertical` / `git::vertical`) when `cfg.enable_verticals` is true, before the generic HTTP path |
| `dispatch_by_name(name, url, ctx)` | Explicit by extractor name — fires `auto_dispatch: false` extractors too | reserved for explicit by-name acquisition |

**No trait objects** — plain match-chain. Named-function dispatch is cleaner and faster at this scale.

**Exhaustiveness:** a unit test in `axon-adapters` asserts every `list()` entry has a corresponding arm in `dispatch_by_name()` so the "added to catalog but forgot to wire" bug is impossible.

## ScrapedDoc Output

```rust
pub struct ScrapedDoc {
    pub url: String,
    pub markdown: String,
    pub title: Option<String>,
    pub extractor_name: &'static str,    // → Qdrant payload, enables retrieval filters
    pub extractor_version: u32,          // bump triggers reindex on upgrade
    pub structured: Option<serde_json::Value>,
}
```

`extractor_name` + `extractor_version` flow through to the Qdrant payload (governed by the vector payload contract `payload_contract_version`, currently `"2026-07-01"` in `axon-api::reset`; see `crates/axon-vectors/src/CLAUDE.md`). Bumping `extractor_version` forces points with that extractor name to be re-embedded on the next source refresh.

## auto_dispatch Flag

`auto_dispatch: false` extractors are excluded from URL-based auto-routing. Use for:

- **ToS-risky** sources (Amazon, eBay) — opt-in only
- **Antibot-gated** sources that should fall through to the generic Chrome path by default
- Extractors that need explicit confirmation before firing

`dispatch_by_url()` skips them entirely. They only fire via `dispatch_by_name()`.

## Integration With acquisition

Vertical dispatch is wired into the acquisition adapters, not this crate: `axon-adapters::web::vertical` and `git::vertical` call `vertical_registry::dispatch_by_url(...)` before the generic HTTP path when `cfg.enable_verticals` is true; if a vertical claims the URL its `ScrapedDoc` is returned instead. (`axon-services::scrape` reads `cfg.enable_verticals` but the dispatch itself lives in the adapters.)

`cfg.enable_verticals` defaults to `true`. Disable with `AXON_ENABLE_VERTICALS=false` for A/B testing or to force generic-path behavior. This means acquisition is no longer a pure HTML→markdown transformer — it can return GitHub/PyPI/etc. structured docs transparently when a URL matches.

## Adding a New Extractor

In **this crate** (`axon-extract`):
1. Create `verticals/<name>.rs` with three items:
   - `pub const INFO: ExtractorInfo = ExtractorInfo { ... }`
   - `pub fn matches(url: &str) -> bool`
   - `pub async fn extract(url: &str, ctx: &VerticalContext) -> Result<ScrapedDoc, VerticalError>`
2. Add `pub mod <name>;` to `verticals.rs`

Then in **`axon-adapters`** (`src/vertical_registry.rs`):
3. Add an entry to `list()`
4. Add an arm to `dispatch_by_url()` (if `auto_dispatch: true`) AND `dispatch_by_name()`
5. The exhaustiveness test (in `axon-adapters`) fails if you skip step 4 for `dispatch_by_name()`

**Ordering matters in `dispatch_by_url()`.** More specific URL patterns must come before less specific ones (e.g. `github_repo` matches 2-segment paths, `github_release` matches 3+ — list the more specific one second, after the broader matcher has had its chance to reject).

## Testing

```bash
cargo test -p axon-extract              # all extractor + matches() tests
cargo test -p axon-adapters vertical    # dispatch + exhaustiveness tests (registry lives in adapters)
cargo test verticals::github_repo       # one extractor's tests
```

Each `verticals/<name>.rs` includes a `matches()` truth-table test. Live HTTP tests are gated behind feature flags or env-driven skips.
