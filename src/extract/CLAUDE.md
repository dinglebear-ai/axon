# src/extract — Vertical Extractor Framework
Last Modified: 2026-06-19

Per-site/per-API "vertical" extractors that produce richer, more structured docs than the generic HTML→markdown crawl path. Ships 18 extractors across a match-chain dispatcher. Replaces the legacy webclaw mod.rs dispatcher.

## Module Layout

```
extract/
├── context.rs          # VerticalContext — http client + cfg + cache surface passed to every extractor
├── error.rs            # VerticalError enum
├── registry.rs         # list(), dispatch_by_url(), dispatch_by_name() — match-chain dispatch (no trait objects)
├── types.rs            # ScrapedDoc, ExtractorInfo
├── verticals.rs        # module root re-exporting all verticals/*
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

Two entry points, both in `registry.rs`:

| Function | When it fires | Used by |
|----------|---------------|---------|
| `dispatch_by_url(url, ctx)` | `auto_dispatch: true` extractors only; first matching `matches(url)` wins | `services::scrape::scrape` — called before the generic HTTP path when `cfg.enable_verticals` is true |
| `dispatch_by_name(name, url, ctx)` | Explicit by extractor name — fires `auto_dispatch: false` extractors too | MCP `vertical_scrape` (catalog-only), reserved for future `--vertical <name>` CLI shortcut |

**No trait objects** — plain match-chain. Named-function dispatch is cleaner and faster at this scale.

**Exhaustiveness:** a unit test asserts every `list()` entry has a corresponding arm in `dispatch_by_name()` so the "added to catalog but forgot to wire" bug is impossible.

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

`extractor_name` + `extractor_version` flow through to the Qdrant payload (`payload_schema_version = 8`, see `src/vector/CLAUDE.md` — Payload schema versioning). Bumping `extractor_version` forces points with that extractor name to be re-embedded on next crawl.

## auto_dispatch Flag

`auto_dispatch: false` extractors are excluded from URL-based auto-routing. Use for:

- **ToS-risky** sources (Amazon, eBay) — opt-in only
- **Antibot-gated** sources that should fall through to the generic Chrome path by default
- Extractors that need explicit confirmation before firing

`dispatch_by_url()` skips them entirely. They only fire via `dispatch_by_name()`.

## Integration With `scrape`

The services layer wires this in `src/services/scrape.rs`:

```rust
if cfg.enable_verticals {
    if let Some(result) = dispatch_by_url(&normalized, &ctx).await {
        return result; // vertical claimed the URL
    }
}
// fall through to generic HTTP scrape
```

`cfg.enable_verticals` defaults to `true`. Disable with `AXON_ENABLE_VERTICALS=false` for A/B testing or to force generic-path behavior. This means `scrape` is no longer a pure HTML→markdown transformer — it can return GitHub/PyPI/etc. structured docs transparently when a URL matches.

## Adding a New Extractor

1. Create `verticals/<name>.rs` with three items:
   - `pub const INFO: ExtractorInfo = ExtractorInfo { ... }`
   - `pub fn matches(url: &str) -> bool`
   - `pub async fn extract(url: &str, ctx: &VerticalContext) -> Result<ScrapedDoc, VerticalError>`
2. Add `pub mod <name>;` to `verticals.rs`
3. Add an entry to `list()` in `registry.rs`
4. Add an arm to `dispatch_by_url()` (if `auto_dispatch: true`) AND `dispatch_by_name()`
5. The exhaustiveness test will fail if you skip step 4 for `dispatch_by_name()` — `cargo test extract::registry`

**Ordering matters in `dispatch_by_url()`.** More specific URL patterns must come before less specific ones (e.g. `github_repo` matches 2-segment paths, `github_release` matches 3+ — list the more specific one second, after the broader matcher has had its chance to reject).

## MCP Surface (Discovery-Only)

The MCP `vertical_scrape` action exposes the catalog but does NOT run extraction:

- `subaction=list` → returns `ExtractorInfo[]`
- `subaction=capabilities` → returns metadata for a single extractor
- `subaction=run` → **removed**; redirects to `action=scrape url=<url>` (which auto-routes via `dispatch_by_url`)

See `src/mcp/server/handlers_vertical_scrape.rs` for the redirect message.

## Testing

```bash
cargo test extract                  # all extractor tests
cargo test extract::registry        # dispatch + exhaustiveness tests
cargo test verticals::github_repo   # one extractor's tests
```

Each `verticals/<name>.rs` includes a `matches()` truth-table test. Live HTTP tests are gated behind feature flags or env-driven skips.
