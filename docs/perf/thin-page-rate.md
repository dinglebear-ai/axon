# Historical thin-page rate

Bead: axon_rust-4j1n. Inputs to the decision gate for bead 1jto (JSON data-island walker scope).

## Query

```sql
SELECT
    COUNT(*) AS jobs,
    SUM(CAST(json_extract(result_json,'$.pages_crawled') AS INTEGER)) AS total_pages,
    SUM(CAST(json_extract(result_json,'$.thin_md') AS INTEGER)) AS total_thin,
    ROUND(100.0 * SUM(CAST(json_extract(result_json,'$.thin_md') AS INTEGER)) /
          NULLIF(SUM(CAST(json_extract(result_json,'$.pages_crawled') AS INTEGER)), 0), 2) AS thin_rate_percent
FROM axon_crawl_jobs
WHERE status='completed' AND result_json IS NOT NULL
  AND finished_at > (strftime('%s','now','-30 days') * 1000);
```

Note: `finished_at` is stored as Unix epoch **milliseconds**, so the 30-day filter
multiplies the seconds-based `strftime` output by 1000.

## Result

Database: `~/.axon/jobs.db` (production-side, as of 2026-05-15).

| Metric             | Value      |
| ------------------ | ---------- |
| Completed jobs     | 143        |
| Total pages crawled| 14,691     |
| Total thin pages   | 1,491      |
| **Thin-page rate** | **10.15 %**|

Window: last 30 days of `status='completed'` crawl jobs with non-null `result_json`.

## Decision

10.15 % is **above the 5 % threshold** the bead spec sets for the walker scope
gate.

> If thin-page rate > 5 %: 1jto ships full 5-pattern walker (Contentful + CMS-entry + quote + stat-array + orphan-body)

**Recommendation for axon_rust-1jto: ship the full 5-pattern walker.**

The 1,491 thin pages over 30 days are a real, recurring blast surface — narrowing
to Contentful + CMS-entry only would leave a meaningful fraction of those pages
unrescued, and the marginal LOC cost of the additional three patterns is small
relative to the recovery upside on doc/SPA pages where the main content is
loaded from a JSON island after initial HTML render.
