# Vertical Extractor Metadata Spec

Status: active
Last updated: 2026-05-21

## Problem

Vertical extractors call provider APIs and produce rich `ScrapedDoc` structs with metadata
(stars, license, pipeline_tag, etc.), but this metadata was previously unreachable from Qdrant.
The scrape→embed path wrote only `(url, markdown)` to disk and re-read them, discarding
`ScrapedDoc.structured` and any extra fields entirely.

This spec defines:
- The `extra` field added to `ScrapedDoc` to carry per-extractor metadata
- The per-extractor payload schemas
- The naming conventions that make cross-extractor queries possible
- The Qdrant indexes required to support filtering

---

## Design Decisions

### `extra: Option<serde_json::Value>` on `ScrapedDoc`

`ScrapedDoc` gains an `extra` field (the same type as `PreparedDoc.extra`). The embed pipeline
merges `extra` flat into the Qdrant payload — every key in the object becomes a top-level
payload field. This is identical to how ingest sources populate metadata today.

The `structured` field on `ScrapedDoc` continues to hold the raw API response as
`structured_blob` in the payload. `extra` holds the curated, indexed fields.

### Prefix namespacing

Every extractor-specific field carries a short prefix that uniquely identifies the source.
Universal fields have no prefix. This prevents collisions and makes source identification
unambiguous from the payload alone.

| Extractor | Prefix | Example |
|-----------|--------|---------|
| npm | `npm_` | `npm_name`, `npm_version` |
| pypi | `pypi_` | `pypi_name`, `pypi_version` |
| crates_io | `crate_` | `crate_name`, `crate_version` |
| docs_rs | `docrs_` | `docrs_name`, `docrs_version` |
| docker_hub | `docker_` | `docker_image`, `docker_pulls` |
| huggingface_model | `hf_` | `hf_task`, `hf_downloads` |
| dev_to | `devto_` | `devto_author`, `devto_tags` |
| shopify | `shop_` | `shop_vendor`, `shop_host` |
| hackernews | `hn_` | `hn_id`, `hn_points` |
| stackoverflow | `so_` | `so_question_id`, `so_tags` |
| arxiv | `arxiv_` | `arxiv_id`, `arxiv_authors` |
| amazon | `amz_` | `amz_asin`, `amz_brand` |
| ebay | `ebay_` | `ebay_item_id`, `ebay_condition` |
| github_* verticals | `git_` | uses shared git payload schema |

### Absent beats null

Fields that are not applicable are omitted entirely rather than written as `null`.
A Qdrant equality filter on an absent field returns no results — same behavior as `null` —
without bloating the payload.

### What is indexed

Only fields with real filter use cases get Qdrant keyword/integer indexes.
High-cardinality strings (description, full URL variants), raw counts that are unlikely
to be filtered, and all array fields beyond the most common ones are stored but not indexed.

---

## Per-Extractor Schemas

### GitHub verticals (`github_repo`, `github_issue`, `github_pr`, `github_release`)

Uses the shared git provider payload schema from `src/ingest/git_payload.rs`.
See [`docs/contracts/qdrant-payload-schema.md`](../contracts/qdrant-payload-schema.md) — Git Provider Fields.

```
provider = "github"
git_host = "github.com"
git_owner = "<owner>"
git_repo = "<repo>"
git_content_kind = "repo_metadata" | "issue" | "pr" | "release"
git_state = "open" | "closed" | "merged"   (issue/pr only)
git_number = <u64>                          (issue/pr only)
git_author = "<login>"                      (issue/pr only)
git_labels = [...]                          (issue/pr only)
git_is_draft = <bool>                       (pr only)
git_merged_at = "<ISO8601>"                 (pr only)
git_meta = { stars, forks, language, topics, ... }
```

---

### npm (`pkg_registry = "npm"`)

```
pkg_registry = "npm"
pkg_name     = "<package name>"          # keyword, indexed
pkg_version  = "<latest version>"        # keyword
pkg_language = "javascript"              # keyword, indexed
pkg_license  = "<SPDX>"                  # keyword, indexed
pkg_author   = "<author name>"           # keyword, indexed
pkg_keywords = [...]                     # keyword array
pkg_downloads = <u64>                    # integer (weekly downloads)
pkg_homepage = "<url>"
pkg_repo_url = "<url>"
```

---

### PyPI (`pkg_registry = "pypi"`)

```
pkg_registry = "pypi"
pkg_name     = "<package name>"          # keyword, indexed
pkg_version  = "<latest version>"        # keyword
pkg_language = "python"                  # keyword, indexed
pkg_license  = "<SPDX or raw>"           # keyword, indexed
pkg_author   = "<author name>"           # keyword, indexed
pkg_keywords = [...]                     # keyword array
pkg_downloads = <u64>                    # integer (total)
pkg_homepage = "<url>"
pypi_requires_python = "<version spec>"
```

---

### crates.io (`pkg_registry = "crates_io"`)

```
pkg_registry = "crates_io"
pkg_name     = "<crate name>"            # keyword, indexed
pkg_version  = "<max stable version>"    # keyword
pkg_language = "rust"                    # keyword, indexed
pkg_license  = "<SPDX>"                  # keyword, indexed
pkg_keywords = [...]                     # keyword array
pkg_downloads = <u64>                    # integer (total)
pkg_homepage = "<url>"
pkg_repo_url = "<url>"
crate_msrv   = "<rust version>"
crate_edition = "2021" | "2018" | ...
```

---

### docs.rs (`pkg_registry = "docs_rs"`)

```
pkg_registry = "docs_rs"
pkg_name     = "<crate name>"            # keyword, indexed
pkg_version  = "<version>"              # keyword
pkg_language = "rust"                    # keyword, indexed
docrs_item_count = <u64>                 # integer — number of public items with docs
```

---

### Docker Hub (`extractor_name = "docker_hub"`)

```
docker_namespace  = "library" | "<org>"  # keyword
docker_image      = "<image name>"       # keyword
docker_full_name  = "<namespace>/<image>" # keyword
docker_pulls      = <u64>                # integer
docker_stars      = <u64>                # integer
docker_is_official = <bool>
docker_last_updated = "<ISO8601>"
```

---

### HuggingFace Model (`extractor_name = "huggingface_model"`)

```
hf_model_id  = "<org>/<model>"           # keyword
hf_org       = "<org>"                   # keyword
hf_task      = "<pipeline_tag>"          # keyword, indexed — "text-generation", "image-classification", …
hf_library   = "<library_name>"          # keyword, indexed — "transformers", "diffusers", …
hf_downloads = <u64>                     # integer
hf_likes     = <u64>                     # integer
hf_tags      = [...]                     # keyword array
```

---

### DEV Community (`extractor_name = "dev_to"`)

```
devto_author            = "<username>"   # keyword, indexed
devto_tags              = [...]          # keyword array
devto_reactions         = <u64>          # integer
devto_reading_time_mins = <u64>          # integer
devto_published_at      = "<ISO8601>"
```

---

### Shopify Product (`extractor_name = "shopify"`)

```
shop_host         = "<storefront domain>"  # keyword
shop_vendor       = "<vendor name>"        # keyword
shop_product_type = "<type>"              # keyword
shop_handle       = "<url handle>"         # keyword
```

---

### Hacker News (`extractor_name = "hackernews"`)

```
hn_id            = <u64>                 # integer
hn_type          = "story" | "ask_hn" | "show_hn" | "job"  # keyword, indexed
hn_author        = "<username>"          # keyword, indexed
hn_points        = <u64>                 # integer
hn_comment_count = <u64>                 # integer
hn_created_at    = "<ISO8601>"
hn_external_url  = "<url>"               # the linked URL for story posts
```

HN type inference from item data:
- `item_type == "job"` → `"job"`
- title starts with `"Ask HN:"` → `"ask_hn"`
- title starts with `"Show HN:"` → `"show_hn"`
- otherwise → `"story"`

---

### Stack Overflow (`extractor_name = "stackoverflow"`)

```
so_question_id  = <u64>                  # integer, indexed
so_tags         = [...]                  # keyword array
so_score        = <i64>                  # integer
so_view_count   = <u64>                  # integer
so_is_answered  = "true" | "false"       # keyword (stored as string for indexing)
so_author       = "<display_name>"       # keyword
so_answer_count = <u64>                  # integer
so_created_at   = "<YYYY-MM-DD>"
```

---

### arXiv (`extractor_name = "arxiv"`)

```
arxiv_id          = "<paper id>"          # keyword, indexed — e.g. "2312.00752"
arxiv_authors     = [...]                 # keyword array
arxiv_categories  = [...]                 # keyword array — e.g. ["cs.LG", "stat.ML"]
arxiv_published   = "<ISO8601>"
arxiv_pdf_url     = "<url>"
```

---

### Amazon (`extractor_name = "amazon"`, `auto_dispatch = false`)

```
amz_asin         = "<ASIN>"              # keyword
amz_brand        = "<brand name>"        # keyword
amz_price        = "<price string>"      # keyword (preserves currency)
amz_currency     = "<ISO 4217>"          # keyword
amz_availability = "InStock" | "OutOfStock" | ...  # keyword
amz_rating       = <f64>                 # float
amz_review_count = <u64>                 # integer
```

---

### eBay (`extractor_name = "ebay"`, `auto_dispatch = false`)

```
ebay_item_id     = "<numeric ID>"         # keyword
ebay_brand       = "<brand name>"         # keyword
ebay_price       = "<price string>"       # keyword
ebay_condition   = "New" | "Used" | ...  # keyword
ebay_availability = "InStock" | ...      # keyword
ebay_rating      = <f64>                  # float
ebay_review_count = <u64>                 # integer
```

---

## Qdrant Indexes

Indexes added to `src/vector/ops/tei/qdrant_store/payload_indexes.rs` for vertical fields.
Only fields with filter/facet use cases are indexed; counts and URLs are stored only.

### Keyword indexes (vertical-specific)

```
pkg_registry, pkg_name, pkg_language, pkg_license, pkg_author
hf_task, hf_library
so_is_answered, hn_type, hn_author
arxiv_id, devto_author
```

### Integer indexes (vertical-specific)

```
so_question_id
```

---

## Scrape→Embed Path Fix

Previously the `axon scrape` command wrote markdown to a temp directory and called
`embed_now_with_source`, which re-read the files and created bare `PreparedDoc`s with
`source_type = "scrape"` and no `extra`. The `ScrapedDoc.structured` and any `extra`
fields were discarded.

The fix: `scrape_one()` in `src/cli/commands/scrape.rs` now returns an
`Option<PreparedDoc>` built directly from the `ScrapeResult`, preserving:
- `extractor_name` from the vertical
- `extra` fields from the vertical
- `title` from the vertical

The embed phase calls `embed_prepared_docs` directly rather than going through the
disk-write path.

---

## Implementation Status

| Component | Status |
|-----------|--------|
| `extra` field on `ScrapedDoc` | pending |
| `extra` on `ScrapeResult` | pending |
| Scrape CLI PreparedDoc path | pending |
| GitHub verticals (`git_*`) | pending (foundation in git_payload.rs exists) |
| npm | pending |
| pypi | pending |
| crates_io | pending |
| docs_rs | pending |
| docker_hub | pending |
| huggingface_model | pending |
| dev_to | pending |
| shopify | pending |
| hackernews | pending |
| stackoverflow | pending |
| arxiv | pending |
| amazon | pending |
| ebay | pending |
| Payload indexes | pending |
