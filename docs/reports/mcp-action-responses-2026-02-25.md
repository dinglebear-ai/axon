# MCP Action Responses (Raw)

Generated: 2026-02-25
Server: `axon` via `mcporter --config config/mcporter.json`

Notes:
- Each section contains the exact command executed and raw stdout/stderr.
- Outputs are captured verbatim from command execution.


## status

```bash
mcporter --config config/mcporter.json call axon.axon action:status --output json
```

```text
{
  "ok": true,
  "action": "status",
  "subaction": "run",
  "data": {
    "json": {
      "local_crawl_jobs": [
        {
          "created_at": "2026-02-25T02:38:02.301698Z",
          "error_text": null,
          "finished_at": "2026-02-25T02:38:35.913176Z",
          "id": "6906b63a-74b6-43ed-9cfa-5f18aae33971",
          "result_json": {
            "audit_diff": {
              "added_count": 173,
              "cache_hit": false,
              "cache_source": null,
              "current_count": 173,
              "previous_count": 0,
              "removed_count": 0,
              "start_url": "https://platejs.org/",
              "unchanged_count": 0
            },
            "audit_report_path": ".cache/axon-rust/output/domains/platejs.org/6906b63a-74b6-43ed-9cfa-5f18aae33971/audit/platejs-org-diff-report.json",
            "cache_hit": false,
            "cache_skip_browser": false,
            "crawl_stream_pages": 186,
            "elapsed_ms": 29508,
            "extraction_prompt": null,
            "filtered_urls": 13,
            "md_created": 173,
            "output_dir": ".cache/axon-rust/output/domains/platejs.org/6906b63a-74b6-43ed-9cfa-5f18aae33971",
            "pages_crawled": 186,
            "pages_discovered": 186,
            "phase": "completed",
            "robots_candidates": 0,
            "robots_declared_sitemaps": 0,
            "robots_discovered_urls": 0,
            "robots_failed": 0,
            "robots_filtered_existing": 0,
            "robots_sitemap_docs_parsed": 0,
            "robots_written": 0,
            "stale_urls_deleted": 86,
            "thin_md": 13
          },
          "started_at": "2026-02-25T02:38:02.352526Z",
          "status": "completed",
          "updated_at": "2026-02-25T02:38:35.913176Z",
          "url": "https://platejs.org/"
        },
        {
          "created_at": "2026-02-25T02:34:56.805545Z",
          "error_text": "watchdog reclaimed stale running crawl job (idle=392s marker=lane=1)",
          "finished_at": "2026-02-25T04:42:00.861585Z",
          "id": "8fe25fd2-88dc-472b-88a6-5b8b7c8e8faa",
          "result_json": {
            "_watchdog": {
              "first_seen_stale_at": "2026-02-25T04:40:30.878840626+00:00",
              "observed_updated_at": "2026-02-25T04:35:28.230324+00:00"
            },
            "crawl_stream_pages": 263455,
            "filtered_urls": 669,
            "md_created": 262786,
            "pages_crawled": 263455,
            "pages_discovered": 0,
            "phase": "crawling",
            "thin_md": 669
          },
          "started_at": "2026-02-25T02:34:56.849478Z",
          "status": "failed",
          "updated_at": "2026-02-25T04:42:00.861585Z",
          "url": "https://tailscale.com/docs"
        },
        {
          "created_at": "2026-02-25T00:21:25.446348Z",
          "error_text": null,
          "finished_at": "2026-02-25T00:22:06.254421Z",
          "id": "b126da5d-553c-4ac7-9a5f-c71062a86138",
          "result_json": null,
          "started_at": null,
          "status": "canceled",
          "updated_at": "2026-02-25T00:22:06.254421Z",
          "url": "https://gofastmcp.com/"
        },
        {
          "created_at": "2026-02-25T00:21:10.195894Z",
          "error_text": null,
          "finished_at": "2026-02-25T00:21:18.978038Z",
          "id": "03cbb6f9-76e8-47ec-87fa-32df3e6e8fc1",
          "result_json": {
            "audit_diff": {
              "added_count": 0,
              "cache_hit": false,
              "cache_source": null,
              "current_count": 0,
              "previous_count": 0,
              "removed_count": 0,
              "start_url": "https://gofastmco.com/",
              "unchanged_count": 0
            },
            "audit_report_path": ".cache/axon-rust/output/domains/gofastmco.com/03cbb6f9-76e8-47ec-87fa-32df3e6e8fc1/audit/gofastmco-com-diff-report.json",
            "cache_hit": false,
            "cache_skip_browser": false,
            "crawl_stream_pages": 1,
            "elapsed_ms": 2506,
            "extraction_prompt": null,
            "filtered_urls": 1,
            "md_created": 0,
            "output_dir": ".cache/axon-rust/output/domains/gofastmco.com/03cbb6f9-76e8-47ec-87fa-32df3e6e8fc1",
            "pages_crawled": 1,
            "pages_discovered": 1,
            "phase": "completed",
            "robots_candidates": 0,
            "robots_declared_sitemaps": 0,
            "robots_discovered_urls": 0,
            "robots_failed": 0,
            "robots_filtered_existing": 0,
            "robots_sitemap_docs_parsed": 0,
            "robots_written": 0,
            "stale_urls_deleted": 0,
            "thin_md": 1
          },
          "started_at": "2026-02-25T00:21:10.232779Z",
          "status": "completed",
          "updated_at": "2026-02-25T00:21:18.978038Z",
          "url": "https://gofastmco.com/"
        },
        {
          "created_at": "2026-02-25T00:19:58.647388Z",
          "error_text": null,
          "finished_at": "2026-02-25T00:23:38.819607Z",
          "id": "a103cad5-fbc3-42b0-9ea0-da98ca796891",
          "result_json": {
            "audit_diff": {
              "added_count": 1070,
              "cache_hit": false,
              "cache_source": null,
              "current_count": 1070,
              "previous_count": 0,
              "removed_count": 0,
              "start_url": "https://code.claude.com/",
              "unchanged_count": 0
            },
            "audit_report_path": ".cache/axon-rust/output/domains/code.claude.com/a103cad5-fbc3-42b0-9ea0-da98ca796891/audit/code-claude-com-diff-report.json",
            "cache_hit": false,
            "cache_skip_browser": false,
            "crawl_stream_pages": 1390,
            "elapsed_ms": 218321,
            "extraction_prompt": null,
            "filtered_urls": 320,
            "md_created": 1070,
            "output_dir": ".cache/axon-rust/output/domains/code.claude.com/a103cad5-fbc3-42b0-9ea0-da98ca796891",
            "pages_crawled": 1390,
            "pages_discovered": 1390,
            "phase": "completed",
            "robots_candidates": 0,
            "robots_declared_sitemaps": 0,
            "robots_discovered_urls": 0,
            "robots_failed": 0,
            "robots_filtered_existing": 0,
            "robots_sitemap_docs_parsed": 3,
            "robots_written": 0,
            "stale_urls_deleted": 0,
            "thin_md": 320
          },
          "started_at": "2026-02-25T00:19:58.686544Z",
          "status": "completed",
          "updated_at": "2026-02-25T00:23:38.819607Z",
          "url": "https://code.claude.com/"
        },
        {
          "created_at": "2026-02-25T00:18:19.679179Z",
          "error_text": null,
          "finished_at": "2026-02-25T00:18:19.742592Z",
          "id": "ad4a7a48-0c15-4268-a951-347805700249",
          "result_json": {
            "audit_diff": {
              "added_count": 0,
              "cache_hit": true,
              "cache_source": "job:de8f10d4-9984-4527-a59d-aa83f83840fb manifest:.cache/axon-rust/output/domains/gofastmcp.com/de8f10d4-9984-4527-a59d-aa83f83840fb/manifest.jsonl",
              "current_count": 390,
              "previous_count": 390,
              "removed_count": 0,
              "start_url": "https://gofastmcp.com/",
              "unchanged_count": 390
            },
            "audit_report_path": ".cache/axon-rust/output/domains/gofastmcp.com/ad4a7a48-0c15-4268-a951-347805700249/audit/gofastmcp-com-diff-report.json",
            "cache_hit": true,
            "cache_skip_browser": false,
            "crawl_stream_pages": 0,
            "elapsed_ms": 0,
            "filtered_urls": 0,
            "md_created": 390,
            "output_dir": ".cache/axon-rust/output/domains/gofastmcp.com/ad4a7a48-0c15-4268-a951-347805700249",
            "pages_crawled": 0,
            "pages_discovered": 390,
            "phase": "completed",
            "thin_md": 0
          },
          "started_at": "2026-02-25T00:18:19.728168Z",
          "status": "completed",
          "updated_at": "2026-02-25T00:18:19.742592Z",
          "url": "https://gofastmcp.com/"
        },
        {
          "created_at": "2026-02-25T00:04:43.908096Z",
          "error_text": null,
          "finished_at": "2026-02-25T00:04:49.388417Z",
          "id": "28b83610-cd84-4575-91c2-68799c6bfdb3",
          "result_json": {
            "audit_diff": {
              "added_count": 0,
              "cache_hit": false,
              "cache_source": null,
              "current_count": 0,
              "previous_count": 0,
              "removed_count": 0,
              "start_url": "https://example.com/",
              "unchanged_count": 0
            },
            "audit_report_path": ".cache/axon-rust/output/domains/example.com/28b83610-cd84-4575-91c2-68799c6bfdb3/audit/example-com-diff-report.json",
            "cache_hit": false,
            "cache_skip_browser": false,
            "crawl_stream_pages": 1,
            "elapsed_ms": 1445,
            "extraction_prompt": null,
            "filtered_urls": 1,
            "md_created": 0,
            "output_dir": ".cache/axon-rust/output/domains/example.com/28b83610-cd84-4575-91c2-68799c6bfdb3",
            "pages_crawled": 1,
            "pages_discovered": 1,
            "phase": "completed",
            "robots_candidates": 0,
            "robots_declared_sitemaps": 0,
            "robots_discovered_urls": 0,
            "robots_failed": 0,
            "robots_filtered_existing": 0,
            "robots_sitemap_docs_parsed": 0,
            "robots_written": 0,
            "stale_urls_deleted": 0,
            "thin_md": 1
          },
          "started_at": "2026-02-25T00:04:43.952632Z",
          "status": "completed",
          "updated_at": "2026-02-25T00:04:49.388417Z",
          "url": "https://example.com/"
        },
        {
          "created_at": "2026-02-24T21:27:31.136695Z",
          "error_text": null,
          "finished_at": "2026-02-24T21:28:34.647363Z",
          "id": "d7c1d3ea-5722-4ed6-a485-ade088103679",
          "result_json": {
            "audit_diff": {
              "added_count": 2040,
              "cache_hit": false,
              "cache_source": null,
              "current_count": 2040,
              "previous_count": 0,
              "removed_count": 0,
              "start_url": "https://zed.dev/",
              "unchanged_count": 0
            },
            "audit_report_path": ".cache/axon-rust/output/domains/zed.dev/d7c1d3ea-5722-4ed6-a485-ade088103679/audit/zed-dev-diff-report.json",
            "cache_hit": false,
            "cache_skip_browser": false,
            "crawl_stream_pages": 2107,
            "elapsed_ms": 28935,
            "extraction_prompt": null,
            "filtered_urls": 182,
            "md_created": 2040,
            "output_dir": ".cache/axon-rust/output/domains/zed.dev/d7c1d3ea-5722-4ed6-a485-ade088103679",
            "pages_crawled": 2107,
            "pages_discovered": 2222,
            "phase": "completed",
            "robots_candidates": 115,
            "robots_declared_sitemaps": 1,
            "robots_discovered_urls": 1320,
            "robots_failed": 0,
            "robots_filtered_existing": 1205,
            "robots_sitemap_docs_parsed": 1,
            "robots_written": 101,
            "stale_urls_deleted": 29,
            "thin_md": 182
          },
          "started_at": "2026-02-24T21:27:31.178880Z",
          "status": "completed",
          "updated_at": "2026-02-24T21:28:34.647363Z",
          "url": "https://zed.dev/"
        },
        {
          "created_at": "2026-02-24T20:58:33.844982Z",
          "error_text": null,
          "finished_at": "2026-02-24T20:58:37.082504Z",
          "id": "fe1a2756-401e-4c8d-b0c1-2d01157a6296",
          "result_json": {
            "audit_diff": {
              "added_count": 109,
              "cache_hit": false,
              "cache_source": null,
              "current_count": 109,
              "previous_count": 0,
              "removed_count": 0,
              "start_url": "https://geminicli.com/",
              "unchanged_count": 0
            },
            "audit_report_path": ".cache/axon-rust/output/domains/geminicli.com/fe1a2756-401e-4c8d-b0c1-2d01157a6296/audit/geminicli-com-diff-report.json",
            "cache_hit": false,
            "cache_skip_browser": false,
            "crawl_stream_pages": 103,
            "elapsed_ms": 232,
            "extraction_prompt": null,
            "filtered_urls": 1,
            "md_created": 109,
            "output_dir": ".cache/axon-rust/output/domains/geminicli.com/fe1a2756-401e-4c8d-b0c1-2d01157a6296",
            "pages_crawled": 103,
            "pages_discovered": 110,
            "phase": "completed",
            "robots_candidates": 7,
            "robots_declared_sitemaps": 0,
            "robots_discovered_urls": 91,
            "robots_failed": 0,
            "robots_filtered_existing": 84,
            "robots_sitemap_docs_parsed": 2,
            "robots_written": 7,
            "stale_urls_deleted": 0,
            "thin_md": 1
          },
          "started_at": "2026-02-24T20:58:33.880973Z",
          "status": "completed",
          "updated_at": "2026-02-24T20:58:37.082504Z",
          "url": "https://geminicli.com/"
        },
        {
          "created_at": "2026-02-24T20:57:01.323627Z",
          "error_text": null,
          "finished_at": "2026-02-24T20:57:01.389873Z",
          "id": "ec6d2991-480d-4b00-a4c7-e407a0d10cd7",
          "result_json": {
            "audit_diff": {
              "added_count": 0,
              "cache_hit": true,
              "cache_source": "job:70118f8a-0005-4ec1-a7a8-5e11ed072b5a manifest:.cache/axon-rust/output/domains/geminicli.com/70118f8a-0005-4ec1-a7a8-5e11ed072b5a/manifest.jsonl",
              "current_count": 109,
              "previous_count": 109,
              "removed_count": 0,
              "start_url": "https://geminicli.com/",
              "unchanged_count": 109
            },
            "audit_report_path": ".cache/axon-rust/output/domains/geminicli.com/ec6d2991-480d-4b00-a4c7-e407a0d10cd7/audit/geminicli-com-diff-report.json",
            "cache_hit": true,
            "cache_skip_browser": false,
            "crawl_stream_pages": 0,
            "elapsed_ms": 0,
            "filtered_urls": 0,
            "md_created": 109,
            "output_dir": ".cache/axon-rust/output/domains/geminicli.com/ec6d2991-480d-4b00-a4c7-e407a0d10cd7",
            "pages_crawled": 0,
            "pages_discovered": 109,
            "phase": "completed",
            "thin_md": 0
          },
          "started_at": "2026-02-24T20:57:01.370328Z",
          "status": "completed",
          "updated_at": "2026-02-24T20:57:01.389873Z",
          "url": "https://geminicli.com/"
        },
        {
          "created_at": "2026-02-24T20:55:25.272782Z",
          "error_text": null,
          "finished_at": "2026-02-24T20:55:31.193441Z",
          "id": "70118f8a-0005-4ec1-a7a8-5e11ed072b5a",
          "result_json": {
            "audit_diff": {
              "added_count": 109,
              "cache_hit": false,
              "cache_source": null,
              "current_count": 109,
              "previous_count": 0,
              "removed_count": 0,
              "start_url": "https://geminicli.com/",
              "unchanged_count": 0
            },
            "audit_report_path": ".cache/axon-rust/output/domains/geminicli.com/70118f8a-0005-4ec1-a7a8-5e11ed072b5a/audit/geminicli-com-diff-report.json",
            "cache_hit": false,
            "cache_skip_browser": false,
            "crawl_stream_pages": 103,
            "elapsed_ms": 1193,
            "extraction_prompt": null,
            "filtered_urls": 1,
            "md_created": 109,
            "output_dir": ".cache/axon-rust/output/domains/geminicli.com/70118f8a-0005-4ec1-a7a8-5e11ed072b5a",
            "pages_crawled": 103,
            "pages_discovered": 110,
            "phase": "completed",
            "robots_candidates": 7,
            "robots_declared_sitemaps": 0,
            "robots_discovered_urls": 91,
            "robots_failed": 0,
            "robots_filtered_existing": 84,
            "robots_sitemap_docs_parsed": 2,
            "robots_written": 7,
            "stale_urls_deleted": 1,
            "thin_md": 1
          },
          "started_at": "2026-02-24T20:55:25.330758Z",
          "status": "completed",
          "updated_at": "2026-02-24T20:55:31.193441Z",
          "url": "https://geminicli.com/"
        },
        {
          "created_at": "2026-02-24T20:26:13.541142Z",
          "error_text": null,
          "finished_at": "2026-02-24T20:26:32.925568Z",
          "id": "2449918e-ea45-4a6f-aab1-5ee3feea5b8a",
          "result_json": {
            "audit_diff": {
              "added_count": 213,
              "cache_hit": false,
              "cache_source": null,
              "current_count": 213,
              "previous_count": 0,
              "removed_count": 0,
              "start_url": "https://modelcontextprotocol.io/",
              "unchanged_count": 0
            },
            "audit_report_path": ".cache/axon-rust/output/domains/modelcontextprotocol.io/2449918e-ea45-4a6f-aab1-5ee3feea5b8a/audit/modelcontextprotocol-io-diff-report.json",
            "cache_hit": false,
            "cache_skip_browser": false,
            "crawl_stream_pages": 213,
            "elapsed_ms": 14389,
            "extraction_prompt": null,
            "filtered_urls": 0,
            "md_created": 213,
            "output_dir": ".cache/axon-rust/output/domains/modelcontextprotocol.io/2449918e-ea45-4a6f-aab1-5ee3feea5b8a",
            "pages_crawled": 213,
            "pages_discovered": 213,
            "phase": "completed",
            "robots_candidates": 0,
            "robots_declared_sitemaps": 1,
            "robots_discovered_urls": 168,
            "robots_failed": 0,
            "robots_filtered_existing": 168,
            "robots_sitemap_docs_parsed": 1,
            "robots_written": 0,
            "stale_urls_deleted": 0,
            "thin_md": 0
          },
          "started_at": "2026-02-24T20:26:13.609785Z",
          "status": "completed",
          "updated_at": "2026-02-24T20:26:32.925568Z",
          "url": "https://modelcontextprotocol.io/"
        },
        {
          "created_at": "2026-02-24T19:48:16.012633Z",
          "error_text": null,
          "finished_at": "2026-02-24T19:49:09.579096Z",
          "id": "de8f10d4-9984-4527-a59d-aa83f83840fb",
          "result_json": {
            "audit_diff": {
              "added_count": 390,
              "cache_hit": false,
              "cache_source": null,
              "current_count": 390,
              "previous_count": 0,
              "removed_count": 0,
              "start_url": "https://gofastmcp.com/",
              "unchanged_count": 0
            },
            "audit_report_path": ".cache/axon-rust/output/domains/gofastmcp.com/de8f10d4-9984-4527-a59d-aa83f83840fb/audit/gofastmcp-com-diff-report.json",
            "cache_hit": false,
            "cache_skip_browser": false,
            "crawl_stream_pages": 105,
            "elapsed_ms": 2679,
            "extraction_prompt": null,
            "filtered_urls": 0,
            "md_created": 390,
            "output_dir": ".cache/axon-rust/output/domains/gofastmcp.com/de8f10d4-9984-4527-a59d-aa83f83840fb",
            "pages_crawled": 105,
            "pages_discovered": 390,
            "phase": "completed",
            "robots_candidates": 285,
            "robots_declared_sitemaps": 1,
            "robots_discovered_urls": 386,
            "robots_failed": 0,
            "robots_filtered_existing": 101,
            "robots_sitemap_docs_parsed": 1,
            "robots_written": 285,
            "stale_urls_deleted": 0,
            "thin_md": 0
          },
          "started_at": "2026-02-24T19:48:16.054201Z",
          "status": "completed",
          "updated_at": "2026-02-24T19:49:09.579096Z",
          "url": "https://gofastmcp.com/"
        },
        {
          "created_at": "2026-02-24T19:47:08.237140Z",
          "error_text": null,
          "finished_at": "2026-02-24T19:47:08.286609Z",
          "id": "9b82247a-8853-4519-829d-ef37c4ecb64d",
          "result_json": {
            "audit_diff": {
              "added_count": 0,
              "cache_hit": true,
              "cache_source": "job:11a40cef-7a73-4371-aeba-463acf65461e manifest:.cache/axon-rust/output/domains/gofastmcp.com/11a40cef-7a73-4371-aeba-463acf65461e/manifest.jsonl",
              "current_count": 390,
              "previous_count": 390,
              "removed_count": 0,
              "start_url": "https://gofastmcp.com/",
              "unchanged_count": 390
            },
            "audit_report_path": ".cache/axon-rust/output/domains/gofastmcp.com/9b82247a-8853-4519-829d-ef37c4ecb64d/audit/gofastmcp-com-diff-report.json",
            "cache_hit": true,
            "cache_skip_browser": false,
            "crawl_stream_pages": 0,
            "elapsed_ms": 0,
            "filtered_urls": 0,
            "md_created": 390,
            "output_dir": ".cache/axon-rust/output/domains/gofastmcp.com/9b82247a-8853-4519-829d-ef37c4ecb64d",
            "pages_crawled": 0,
            "pages_discovered": 390,
            "phase": "completed",
            "thin_md": 0
          },
          "started_at": "2026-02-24T19:47:08.274994Z",
          "status": "completed",
          "updated_at": "2026-02-24T19:47:08.286609Z",
          "url": "https://gofastmcp.com/"
        },
        {
          "created_at": "2026-02-24T19:38:59.583753Z",
          "error_text": null,
          "finished_at": "2026-02-24T19:39:58.578121Z",
          "id": "11a40cef-7a73-4371-aeba-463acf65461e",
          "result_json": {
            "audit_diff": {
              "added_count": 390,
              "cache_hit": false,
              "cache_source": null,
              "current_count": 390,
              "previous_count": 0,
              "removed_count": 0,
              "start_url": "https://gofastmcp.com/",
              "unchanged_count": 0
            },
            "audit_report_path": ".cache/axon-rust/output/domains/gofastmcp.com/11a40cef-7a73-4371-aeba-463acf65461e/audit/gofastmcp-com-diff-report.json",
            "cache_hit": false,
            "cache_skip_browser": false,
            "crawl_stream_pages": 105,
            "elapsed_ms": 11196,
            "extraction_prompt": null,
            "filtered_urls": 0,
            "md_created": 390,
            "output_dir": ".cache/axon-rust/output/domains/gofastmcp.com/11a40cef-7a73-4371-aeba-463acf65461e",
            "pages_crawled": 105,
            "pages_discovered": 390,
            "phase": "completed",
            "robots_candidates": 285,
            "robots_declared_sitemaps": 1,
            "robots_discovered_urls": 386,
            "robots_failed": 0,
            "robots_filtered_existing": 101,
            "robots_sitemap_docs_parsed": 1,
            "robots_written": 285,
            "stale_urls_deleted": 0,
            "thin_md": 0
          },
          "started_at": "2026-02-24T19:38:59.626469Z",
          "status": "completed",
          "updated_at": "2026-02-24T19:39:58.578121Z",
          "url": "https://gofastmcp.com/"
        },
        {
          "created_at": "2026-02-24T15:56:07.574801Z",
          "error_text": null,
          "finished_at": "2026-02-24T15:56:13.534695Z",
          "id": "4cbe8acd-933f-4d1c-b0de-686e44dd277f",
          "result_json": {
            "audit_diff": {
              "added_count": 259,
              "cache_hit": false,
              "cache_source": null,
              "current_count": 259,
              "previous_count": 0,
              "removed_count": 0,
              "start_url": "https://platejs.org/docs/installation",
              "unchanged_count": 0
            },
            "audit_report_path": ".cache/axon-rust/output/jobs/4cbe8acd-933f-4d1c-b0de-686e44dd277f/audit/diff-report.json",
            "cache_hit": false,
            "cache_skip_browser": false,
            "crawl_stream_pages": 289,
            "elapsed_ms": 2887,
            "extraction_observability": {
              "avg_quality_score": 1,
              "estimated_cost_usd": 0.48527,
              "input_tokens_estimated": 190587,
              "output_tokens_estimated": 3520,
              "quality_band": "high",
              "total_tokens_estimated": 194107
            },
            "filtered_urls": 30,
            "md_created": 259,
            "mid_queue_injection": {
              "decisions": [
                {
                  "markdown_chars": 48423,
                  "matched_rule": "docs-first",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/api/slate/editor-api"
                },
                {
                  "markdown_chars": 23609,
                  "matched_rule": "docs-first",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/link"
                },
                {
                  "markdown_chars": 19448,
                  "matched_rule": "docs-first",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/installation/manual"
                },
                {
                  "markdown_chars": 19187,
                  "matched_rule": "docs-first",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/installation/next"
                },
                {
                  "markdown_chars": 19124,
                  "matched_rule": "docs-first",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/installation/react"
                },
                {
                  "markdown_chars": 13466,
                  "matched_rule": "docs-first",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/plugin"
                },
                {
                  "markdown_chars": 12535,
                  "matched_rule": "docs-first",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/toggle"
                },
                {
                  "markdown_chars": 10671,
                  "matched_rule": "docs-first",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/blockquote"
                },
                {
                  "markdown_chars": 10272,
                  "matched_rule": "docs-first",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/components/editor"
                },
                {
                  "markdown_chars": 10089,
                  "matched_rule": "docs-first",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/editor"
                },
                {
                  "markdown_chars": 8622,
                  "matched_rule": "docs-first",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/typescript"
                },
                {
                  "markdown_chars": 8349,
                  "matched_rule": "docs-first",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/basic-marks"
                },
                {
                  "markdown_chars": 7982,
                  "matched_rule": "high-signal-catchall",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/installation/plate-ui"
                },
                {
                  "markdown_chars": 7150,
                  "matched_rule": "high-signal-catchall",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/editor-methods"
                },
                {
                  "markdown_chars": 6436,
                  "matched_rule": "high-signal-catchall",
                  "quality_score": 0.8333333333333334,
                  "selected": true,
                  "url": "https://platejs.org/editors"
                },
                {
                  "markdown_chars": 6028,
                  "matched_rule": "high-signal-catchall",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/installation/mcp"
                },
                {
                  "markdown_chars": 5956,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/installation/node"
                },
                {
                  "markdown_chars": 5556,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/mark-toolbar-button"
                },
                {
                  "markdown_chars": 5490,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/installation/rsc"
                },
                {
                  "markdown_chars": 5386,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/plugin-components"
                },
                {
                  "markdown_chars": 5061,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/controlled"
                },
                {
                  "markdown_chars": 4052,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/troubleshooting"
                },
                {
                  "markdown_chars": 3475,
                  "matched_rule": null,
                  "quality_score": 0.9783333333333334,
                  "selected": false,
                  "url": "https://platejs.org/docs"
                },
                {
                  "markdown_chars": 3392,
                  "matched_rule": null,
                  "quality_score": 0.8117333333333334,
                  "selected": false,
                  "url": "https://platejs.org/"
                },
                {
                  "markdown_chars": 3270,
                  "matched_rule": null,
                  "quality_score": 0.9539999999999998,
                  "selected": false,
                  "url": "https://platejs.org/docs/installation"
                },
                {
                  "markdown_chars": 3099,
                  "matched_rule": null,
                  "quality_score": 0.7698,
                  "selected": false,
                  "url": "https://platejs.org/blocks/playground"
                },
                {
                  "markdown_chars": 2433,
                  "matched_rule": null,
                  "quality_score": 0.6365999999999999,
                  "selected": false,
                  "url": "https://platejs.org/blocks/editor-ai"
                }
              ],
              "enqueue_enabled": true,
              "extract_job_id": null,
              "observability": {
                "avg_quality_score": 0.9895833333333331,
                "estimated_cost_usd": 0.15343,
                "input_tokens_estimated": 57853,
                "output_tokens_estimated": 3520,
                "quality_band": "high",
                "total_tokens_estimated": 61373
              },
              "phase": "mid-crawl",
              "queue_status": "skipped_missing_prompt",
              "rules": [
                {
                  "max_urls": 12,
                  "min_markdown_chars": 800,
                  "min_quality_score": 0.55,
                  "name": "docs-first",
                  "url_contains_any": [
                    "docs",
                    "api",
                    "reference",
                    "guide"
                  ]
                },
                {
                  "max_urls": 8,
                  "min_markdown_chars": 1600,
                  "min_quality_score": 0.6,
                  "name": "tutorial-longform",
                  "url_contains_any": [
                    "tutorial",
                    "blog",
                    "article",
                    "learn"
                  ]
                },
                {
                  "max_urls": 4,
                  "min_markdown_chars": 2200,
                  "min_quality_score": 0.72,
                  "name": "high-signal-catchall",
                  "url_contains_any": []
                }
              ],
              "selected_by_rule": [
                {
                  "name": "docs-first",
                  "selected": 12
                },
                {
                  "name": "high-signal-catchall",
                  "selected": 4
                }
              ],
              "selected_candidates": 16,
              "selected_urls": [
                "https://platejs.org/docs/api/slate/editor-api",
                "https://platejs.org/docs/link",
                "https://platejs.org/docs/installation/manual",
                "https://platejs.org/docs/installation/next",
                "https://platejs.org/docs/installation/react",
                "https://platejs.org/docs/plugin",
                "https://platejs.org/docs/toggle",
                "https://platejs.org/docs/blockquote",
                "https://platejs.org/docs/components/editor",
                "https://platejs.org/docs/editor",
                "https://platejs.org/docs/typescript",
                "https://platejs.org/docs/basic-marks",
                "https://platejs.org/docs/installation/plate-ui",
                "https://platejs.org/docs/editor-methods",
                "https://platejs.org/editors",
                "https://platejs.org/docs/installation/mcp"
              ],
              "total_candidates": 27
            },
            "output_dir": ".cache/axon-rust/output/jobs/4cbe8acd-933f-4d1c-b0de-686e44dd277f",
            "pages_crawled": 289,
            "pages_discovered": 289,
            "phase": "completed",
            "queue_injection": {
              "decisions": [
                {
                  "markdown_chars": 166096,
                  "matched_rule": "docs-first",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/migration/v48"
                },
                {
                  "markdown_chars": 80836,
                  "matched_rule": "docs-first",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/ai"
                },
                {
                  "markdown_chars": 56871,
                  "matched_rule": "docs-first",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/components/changelog"
                },
                {
                  "markdown_chars": 56063,
                  "matched_rule": "docs-first",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/markdown"
                },
                {
                  "markdown_chars": 55583,
                  "matched_rule": "docs-first",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/migration"
                },
                {
                  "markdown_chars": 48423,
                  "matched_rule": "docs-first",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/api/slate/editor-api"
                },
                {
                  "markdown_chars": 45339,
                  "matched_rule": "docs-first",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/table"
                },
                {
                  "markdown_chars": 33876,
                  "matched_rule": "docs-first",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/media"
                },
                {
                  "markdown_chars": 31891,
                  "matched_rule": "docs-first",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/html"
                },
                {
                  "markdown_chars": 31205,
                  "matched_rule": "docs-first",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/api/slate/editor-transforms"
                },
                {
                  "markdown_chars": 30833,
                  "matched_rule": "docs-first",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/toolbar"
                },
                {
                  "markdown_chars": 27421,
                  "matched_rule": "docs-first",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/yjs"
                },
                {
                  "markdown_chars": 26249,
                  "matched_rule": "high-signal-catchall",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/api/core"
                },
                {
                  "markdown_chars": 24344,
                  "matched_rule": "high-signal-catchall",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/plugin-rules"
                },
                {
                  "markdown_chars": 23686,
                  "matched_rule": "high-signal-catchall",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/combobox"
                },
                {
                  "markdown_chars": 23609,
                  "matched_rule": "high-signal-catchall",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/link"
                },
                {
                  "markdown_chars": 23528,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/copilot"
                },
                {
                  "markdown_chars": 22608,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/markdown-streaming"
                },
                {
                  "markdown_chars": 21635,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/static"
                },
                {
                  "markdown_chars": 21348,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/block-selection"
                },
                {
                  "markdown_chars": 20789,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/list-classic"
                },
                {
                  "markdown_chars": 20611,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/suggestion"
                },
                {
                  "markdown_chars": 20353,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/autoformat"
                },
                {
                  "markdown_chars": 19449,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/comment"
                },
                {
                  "markdown_chars": 19448,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/installation/manual"
                },
                {
                  "markdown_chars": 19187,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/installation/next"
                },
                {
                  "markdown_chars": 19124,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/installation/react"
                },
                {
                  "markdown_chars": 18662,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/api/slate/node"
                },
                {
                  "markdown_chars": 16058,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/dnd"
                },
                {
                  "markdown_chars": 15524,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/list"
                },
                {
                  "markdown_chars": 14773,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/inline-combobox"
                },
                {
                  "markdown_chars": 14425,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/plugin-shortcuts"
                },
                {
                  "markdown_chars": 14388,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/toc"
                },
                {
                  "markdown_chars": 14105,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/code-drawing"
                },
                {
                  "markdown_chars": 14079,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/discussion"
                },
                {
                  "markdown_chars": 13904,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/code-block"
                },
                {
                  "markdown_chars": 13863,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/docx-io"
                },
                {
                  "markdown_chars": 13479,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/multi-select"
                },
                {
                  "markdown_chars": 13466,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/plugin"
                },
                {
                  "markdown_chars": 13256,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/tabbable"
                },
                {
                  "markdown_chars": 13183,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/form"
                },
                {
                  "markdown_chars": 12628,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/column"
                },
                {
                  "markdown_chars": 12535,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/toggle"
                },
                {
                  "markdown_chars": 12449,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/font"
                },
                {
                  "markdown_chars": 12270,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/indent"
                },
                {
                  "markdown_chars": 11774,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/heading"
                },
                {
                  "markdown_chars": 11397,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/api/slate/path"
                },
                {
                  "markdown_chars": 11234,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/cursor-overlay"
                },
                {
                  "markdown_chars": 11160,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/comment-toolbar-button"
                },
                {
                  "markdown_chars": 10828,
                  "matched_rule": null,
                  "quality_score": 0.85,
                  "selected": false,
                  "url": "https://platejs.org/blocks/slate-to-html"
                },
                {
                  "markdown_chars": 10823,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/text-align"
                },
                {
                  "markdown_chars": 10789,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/plugin-methods"
                },
                {
                  "markdown_chars": 10781,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/caption"
                },
                {
                  "markdown_chars": 10737,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/api/core/plate-plugin"
                },
                {
                  "markdown_chars": 10671,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/blockquote"
                },
                {
                  "markdown_chars": 10471,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/block-menu"
                },
                {
                  "markdown_chars": 10435,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/playwright"
                },
                {
                  "markdown_chars": 10428,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/exit-break"
                },
                {
                  "markdown_chars": 10349,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/slash-command"
                },
                {
                  "markdown_chars": 10303,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/mention"
                },
                {
                  "markdown_chars": 10301,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/list-classic"
                },
                {
                  "markdown_chars": 10272,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/editor"
                },
                {
                  "markdown_chars": 10251,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/ai-toolbar-button"
                },
                {
                  "markdown_chars": 10237,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/horizontal-rule"
                },
                {
                  "markdown_chars": 10089,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/editor"
                },
                {
                  "markdown_chars": 10033,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/find-replace"
                },
                {
                  "markdown_chars": 10005,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/equation-toolbar-button"
                },
                {
                  "markdown_chars": 9895,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/single-block"
                },
                {
                  "markdown_chars": 9797,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/callout"
                },
                {
                  "markdown_chars": 9772,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/equation"
                },
                {
                  "markdown_chars": 9691,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/emoji"
                },
                {
                  "markdown_chars": 9652,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/line-height"
                },
                {
                  "markdown_chars": 9509,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/block-placeholder"
                },
                {
                  "markdown_chars": 9444,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components"
                },
                {
                  "markdown_chars": 9294,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/code-drawing"
                },
                {
                  "markdown_chars": 9273,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/excalidraw"
                },
                {
                  "markdown_chars": 9240,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/highlight"
                },
                {
                  "markdown_chars": 9164,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/table-nomerge"
                },
                {
                  "markdown_chars": 9108,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/code"
                },
                {
                  "markdown_chars": 9100,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/date"
                },
                {
                  "markdown_chars": 9099,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/basic-nodes"
                },
                {
                  "markdown_chars": 9068,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/slash-command"
                },
                {
                  "markdown_chars": 9059,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/block-menu"
                },
                {
                  "markdown_chars": 9044,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/discussion"
                },
                {
                  "markdown_chars": 9036,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/column"
                },
                {
                  "markdown_chars": 9032,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/mention"
                },
                {
                  "markdown_chars": 9031,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/list"
                },
                {
                  "markdown_chars": 9029,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/strikethrough"
                },
                {
                  "markdown_chars": 9029,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/csv"
                },
                {
                  "markdown_chars": 9026,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/markdown"
                },
                {
                  "markdown_chars": 9025,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/floating-toolbar"
                },
                {
                  "markdown_chars": 9021,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/callout"
                },
                {
                  "markdown_chars": 9020,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/code-block"
                },
                {
                  "markdown_chars": 9017,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/html"
                },
                {
                  "markdown_chars": 9016,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/equation"
                },
                {
                  "markdown_chars": 9015,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/table"
                },
                {
                  "markdown_chars": 9014,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/date"
                },
                {
                  "markdown_chars": 9011,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/exit-break"
                },
                {
                  "markdown_chars": 9007,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/block-selection"
                },
                {
                  "markdown_chars": 9004,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/dnd"
                },
                {
                  "markdown_chars": 9002,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/toc"
                },
                {
                  "markdown_chars": 9002,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/docx"
                },
                {
                  "markdown_chars": 9001,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/emoji"
                },
                {
                  "markdown_chars": 8999,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/link"
                },
                {
                  "markdown_chars": 8999,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/plugin-rules"
                },
                {
                  "markdown_chars": 8996,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/debugging"
                },
                {
                  "markdown_chars": 8993,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/cursor-overlay"
                },
                {
                  "markdown_chars": 8992,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/font"
                },
                {
                  "markdown_chars": 8987,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/autoformat"
                },
                {
                  "markdown_chars": 8987,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/ai"
                },
                {
                  "markdown_chars": 8978,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/superscript"
                },
                {
                  "markdown_chars": 8977,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/block-placeholder"
                },
                {
                  "markdown_chars": 8972,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/subscript"
                },
                {
                  "markdown_chars": 8971,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/line-height"
                },
                {
                  "markdown_chars": 8968,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/csv"
                },
                {
                  "markdown_chars": 8965,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/align"
                },
                {
                  "markdown_chars": 8962,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/media"
                },
                {
                  "markdown_chars": 8956,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/indent"
                },
                {
                  "markdown_chars": 8937,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/toggle"
                },
                {
                  "markdown_chars": 8912,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/underline"
                },
                {
                  "markdown_chars": 8905,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/kbd"
                },
                {
                  "markdown_chars": 8881,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/italic"
                },
                {
                  "markdown_chars": 8833,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/bold"
                },
                {
                  "markdown_chars": 8622,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/typescript"
                },
                {
                  "markdown_chars": 8588,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/basic-blocks"
                },
                {
                  "markdown_chars": 8472,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/api/slate/operation"
                },
                {
                  "markdown_chars": 8404,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/docx"
                },
                {
                  "markdown_chars": 8373,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/fixed-toolbar-buttons"
                },
                {
                  "markdown_chars": 8349,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/basic-marks"
                },
                {
                  "markdown_chars": 8310,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/installation/docs"
                },
                {
                  "markdown_chars": 8103,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/trailing-block"
                },
                {
                  "markdown_chars": 8069,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/version-history"
                },
                {
                  "markdown_chars": 7982,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/installation/plate-ui"
                },
                {
                  "markdown_chars": 7701,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/single-block"
                },
                {
                  "markdown_chars": 7654,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/excalidraw"
                },
                {
                  "markdown_chars": 7477,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/ai-menu"
                },
                {
                  "markdown_chars": 7281,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/floating-toolbar-classic-buttons"
                },
                {
                  "markdown_chars": 7211,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/export-toolbar-button"
                },
                {
                  "markdown_chars": 7194,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/api/core/plate-store"
                },
                {
                  "markdown_chars": 7150,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/editor-methods"
                },
                {
                  "markdown_chars": 7073,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/editable-voids"
                },
                {
                  "markdown_chars": 6846,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/unit-testing"
                },
                {
                  "markdown_chars": 6795,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/api/resizable"
                },
                {
                  "markdown_chars": 6785,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/slate-to-html"
                },
                {
                  "markdown_chars": 6719,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/collaboration"
                },
                {
                  "markdown_chars": 6707,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/floating-toolbar-buttons"
                },
                {
                  "markdown_chars": 6700,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/api/slate/range"
                },
                {
                  "markdown_chars": 6577,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/migration/slate-to-plate"
                },
                {
                  "markdown_chars": 6436,
                  "matched_rule": null,
                  "quality_score": 0.8333333333333334,
                  "selected": false,
                  "url": "https://platejs.org/editors"
                },
                {
                  "markdown_chars": 6313,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/media-video-node"
                },
                {
                  "markdown_chars": 6306,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/media-embed-node"
                },
                {
                  "markdown_chars": 6281,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/block-discussion"
                },
                {
                  "markdown_chars": 6280,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/api/core/plate-components"
                },
                {
                  "markdown_chars": 6280,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/media-image-node"
                },
                {
                  "markdown_chars": 6192,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/media-file-node"
                },
                {
                  "markdown_chars": 6172,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/media-audio-node"
                },
                {
                  "markdown_chars": 6171,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/media-placeholder-node"
                },
                {
                  "markdown_chars": 6168,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/suggestion-node"
                },
                {
                  "markdown_chars": 6140,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/api/core/plate-controller"
                },
                {
                  "markdown_chars": 6139,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/turn-into-toolbar-classic-button"
                },
                {
                  "markdown_chars": 6139,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/resize-handle"
                },
                {
                  "markdown_chars": 6136,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/insert-toolbar-classic-button"
                },
                {
                  "markdown_chars": 6130,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/list-classic-toolbar-button"
                },
                {
                  "markdown_chars": 6122,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/media-preview-dialog"
                },
                {
                  "markdown_chars": 6114,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/api/slate/element"
                },
                {
                  "markdown_chars": 6100,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/media-toolbar-button"
                },
                {
                  "markdown_chars": 6083,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/media-upload-toast"
                },
                {
                  "markdown_chars": 6068,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/media-toolbar"
                },
                {
                  "markdown_chars": 6032,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/list-classic-node"
                },
                {
                  "markdown_chars": 6028,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/installation/mcp"
                },
                {
                  "markdown_chars": 5956,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/installation/node"
                },
                {
                  "markdown_chars": 5947,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/export"
                },
                {
                  "markdown_chars": 5719,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/turn-into-toolbar-button"
                },
                {
                  "markdown_chars": 5675,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/block-context-menu"
                },
                {
                  "markdown_chars": 5618,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/ghost-text"
                },
                {
                  "markdown_chars": 5597,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/api/floating"
                },
                {
                  "markdown_chars": 5589,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/slash-node"
                },
                {
                  "markdown_chars": 5582,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/more-toolbar-button"
                },
                {
                  "markdown_chars": 5574,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/paragraph-node"
                },
                {
                  "markdown_chars": 5568,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/heading-node"
                },
                {
                  "markdown_chars": 5561,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/toc-node"
                },
                {
                  "markdown_chars": 5556,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/mark-toolbar-button"
                },
                {
                  "markdown_chars": 5543,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/block-draggable"
                },
                {
                  "markdown_chars": 5535,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/blockquote-node"
                },
                {
                  "markdown_chars": 5528,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/comment-node"
                },
                {
                  "markdown_chars": 5526,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/code-drawing-node"
                },
                {
                  "markdown_chars": 5517,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/floating-toolbar"
                },
                {
                  "markdown_chars": 5503,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/suggestion-toolbar-button"
                },
                {
                  "markdown_chars": 5490,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/installation/rsc"
                },
                {
                  "markdown_chars": 5476,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/api/slate/text"
                },
                {
                  "markdown_chars": 5448,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/ai-node"
                },
                {
                  "markdown_chars": 5445,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/block-selection"
                },
                {
                  "markdown_chars": 5431,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/table-node"
                },
                {
                  "markdown_chars": 5386,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/plugin-components"
                },
                {
                  "markdown_chars": 5361,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/callout-node"
                },
                {
                  "markdown_chars": 5351,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/link-toolbar-button"
                },
                {
                  "markdown_chars": 5349,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/font-color-toolbar-button"
                },
                {
                  "markdown_chars": 5349,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/link-toolbar"
                },
                {
                  "markdown_chars": 5342,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/history-toolbar-button"
                },
                {
                  "markdown_chars": 5341,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/emoji-node"
                },
                {
                  "markdown_chars": 5326,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/emoji-toolbar-button"
                },
                {
                  "markdown_chars": 5324,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/import-toolbar-button"
                },
                {
                  "markdown_chars": 5324,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/line-height-toolbar-button"
                },
                {
                  "markdown_chars": 5320,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/mention-node"
                },
                {
                  "markdown_chars": 5319,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/fixed-toolbar"
                },
                {
                  "markdown_chars": 5319,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/insert-toolbar-button"
                },
                {
                  "markdown_chars": 5292,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/mode-toolbar-button"
                },
                {
                  "markdown_chars": 5286,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/hr-node"
                },
                {
                  "markdown_chars": 5284,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/code-block-node"
                },
                {
                  "markdown_chars": 5283,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/cursor-overlay"
                },
                {
                  "markdown_chars": 5282,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/caption"
                },
                {
                  "markdown_chars": 5280,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/list-toolbar-button"
                },
                {
                  "markdown_chars": 5275,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/toggle-toolbar-button"
                },
                {
                  "markdown_chars": 5275,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/column-node"
                },
                {
                  "markdown_chars": 5271,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/align-toolbar-button"
                },
                {
                  "markdown_chars": 5266,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/indent-toolbar-button"
                },
                {
                  "markdown_chars": 5258,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/table-toolbar-button"
                },
                {
                  "markdown_chars": 5252,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/font-size-toolbar-button"
                },
                {
                  "markdown_chars": 5240,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/date-node"
                },
                {
                  "markdown_chars": 5239,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/code-node"
                },
                {
                  "markdown_chars": 5232,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/link-node"
                },
                {
                  "markdown_chars": 5228,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/equation-node"
                },
                {
                  "markdown_chars": 5210,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/kbd-node"
                },
                {
                  "markdown_chars": 5193,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/highlight-node"
                },
                {
                  "markdown_chars": 5145,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/block-list"
                },
                {
                  "markdown_chars": 5113,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/toggle-node"
                },
                {
                  "markdown_chars": 5061,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/controlled"
                },
                {
                  "markdown_chars": 5004,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/api/slate/location-ref"
                },
                {
                  "markdown_chars": 4991,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/api/core/plate-editor"
                },
                {
                  "markdown_chars": 4571,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/api/slate/point"
                },
                {
                  "markdown_chars": 4541,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples"
                },
                {
                  "markdown_chars": 4449,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/plugins"
                },
                {
                  "markdown_chars": 4316,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/plugin-context"
                },
                {
                  "markdown_chars": 4052,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/troubleshooting"
                },
                {
                  "markdown_chars": 3631,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/select-editor"
                },
                {
                  "markdown_chars": 3626,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/preview-markdown"
                },
                {
                  "markdown_chars": 3531,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/tag-node"
                },
                {
                  "markdown_chars": 3475,
                  "matched_rule": null,
                  "quality_score": 0.9783333333333334,
                  "selected": false,
                  "url": "https://platejs.org/docs"
                },
                {
                  "markdown_chars": 3392,
                  "matched_rule": null,
                  "quality_score": 0.8117333333333334,
                  "selected": false,
                  "url": "https://platejs.org/"
                },
                {
                  "markdown_chars": 3392,
                  "matched_rule": null,
                  "quality_score": 0.9784,
                  "selected": false,
                  "url": "https://platejs.org/docs/feature-kits"
                },
                {
                  "markdown_chars": 3270,
                  "matched_rule": null,
                  "quality_score": 0.9539999999999998,
                  "selected": false,
                  "url": "https://platejs.org/docs/installation"
                },
                {
                  "markdown_chars": 3099,
                  "matched_rule": null,
                  "quality_score": 0.7698,
                  "selected": false,
                  "url": "https://platejs.org/blocks/playground"
                },
                {
                  "markdown_chars": 3084,
                  "matched_rule": null,
                  "quality_score": 0.9168,
                  "selected": false,
                  "url": "https://platejs.org/docs/api/react-utils"
                },
                {
                  "markdown_chars": 2812,
                  "matched_rule": null,
                  "quality_score": 0.8623999999999999,
                  "selected": false,
                  "url": "https://platejs.org/docs/api/utils"
                },
                {
                  "markdown_chars": 2746,
                  "matched_rule": null,
                  "quality_score": 0.8492000000000001,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/hundreds-blocks"
                },
                {
                  "markdown_chars": 2517,
                  "matched_rule": null,
                  "quality_score": 0.8034,
                  "selected": false,
                  "url": "https://platejs.org/docs/api/slate/location"
                },
                {
                  "markdown_chars": 2498,
                  "matched_rule": null,
                  "quality_score": 0.7996,
                  "selected": false,
                  "url": "https://platejs.org/docs/api/cn"
                },
                {
                  "markdown_chars": 2433,
                  "matched_rule": null,
                  "quality_score": 0.6365999999999999,
                  "selected": false,
                  "url": "https://platejs.org/blocks/editor-ai"
                },
                {
                  "markdown_chars": 2400,
                  "matched_rule": null,
                  "quality_score": 0.7799999999999999,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/hundreds-editors"
                },
                {
                  "markdown_chars": 2262,
                  "matched_rule": null,
                  "quality_score": 0.7523999999999998,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/fixed-toolbar-classic-buttons"
                },
                {
                  "markdown_chars": 2243,
                  "matched_rule": null,
                  "quality_score": 0.7486,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/server-side"
                },
                {
                  "markdown_chars": 1502,
                  "matched_rule": null,
                  "quality_score": 0.4504,
                  "selected": false,
                  "url": "https://platejs.org/blocks/markdown-streaming-demo"
                },
                {
                  "markdown_chars": 1209,
                  "matched_rule": null,
                  "quality_score": 0.5418,
                  "selected": false,
                  "url": "https://platejs.org/docs/api"
                },
                {
                  "markdown_chars": 874,
                  "matched_rule": null,
                  "quality_score": 0.4748,
                  "selected": false,
                  "url": "https://platejs.org/docs/api/slate"
                },
                {
                  "markdown_chars": 608,
                  "matched_rule": null,
                  "quality_score": 0.4216,
                  "selected": false,
                  "url": "https://platejs.org/docs/api/plate"
                },
                {
                  "markdown_chars": 500,
                  "matched_rule": null,
                  "quality_score": 0.4,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/remote-cursor-overlay"
                },
                {
                  "markdown_chars": 496,
                  "matched_rule": null,
                  "quality_score": 0.3992,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/excalidraw-node"
                },
                {
                  "markdown_chars": 479,
                  "matched_rule": null,
                  "quality_score": 0.3958,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/search-highlight-node"
                },
                {
                  "markdown_chars": 455,
                  "matched_rule": null,
                  "quality_score": 0.391,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/toolbar"
                }
              ],
              "enqueue_enabled": true,
              "extract_job_id": null,
              "observability": {
                "avg_quality_score": 1,
                "estimated_cost_usd": 0.48527,
                "input_tokens_estimated": 190587,
                "output_tokens_estimated": 3520,
                "quality_band": "high",
                "total_tokens_estimated": 194107
              },
              "phase": "post-crawl",
              "queue_status": "skipped_missing_prompt",
              "rules": [
                {
                  "max_urls": 12,
                  "min_markdown_chars": 800,
                  "min_quality_score": 0.55,
                  "name": "docs-first",
                  "url_contains_any": [
                    "docs",
                    "api",
                    "reference",
                    "guide"
                  ]
                },
                {
                  "max_urls": 8,
                  "min_markdown_chars": 1600,
                  "min_quality_score": 0.6,
                  "name": "tutorial-longform",
                  "url_contains_any": [
                    "tutorial",
                    "blog",
                    "article",
                    "learn"
                  ]
                },
                {
                  "max_urls": 4,
                  "min_markdown_chars": 2200,
                  "min_quality_score": 0.72,
                  "name": "high-signal-catchall",
                  "url_contains_any": []
                }
              ],
              "selected_by_rule": [
                {
                  "name": "docs-first",
                  "selected": 12
                },
                {
                  "name": "high-signal-catchall",
                  "selected": 4
                }
              ],
              "selected_candidates": 16,
              "selected_urls": [
                "https://platejs.org/docs/migration/v48",
                "https://platejs.org/docs/ai",
                "https://platejs.org/docs/components/changelog",
                "https://platejs.org/docs/markdown",
                "https://platejs.org/docs/migration",
                "https://platejs.org/docs/api/slate/editor-api",
                "https://platejs.org/docs/table",
                "https://platejs.org/docs/media",
                "https://platejs.org/docs/html",
                "https://platejs.org/docs/api/slate/editor-transforms",
                "https://platejs.org/docs/toolbar",
                "https://platejs.org/docs/yjs",
                "https://platejs.org/docs/api/core",
                "https://platejs.org/docs/plugin-rules",
                "https://platejs.org/docs/combobox",
                "https://platejs.org/docs/link"
              ],
              "total_candidates": 259
            },
            "robots_candidates": 0,
            "robots_declared_sitemaps": 0,
            "robots_discovered_urls": 0,
            "robots_failed": 0,
            "robots_filtered_existing": 0,
            "robots_sitemap_docs_parsed": 0,
            "robots_written": 0,
            "stale_urls_deleted": 0,
            "thin_md": 30
          },
          "started_at": "2026-02-24T15:56:07.616407Z",
          "status": "completed",
          "updated_at": "2026-02-24T15:56:13.534695Z",
          "url": "https://platejs.org/docs/installation"
        },
        {
          "created_at": "2026-02-24T15:22:39.044613Z",
          "error_text": null,
          "finished_at": "2026-02-24T15:23:18.278527Z",
          "id": "d13f39b5-a23e-45ea-ac0e-9f9beedb3461",
          "result_json": {
            "audit_diff": {
              "added_count": 259,
              "cache_hit": false,
              "cache_source": null,
              "current_count": 259,
              "previous_count": 0,
              "removed_count": 0,
              "start_url": "https://platejs.org/docs/installation/plate-ui",
              "unchanged_count": 0
            },
            "audit_report_path": ".cache/axon-rust/output/jobs/d13f39b5-a23e-45ea-ac0e-9f9beedb3461/audit/diff-report.json",
            "cache_hit": false,
            "cache_skip_browser": false,
            "crawl_stream_pages": 289,
            "elapsed_ms": 35067,
            "extraction_observability": {
              "avg_quality_score": 1,
              "estimated_cost_usd": 0.48527,
              "input_tokens_estimated": 190587,
              "output_tokens_estimated": 3520,
              "quality_band": "high",
              "total_tokens_estimated": 194107
            },
            "filtered_urls": 30,
            "md_created": 259,
            "mid_queue_injection": {
              "decisions": [
                {
                  "markdown_chars": 80836,
                  "matched_rule": "docs-first",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/ai"
                },
                {
                  "markdown_chars": 24344,
                  "matched_rule": "docs-first",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/plugin-rules"
                },
                {
                  "markdown_chars": 19187,
                  "matched_rule": "docs-first",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/installation/next"
                },
                {
                  "markdown_chars": 19124,
                  "matched_rule": "docs-first",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/installation/react"
                },
                {
                  "markdown_chars": 10303,
                  "matched_rule": "docs-first",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/mention"
                },
                {
                  "markdown_chars": 10005,
                  "matched_rule": "docs-first",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/components/equation-toolbar-button"
                },
                {
                  "markdown_chars": 9691,
                  "matched_rule": "docs-first",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/emoji"
                },
                {
                  "markdown_chars": 9444,
                  "matched_rule": "docs-first",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/components"
                },
                {
                  "markdown_chars": 8373,
                  "matched_rule": "docs-first",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/components/fixed-toolbar-buttons"
                },
                {
                  "markdown_chars": 7982,
                  "matched_rule": "docs-first",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/installation/plate-ui"
                },
                {
                  "markdown_chars": 6436,
                  "matched_rule": "high-signal-catchall",
                  "quality_score": 0.8333333333333334,
                  "selected": true,
                  "url": "https://platejs.org/editors"
                },
                {
                  "markdown_chars": 5719,
                  "matched_rule": "docs-first",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/components/turn-into-toolbar-button"
                },
                {
                  "markdown_chars": 5582,
                  "matched_rule": "docs-first",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/components/more-toolbar-button"
                },
                {
                  "markdown_chars": 5528,
                  "matched_rule": "high-signal-catchall",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/components/comment-node"
                },
                {
                  "markdown_chars": 5517,
                  "matched_rule": "high-signal-catchall",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/components/floating-toolbar"
                },
                {
                  "markdown_chars": 5503,
                  "matched_rule": "high-signal-catchall",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/components/suggestion-toolbar-button"
                },
                {
                  "markdown_chars": 5386,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/plugin-components"
                },
                {
                  "markdown_chars": 5349,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/font-color-toolbar-button"
                },
                {
                  "markdown_chars": 5324,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/import-toolbar-button"
                },
                {
                  "markdown_chars": 5319,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/fixed-toolbar"
                },
                {
                  "markdown_chars": 5228,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/equation-node"
                },
                {
                  "markdown_chars": 5193,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/highlight-node"
                },
                {
                  "markdown_chars": 3475,
                  "matched_rule": null,
                  "quality_score": 0.9783333333333334,
                  "selected": false,
                  "url": "https://platejs.org/docs"
                },
                {
                  "markdown_chars": 3392,
                  "matched_rule": null,
                  "quality_score": 0.8117333333333334,
                  "selected": false,
                  "url": "https://platejs.org/"
                },
                {
                  "markdown_chars": 2262,
                  "matched_rule": null,
                  "quality_score": 0.7523999999999998,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/fixed-toolbar-classic-buttons"
                },
                {
                  "markdown_chars": 455,
                  "matched_rule": null,
                  "quality_score": 0.391,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/toolbar"
                }
              ],
              "enqueue_enabled": true,
              "extract_job_id": null,
              "observability": {
                "avg_quality_score": 0.9895833333333331,
                "estimated_cost_usd": 0.15479,
                "input_tokens_estimated": 58398,
                "output_tokens_estimated": 3520,
                "quality_band": "high",
                "total_tokens_estimated": 61918
              },
              "phase": "mid-crawl",
              "queue_status": "skipped_missing_prompt",
              "rules": [
                {
                  "max_urls": 12,
                  "min_markdown_chars": 800,
                  "min_quality_score": 0.55,
                  "name": "docs-first",
                  "url_contains_any": [
                    "docs",
                    "api",
                    "reference",
                    "guide"
                  ]
                },
                {
                  "max_urls": 8,
                  "min_markdown_chars": 1600,
                  "min_quality_score": 0.6,
                  "name": "tutorial-longform",
                  "url_contains_any": [
                    "tutorial",
                    "blog",
                    "article",
                    "learn"
                  ]
                },
                {
                  "max_urls": 4,
                  "min_markdown_chars": 2200,
                  "min_quality_score": 0.72,
                  "name": "high-signal-catchall",
                  "url_contains_any": []
                }
              ],
              "selected_by_rule": [
                {
                  "name": "docs-first",
                  "selected": 12
                },
                {
                  "name": "high-signal-catchall",
                  "selected": 4
                }
              ],
              "selected_candidates": 16,
              "selected_urls": [
                "https://platejs.org/docs/ai",
                "https://platejs.org/docs/plugin-rules",
                "https://platejs.org/docs/installation/next",
                "https://platejs.org/docs/installation/react",
                "https://platejs.org/docs/mention",
                "https://platejs.org/docs/components/equation-toolbar-button",
                "https://platejs.org/docs/emoji",
                "https://platejs.org/docs/components",
                "https://platejs.org/docs/components/fixed-toolbar-buttons",
                "https://platejs.org/docs/installation/plate-ui",
                "https://platejs.org/editors",
                "https://platejs.org/docs/components/turn-into-toolbar-button",
                "https://platejs.org/docs/components/more-toolbar-button",
                "https://platejs.org/docs/components/comment-node",
                "https://platejs.org/docs/components/floating-toolbar",
                "https://platejs.org/docs/components/suggestion-toolbar-button"
              ],
              "total_candidates": 26
            },
            "output_dir": ".cache/axon-rust/output/jobs/d13f39b5-a23e-45ea-ac0e-9f9beedb3461",
            "pages_crawled": 289,
            "pages_discovered": 289,
            "phase": "completed",
            "queue_injection": {
              "decisions": [
                {
                  "markdown_chars": 166096,
                  "matched_rule": "docs-first",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/migration/v48"
                },
                {
                  "markdown_chars": 80836,
                  "matched_rule": "docs-first",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/ai"
                },
                {
                  "markdown_chars": 56871,
                  "matched_rule": "docs-first",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/components/changelog"
                },
                {
                  "markdown_chars": 56063,
                  "matched_rule": "docs-first",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/markdown"
                },
                {
                  "markdown_chars": 55583,
                  "matched_rule": "docs-first",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/migration"
                },
                {
                  "markdown_chars": 48423,
                  "matched_rule": "docs-first",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/api/slate/editor-api"
                },
                {
                  "markdown_chars": 45339,
                  "matched_rule": "docs-first",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/table"
                },
                {
                  "markdown_chars": 33876,
                  "matched_rule": "docs-first",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/media"
                },
                {
                  "markdown_chars": 31891,
                  "matched_rule": "docs-first",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/html"
                },
                {
                  "markdown_chars": 31205,
                  "matched_rule": "docs-first",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/api/slate/editor-transforms"
                },
                {
                  "markdown_chars": 30833,
                  "matched_rule": "docs-first",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/toolbar"
                },
                {
                  "markdown_chars": 27421,
                  "matched_rule": "docs-first",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/yjs"
                },
                {
                  "markdown_chars": 26249,
                  "matched_rule": "high-signal-catchall",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/api/core"
                },
                {
                  "markdown_chars": 24344,
                  "matched_rule": "high-signal-catchall",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/plugin-rules"
                },
                {
                  "markdown_chars": 23686,
                  "matched_rule": "high-signal-catchall",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/combobox"
                },
                {
                  "markdown_chars": 23609,
                  "matched_rule": "high-signal-catchall",
                  "quality_score": 1,
                  "selected": true,
                  "url": "https://platejs.org/docs/link"
                },
                {
                  "markdown_chars": 23528,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/copilot"
                },
                {
                  "markdown_chars": 22608,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/markdown-streaming"
                },
                {
                  "markdown_chars": 21635,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/static"
                },
                {
                  "markdown_chars": 21348,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/block-selection"
                },
                {
                  "markdown_chars": 20789,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/list-classic"
                },
                {
                  "markdown_chars": 20611,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/suggestion"
                },
                {
                  "markdown_chars": 20353,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/autoformat"
                },
                {
                  "markdown_chars": 19449,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/comment"
                },
                {
                  "markdown_chars": 19448,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/installation/manual"
                },
                {
                  "markdown_chars": 19187,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/installation/next"
                },
                {
                  "markdown_chars": 19124,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/installation/react"
                },
                {
                  "markdown_chars": 18662,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/api/slate/node"
                },
                {
                  "markdown_chars": 16058,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/dnd"
                },
                {
                  "markdown_chars": 15524,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/list"
                },
                {
                  "markdown_chars": 14773,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/inline-combobox"
                },
                {
                  "markdown_chars": 14425,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/plugin-shortcuts"
                },
                {
                  "markdown_chars": 14388,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/toc"
                },
                {
                  "markdown_chars": 14105,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/code-drawing"
                },
                {
                  "markdown_chars": 14079,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/discussion"
                },
                {
                  "markdown_chars": 13904,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/code-block"
                },
                {
                  "markdown_chars": 13863,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/docx-io"
                },
                {
                  "markdown_chars": 13479,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/multi-select"
                },
                {
                  "markdown_chars": 13466,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/plugin"
                },
                {
                  "markdown_chars": 13256,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/tabbable"
                },
                {
                  "markdown_chars": 13183,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/form"
                },
                {
                  "markdown_chars": 12628,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/column"
                },
                {
                  "markdown_chars": 12535,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/toggle"
                },
                {
                  "markdown_chars": 12449,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/font"
                },
                {
                  "markdown_chars": 12270,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/indent"
                },
                {
                  "markdown_chars": 11774,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/heading"
                },
                {
                  "markdown_chars": 11397,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/api/slate/path"
                },
                {
                  "markdown_chars": 11234,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/cursor-overlay"
                },
                {
                  "markdown_chars": 11160,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/comment-toolbar-button"
                },
                {
                  "markdown_chars": 10828,
                  "matched_rule": null,
                  "quality_score": 0.85,
                  "selected": false,
                  "url": "https://platejs.org/blocks/slate-to-html"
                },
                {
                  "markdown_chars": 10823,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/text-align"
                },
                {
                  "markdown_chars": 10789,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/plugin-methods"
                },
                {
                  "markdown_chars": 10781,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/caption"
                },
                {
                  "markdown_chars": 10737,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/api/core/plate-plugin"
                },
                {
                  "markdown_chars": 10671,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/blockquote"
                },
                {
                  "markdown_chars": 10471,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/block-menu"
                },
                {
                  "markdown_chars": 10435,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/playwright"
                },
                {
                  "markdown_chars": 10428,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/exit-break"
                },
                {
                  "markdown_chars": 10349,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/slash-command"
                },
                {
                  "markdown_chars": 10303,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/mention"
                },
                {
                  "markdown_chars": 10301,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/list-classic"
                },
                {
                  "markdown_chars": 10272,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/editor"
                },
                {
                  "markdown_chars": 10251,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/ai-toolbar-button"
                },
                {
                  "markdown_chars": 10237,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/horizontal-rule"
                },
                {
                  "markdown_chars": 10089,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/editor"
                },
                {
                  "markdown_chars": 10033,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/find-replace"
                },
                {
                  "markdown_chars": 10005,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/equation-toolbar-button"
                },
                {
                  "markdown_chars": 9895,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/single-block"
                },
                {
                  "markdown_chars": 9797,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/callout"
                },
                {
                  "markdown_chars": 9772,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/equation"
                },
                {
                  "markdown_chars": 9691,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/emoji"
                },
                {
                  "markdown_chars": 9652,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/line-height"
                },
                {
                  "markdown_chars": 9509,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/block-placeholder"
                },
                {
                  "markdown_chars": 9444,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components"
                },
                {
                  "markdown_chars": 9294,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/code-drawing"
                },
                {
                  "markdown_chars": 9273,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/excalidraw"
                },
                {
                  "markdown_chars": 9240,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/highlight"
                },
                {
                  "markdown_chars": 9164,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/table-nomerge"
                },
                {
                  "markdown_chars": 9108,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/code"
                },
                {
                  "markdown_chars": 9100,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/date"
                },
                {
                  "markdown_chars": 9099,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/basic-nodes"
                },
                {
                  "markdown_chars": 9068,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/slash-command"
                },
                {
                  "markdown_chars": 9059,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/block-menu"
                },
                {
                  "markdown_chars": 9044,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/discussion"
                },
                {
                  "markdown_chars": 9036,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/column"
                },
                {
                  "markdown_chars": 9032,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/mention"
                },
                {
                  "markdown_chars": 9031,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/list"
                },
                {
                  "markdown_chars": 9029,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/strikethrough"
                },
                {
                  "markdown_chars": 9029,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/csv"
                },
                {
                  "markdown_chars": 9026,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/markdown"
                },
                {
                  "markdown_chars": 9025,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/floating-toolbar"
                },
                {
                  "markdown_chars": 9021,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/callout"
                },
                {
                  "markdown_chars": 9020,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/code-block"
                },
                {
                  "markdown_chars": 9017,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/html"
                },
                {
                  "markdown_chars": 9016,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/equation"
                },
                {
                  "markdown_chars": 9015,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/table"
                },
                {
                  "markdown_chars": 9014,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/date"
                },
                {
                  "markdown_chars": 9011,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/exit-break"
                },
                {
                  "markdown_chars": 9007,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/block-selection"
                },
                {
                  "markdown_chars": 9004,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/dnd"
                },
                {
                  "markdown_chars": 9002,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/toc"
                },
                {
                  "markdown_chars": 9002,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/docx"
                },
                {
                  "markdown_chars": 9001,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/emoji"
                },
                {
                  "markdown_chars": 8999,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/link"
                },
                {
                  "markdown_chars": 8999,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/plugin-rules"
                },
                {
                  "markdown_chars": 8996,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/debugging"
                },
                {
                  "markdown_chars": 8993,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/cursor-overlay"
                },
                {
                  "markdown_chars": 8992,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/font"
                },
                {
                  "markdown_chars": 8987,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/ai"
                },
                {
                  "markdown_chars": 8987,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/autoformat"
                },
                {
                  "markdown_chars": 8978,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/superscript"
                },
                {
                  "markdown_chars": 8977,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/block-placeholder"
                },
                {
                  "markdown_chars": 8972,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/subscript"
                },
                {
                  "markdown_chars": 8971,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/line-height"
                },
                {
                  "markdown_chars": 8968,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/csv"
                },
                {
                  "markdown_chars": 8965,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/align"
                },
                {
                  "markdown_chars": 8962,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/media"
                },
                {
                  "markdown_chars": 8956,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/indent"
                },
                {
                  "markdown_chars": 8937,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/toggle"
                },
                {
                  "markdown_chars": 8912,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/underline"
                },
                {
                  "markdown_chars": 8905,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/kbd"
                },
                {
                  "markdown_chars": 8881,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/italic"
                },
                {
                  "markdown_chars": 8833,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/bold"
                },
                {
                  "markdown_chars": 8622,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/typescript"
                },
                {
                  "markdown_chars": 8588,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/basic-blocks"
                },
                {
                  "markdown_chars": 8472,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/api/slate/operation"
                },
                {
                  "markdown_chars": 8404,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/docx"
                },
                {
                  "markdown_chars": 8373,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/fixed-toolbar-buttons"
                },
                {
                  "markdown_chars": 8349,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/basic-marks"
                },
                {
                  "markdown_chars": 8310,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/installation/docs"
                },
                {
                  "markdown_chars": 8103,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/trailing-block"
                },
                {
                  "markdown_chars": 8069,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/version-history"
                },
                {
                  "markdown_chars": 7982,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/installation/plate-ui"
                },
                {
                  "markdown_chars": 7701,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/single-block"
                },
                {
                  "markdown_chars": 7654,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/excalidraw"
                },
                {
                  "markdown_chars": 7477,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/ai-menu"
                },
                {
                  "markdown_chars": 7281,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/floating-toolbar-classic-buttons"
                },
                {
                  "markdown_chars": 7211,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/export-toolbar-button"
                },
                {
                  "markdown_chars": 7194,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/api/core/plate-store"
                },
                {
                  "markdown_chars": 7150,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/editor-methods"
                },
                {
                  "markdown_chars": 7073,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/editable-voids"
                },
                {
                  "markdown_chars": 6846,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/unit-testing"
                },
                {
                  "markdown_chars": 6795,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/api/resizable"
                },
                {
                  "markdown_chars": 6785,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/slate-to-html"
                },
                {
                  "markdown_chars": 6719,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/collaboration"
                },
                {
                  "markdown_chars": 6707,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/floating-toolbar-buttons"
                },
                {
                  "markdown_chars": 6700,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/api/slate/range"
                },
                {
                  "markdown_chars": 6577,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/migration/slate-to-plate"
                },
                {
                  "markdown_chars": 6436,
                  "matched_rule": null,
                  "quality_score": 0.8333333333333334,
                  "selected": false,
                  "url": "https://platejs.org/editors"
                },
                {
                  "markdown_chars": 6313,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/media-video-node"
                },
                {
                  "markdown_chars": 6306,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/media-embed-node"
                },
                {
                  "markdown_chars": 6281,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/block-discussion"
                },
                {
                  "markdown_chars": 6280,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/media-image-node"
                },
                {
                  "markdown_chars": 6280,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/api/core/plate-components"
                },
                {
                  "markdown_chars": 6192,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/media-file-node"
                },
                {
                  "markdown_chars": 6172,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/media-audio-node"
                },
                {
                  "markdown_chars": 6171,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/media-placeholder-node"
                },
                {
                  "markdown_chars": 6168,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/suggestion-node"
                },
                {
                  "markdown_chars": 6140,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/api/core/plate-controller"
                },
                {
                  "markdown_chars": 6139,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/resize-handle"
                },
                {
                  "markdown_chars": 6139,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/turn-into-toolbar-classic-button"
                },
                {
                  "markdown_chars": 6136,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/insert-toolbar-classic-button"
                },
                {
                  "markdown_chars": 6130,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/list-classic-toolbar-button"
                },
                {
                  "markdown_chars": 6122,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/media-preview-dialog"
                },
                {
                  "markdown_chars": 6114,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/api/slate/element"
                },
                {
                  "markdown_chars": 6100,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/media-toolbar-button"
                },
                {
                  "markdown_chars": 6083,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/media-upload-toast"
                },
                {
                  "markdown_chars": 6068,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/media-toolbar"
                },
                {
                  "markdown_chars": 6032,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/list-classic-node"
                },
                {
                  "markdown_chars": 6028,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/installation/mcp"
                },
                {
                  "markdown_chars": 5956,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/installation/node"
                },
                {
                  "markdown_chars": 5947,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/export"
                },
                {
                  "markdown_chars": 5719,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/turn-into-toolbar-button"
                },
                {
                  "markdown_chars": 5675,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/block-context-menu"
                },
                {
                  "markdown_chars": 5618,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/ghost-text"
                },
                {
                  "markdown_chars": 5597,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/api/floating"
                },
                {
                  "markdown_chars": 5589,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/slash-node"
                },
                {
                  "markdown_chars": 5582,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/more-toolbar-button"
                },
                {
                  "markdown_chars": 5574,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/paragraph-node"
                },
                {
                  "markdown_chars": 5568,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/heading-node"
                },
                {
                  "markdown_chars": 5561,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/toc-node"
                },
                {
                  "markdown_chars": 5556,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/mark-toolbar-button"
                },
                {
                  "markdown_chars": 5543,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/block-draggable"
                },
                {
                  "markdown_chars": 5535,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/blockquote-node"
                },
                {
                  "markdown_chars": 5528,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/comment-node"
                },
                {
                  "markdown_chars": 5526,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/code-drawing-node"
                },
                {
                  "markdown_chars": 5517,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/floating-toolbar"
                },
                {
                  "markdown_chars": 5503,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/suggestion-toolbar-button"
                },
                {
                  "markdown_chars": 5490,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/installation/rsc"
                },
                {
                  "markdown_chars": 5476,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/api/slate/text"
                },
                {
                  "markdown_chars": 5448,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/ai-node"
                },
                {
                  "markdown_chars": 5445,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/block-selection"
                },
                {
                  "markdown_chars": 5431,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/table-node"
                },
                {
                  "markdown_chars": 5386,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/plugin-components"
                },
                {
                  "markdown_chars": 5361,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/callout-node"
                },
                {
                  "markdown_chars": 5351,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/link-toolbar-button"
                },
                {
                  "markdown_chars": 5349,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/font-color-toolbar-button"
                },
                {
                  "markdown_chars": 5349,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/link-toolbar"
                },
                {
                  "markdown_chars": 5342,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/history-toolbar-button"
                },
                {
                  "markdown_chars": 5341,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/emoji-node"
                },
                {
                  "markdown_chars": 5326,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/emoji-toolbar-button"
                },
                {
                  "markdown_chars": 5324,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/import-toolbar-button"
                },
                {
                  "markdown_chars": 5324,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/line-height-toolbar-button"
                },
                {
                  "markdown_chars": 5320,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/mention-node"
                },
                {
                  "markdown_chars": 5319,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/fixed-toolbar"
                },
                {
                  "markdown_chars": 5319,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/insert-toolbar-button"
                },
                {
                  "markdown_chars": 5292,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/mode-toolbar-button"
                },
                {
                  "markdown_chars": 5286,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/hr-node"
                },
                {
                  "markdown_chars": 5284,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/code-block-node"
                },
                {
                  "markdown_chars": 5283,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/cursor-overlay"
                },
                {
                  "markdown_chars": 5282,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/caption"
                },
                {
                  "markdown_chars": 5280,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/list-toolbar-button"
                },
                {
                  "markdown_chars": 5275,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/toggle-toolbar-button"
                },
                {
                  "markdown_chars": 5275,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/column-node"
                },
                {
                  "markdown_chars": 5271,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/align-toolbar-button"
                },
                {
                  "markdown_chars": 5266,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/indent-toolbar-button"
                },
                {
                  "markdown_chars": 5258,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/table-toolbar-button"
                },
                {
                  "markdown_chars": 5252,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/font-size-toolbar-button"
                },
                {
                  "markdown_chars": 5240,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/date-node"
                },
                {
                  "markdown_chars": 5239,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/code-node"
                },
                {
                  "markdown_chars": 5232,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/link-node"
                },
                {
                  "markdown_chars": 5228,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/equation-node"
                },
                {
                  "markdown_chars": 5210,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/kbd-node"
                },
                {
                  "markdown_chars": 5193,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/highlight-node"
                },
                {
                  "markdown_chars": 5145,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/block-list"
                },
                {
                  "markdown_chars": 5113,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/toggle-node"
                },
                {
                  "markdown_chars": 5061,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/controlled"
                },
                {
                  "markdown_chars": 5004,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/api/slate/location-ref"
                },
                {
                  "markdown_chars": 4991,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/api/core/plate-editor"
                },
                {
                  "markdown_chars": 4571,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/api/slate/point"
                },
                {
                  "markdown_chars": 4541,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples"
                },
                {
                  "markdown_chars": 4449,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/plugins"
                },
                {
                  "markdown_chars": 4316,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/plugin-context"
                },
                {
                  "markdown_chars": 4052,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/troubleshooting"
                },
                {
                  "markdown_chars": 3631,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/select-editor"
                },
                {
                  "markdown_chars": 3626,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/preview-markdown"
                },
                {
                  "markdown_chars": 3531,
                  "matched_rule": null,
                  "quality_score": 1,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/tag-node"
                },
                {
                  "markdown_chars": 3475,
                  "matched_rule": null,
                  "quality_score": 0.9783333333333334,
                  "selected": false,
                  "url": "https://platejs.org/docs"
                },
                {
                  "markdown_chars": 3392,
                  "matched_rule": null,
                  "quality_score": 0.8117333333333334,
                  "selected": false,
                  "url": "https://platejs.org/"
                },
                {
                  "markdown_chars": 3392,
                  "matched_rule": null,
                  "quality_score": 0.9784,
                  "selected": false,
                  "url": "https://platejs.org/docs/feature-kits"
                },
                {
                  "markdown_chars": 3270,
                  "matched_rule": null,
                  "quality_score": 0.9539999999999998,
                  "selected": false,
                  "url": "https://platejs.org/docs/installation"
                },
                {
                  "markdown_chars": 3099,
                  "matched_rule": null,
                  "quality_score": 0.7698,
                  "selected": false,
                  "url": "https://platejs.org/blocks/playground"
                },
                {
                  "markdown_chars": 3084,
                  "matched_rule": null,
                  "quality_score": 0.9168,
                  "selected": false,
                  "url": "https://platejs.org/docs/api/react-utils"
                },
                {
                  "markdown_chars": 2812,
                  "matched_rule": null,
                  "quality_score": 0.8623999999999999,
                  "selected": false,
                  "url": "https://platejs.org/docs/api/utils"
                },
                {
                  "markdown_chars": 2746,
                  "matched_rule": null,
                  "quality_score": 0.8492000000000001,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/hundreds-blocks"
                },
                {
                  "markdown_chars": 2517,
                  "matched_rule": null,
                  "quality_score": 0.8034,
                  "selected": false,
                  "url": "https://platejs.org/docs/api/slate/location"
                },
                {
                  "markdown_chars": 2498,
                  "matched_rule": null,
                  "quality_score": 0.7996,
                  "selected": false,
                  "url": "https://platejs.org/docs/api/cn"
                },
                {
                  "markdown_chars": 2433,
                  "matched_rule": null,
                  "quality_score": 0.6365999999999999,
                  "selected": false,
                  "url": "https://platejs.org/blocks/editor-ai"
                },
                {
                  "markdown_chars": 2400,
                  "matched_rule": null,
                  "quality_score": 0.7799999999999999,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/hundreds-editors"
                },
                {
                  "markdown_chars": 2262,
                  "matched_rule": null,
                  "quality_score": 0.7523999999999998,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/fixed-toolbar-classic-buttons"
                },
                {
                  "markdown_chars": 2243,
                  "matched_rule": null,
                  "quality_score": 0.7486,
                  "selected": false,
                  "url": "https://platejs.org/docs/examples/server-side"
                },
                {
                  "markdown_chars": 1502,
                  "matched_rule": null,
                  "quality_score": 0.4504,
                  "selected": false,
                  "url": "https://platejs.org/blocks/markdown-streaming-demo"
                },
                {
                  "markdown_chars": 1209,
                  "matched_rule": null,
                  "quality_score": 0.5418,
                  "selected": false,
                  "url": "https://platejs.org/docs/api"
                },
                {
                  "markdown_chars": 874,
                  "matched_rule": null,
                  "quality_score": 0.4748,
                  "selected": false,
                  "url": "https://platejs.org/docs/api/slate"
                },
                {
                  "markdown_chars": 608,
                  "matched_rule": null,
                  "quality_score": 0.4216,
                  "selected": false,
                  "url": "https://platejs.org/docs/api/plate"
                },
                {
                  "markdown_chars": 500,
                  "matched_rule": null,
                  "quality_score": 0.4,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/remote-cursor-overlay"
                },
                {
                  "markdown_chars": 496,
                  "matched_rule": null,
                  "quality_score": 0.3992,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/excalidraw-node"
                },
                {
                  "markdown_chars": 479,
                  "matched_rule": null,
                  "quality_score": 0.3958,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/search-highlight-node"
                },
                {
                  "markdown_chars": 455,
                  "matched_rule": null,
                  "quality_score": 0.391,
                  "selected": false,
                  "url": "https://platejs.org/docs/components/toolbar"
                }
              ],
              "enqueue_enabled": true,
              "extract_job_id": null,
              "observability": {
                "avg_quality_score": 1,
                "estimated_cost_usd": 0.48527,
                "input_tokens_estimated": 190587,
                "output_tokens_estimated": 3520,
                "quality_band": "high",
                "total_tokens_estimated": 194107
              },
              "phase": "post-crawl",
              "queue_status": "skipped_missing_prompt",
              "rules": [
                {
                  "max_urls": 12,
                  "min_markdown_chars": 800,
                  "min_quality_score": 0.55,
                  "name": "docs-first",
                  "url_contains_any": [
                    "docs",
                    "api",
                    "reference",
                    "guide"
                  ]
                },
                {
                  "max_urls": 8,
                  "min_markdown_chars": 1600,
                  "min_quality_score": 0.6,
                  "name": "tutorial-longform",
                  "url_contains_any": [
                    "tutorial",
                    "blog",
                    "article",
                    "learn"
                  ]
                },
                {
                  "max_urls": 4,
                  "min_markdown_chars": 2200,
                  "min_quality_score": 0.72,
                  "name": "high-signal-catchall",
                  "url_contains_any": []
                }
              ],
              "selected_by_rule": [
                {
                  "name": "docs-first",
                  "selected": 12
                },
                {
                  "name": "high-signal-catchall",
                  "selected": 4
                }
              ],
              "selected_candidates": 16,
              "selected_urls": [
                "https://platejs.org/docs/migration/v48",
                "https://platejs.org/docs/ai",
                "https://platejs.org/docs/components/changelog",
                "https://platejs.org/docs/markdown",
                "https://platejs.org/docs/migration",
                "https://platejs.org/docs/api/slate/editor-api",
                "https://platejs.org/docs/table",
                "https://platejs.org/docs/media",
                "https://platejs.org/docs/html",
                "https://platejs.org/docs/api/slate/editor-transforms",
                "https://platejs.org/docs/toolbar",
                "https://platejs.org/docs/yjs",
                "https://platejs.org/docs/api/core",
                "https://platejs.org/docs/plugin-rules",
                "https://platejs.org/docs/combobox",
                "https://platejs.org/docs/link"
              ],
              "total_candidates": 259
            },
            "robots_candidates": 0,
            "robots_declared_sitemaps": 0,
            "robots_discovered_urls": 0,
            "robots_failed": 0,
            "robots_filtered_existing": 0,
            "robots_sitemap_docs_parsed": 0,
            "robots_written": 0,
            "stale_urls_deleted": 3,
            "thin_md": 30
          },
          "started_at": "2026-02-24T15:22:39.673473Z",
          "status": "completed",
          "updated_at": "2026-02-24T15:23:18.278527Z",
          "url": "https://platejs.org/docs/installation/plate-ui"
        }
      ],
      "local_embed_jobs": [
        {
          "config_json": {
            "collection": "cortex"
          },
          "created_at": "2026-02-25T06:22:02.549251Z",
          "error_text": null,
          "finished_at": "2026-02-25T06:22:02.706731Z",
          "id": "1b1b7c60-30a3-4e9a-8a03-792e884fc5a5",
          "input_text": "/home/jmagar/workspace/axon_rust/docs/sessions/2026-02-25-pr-review-lint-debug-session.md",
          "result_json": {
            "chunks_embedded": 1,
            "collection": "cortex",
            "docs_embedded": 1,
            "input": "/home/jmagar/workspace/axon_rust/docs/sessions/2026-02-25-pr-review-lint-debug-session.md",
            "source": "rust"
          },
          "started_at": "2026-02-25T06:22:02.585315Z",
          "status": "completed",
          "updated_at": "2026-02-25T06:22:02.706731Z"
        },
        {
          "config_json": {
            "collection": "cortex"
          },
          "created_at": "2026-02-25T06:13:45.248727Z",
          "error_text": null,
          "finished_at": "2026-02-25T06:13:45.409584Z",
          "id": "ae00e1f8-d0da-4ae9-83ac-fae5e691d230",
          "input_text": "docs/sessions/2026-02-25-neural-canvas-seam-debug.md",
          "result_json": {
            "chunks_embedded": 1,
            "collection": "cortex",
            "docs_embedded": 1,
            "input": "docs/sessions/2026-02-25-neural-canvas-seam-debug.md",
            "source": "rust"
          },
          "started_at": "2026-02-25T06:13:45.291148Z",
          "status": "completed",
          "updated_at": "2026-02-25T06:13:45.409584Z"
        },
        {
          "config_json": {
            "collection": "cortex"
          },
          "created_at": "2026-02-25T05:07:11.648311Z",
          "error_text": null,
          "finished_at": "2026-02-25T05:07:11.810132Z",
          "id": "34b5dd84-983a-475c-8e8a-b39a324b98f7",
          "input_text": "docs/sessions/2026-02-25-neural-canvas-presets-settings-cog.md",
          "result_json": {
            "chunks_embedded": 1,
            "collection": "cortex",
            "docs_embedded": 1,
            "input": "docs/sessions/2026-02-25-neural-canvas-presets-settings-cog.md",
            "source": "rust"
          },
          "started_at": "2026-02-25T05:07:11.683442Z",
          "status": "completed",
          "updated_at": "2026-02-25T05:07:11.810132Z"
        },
        {
          "config_json": {
            "collection": "cortex"
          },
          "created_at": "2026-02-25T04:51:16.882546Z",
          "error_text": null,
          "finished_at": "2026-02-25T04:51:17.151121Z",
          "id": "46adeddd-6c46-4ca1-99d0-03b5e679777c",
          "input_text": "docs/sessions/2026-02-25-pulse-mode-phase-1-foundation-session.md",
          "result_json": {
            "chunks_embedded": 1,
            "collection": "cortex",
            "docs_embedded": 1,
            "input": "docs/sessions/2026-02-25-pulse-mode-phase-1-foundation-session.md",
            "source": "rust"
          },
          "started_at": "2026-02-25T04:51:16.932137Z",
          "status": "completed",
          "updated_at": "2026-02-25T04:51:17.151121Z"
        },
        {
          "config_json": {
            "collection": "cortex"
          },
          "created_at": "2026-02-25T02:38:35.867279Z",
          "error_text": null,
          "finished_at": "2026-02-25T02:38:56.166814Z",
          "id": "1dc1ab8d-8d79-4be4-94c8-f5abf55a2c61",
          "input_text": ".cache/axon-rust/output/domains/platejs.org/6906b63a-74b6-43ed-9cfa-5f18aae33971/markdown",
          "result_json": {
            "chunks_embedded": 1027,
            "collection": "cortex",
            "docs_embedded": 173,
            "input": ".cache/axon-rust/output/domains/platejs.org/6906b63a-74b6-43ed-9cfa-5f18aae33971/markdown",
            "source": "rust"
          },
          "started_at": "2026-02-25T02:38:35.881493Z",
          "status": "completed",
          "updated_at": "2026-02-25T02:38:56.166814Z"
        },
        {
          "config_json": {
            "collection": "cortex"
          },
          "created_at": "2026-02-25T00:23:38.633048Z",
          "error_text": "watchdog reclaimed stale running embed job (idle=372s marker=amqp)",
          "finished_at": "2026-02-25T00:32:22.580989Z",
          "id": "1813ed3e-b901-4044-9f5e-3e8920ac5841",
          "input_text": ".cache/axon-rust/output/domains/code.claude.com/a103cad5-fbc3-42b0-9ea0-da98ca796891/markdown",
          "result_json": {
            "_watchdog": {
              "first_seen_stale_at": "2026-02-25T00:31:22.541937745+00:00",
              "observed_updated_at": "2026-02-25T00:26:09.680290+00:00"
            },
            "chunks_embedded": 3369,
            "docs_completed": 317,
            "docs_total": 1070,
            "phase": "embedding"
          },
          "started_at": "2026-02-25T00:23:38.641966Z",
          "status": "failed",
          "updated_at": "2026-02-25T00:32:22.580989Z"
        },
        {
          "config_json": {
            "collection": "cortex"
          },
          "created_at": "2026-02-25T00:21:18.964515Z",
          "error_text": null,
          "finished_at": "2026-02-25T00:21:18.985934Z",
          "id": "36d09efd-a669-43ce-94df-b29951fae7f7",
          "input_text": ".cache/axon-rust/output/domains/gofastmco.com/03cbb6f9-76e8-47ec-87fa-32df3e6e8fc1/markdown",
          "result_json": {
            "chunks_embedded": 0,
            "collection": "cortex",
            "docs_embedded": 0,
            "input": ".cache/axon-rust/output/domains/gofastmco.com/03cbb6f9-76e8-47ec-87fa-32df3e6e8fc1/markdown",
            "source": "rust"
          },
          "started_at": "2026-02-25T00:21:18.975786Z",
          "status": "completed",
          "updated_at": "2026-02-25T00:21:18.985934Z"
        },
        {
          "config_json": {
            "collection": "cortex"
          },
          "created_at": "2026-02-25T00:19:49.449686Z",
          "error_text": null,
          "finished_at": "2026-02-25T00:19:53.109687Z",
          "id": "b1d6f45f-d19a-41bb-b67d-bac3df330d0e",
          "input_text": "https://example.com",
          "result_json": {
            "chunks_embedded": 1,
            "collection": "cortex",
            "docs_embedded": 1,
            "input": "https://example.com",
            "source": "rust"
          },
          "started_at": "2026-02-25T00:19:49.497647Z",
          "status": "completed",
          "updated_at": "2026-02-25T00:19:53.109687Z"
        },
        {
          "config_json": {
            "collection": "cortex"
          },
          "created_at": "2026-02-25T00:18:06.771108Z",
          "error_text": null,
          "finished_at": "2026-02-25T00:18:06.990003Z",
          "id": "07c25570-0012-42a5-9fa4-92cbdfad53d4",
          "input_text": "docs/sessions/2026-02-24-screenshot-display-fix.md",
          "result_json": {
            "chunks_embedded": 1,
            "collection": "cortex",
            "docs_embedded": 1,
            "input": "docs/sessions/2026-02-24-screenshot-display-fix.md",
            "source": "rust"
          },
          "started_at": "2026-02-25T00:18:06.820082Z",
          "status": "completed",
          "updated_at": "2026-02-25T00:18:06.990003Z"
        },
        {
          "config_json": {
            "collection": "cortex"
          },
          "created_at": "2026-02-25T00:04:49.374673Z",
          "error_text": null,
          "finished_at": "2026-02-25T00:04:49.397543Z",
          "id": "a57af80e-f567-4388-9b0d-3113269f73af",
          "input_text": ".cache/axon-rust/output/domains/example.com/28b83610-cd84-4575-91c2-68799c6bfdb3/markdown",
          "result_json": {
            "chunks_embedded": 0,
            "collection": "cortex",
            "docs_embedded": 0,
            "input": ".cache/axon-rust/output/domains/example.com/28b83610-cd84-4575-91c2-68799c6bfdb3/markdown",
            "source": "rust"
          },
          "started_at": "2026-02-25T00:04:49.384680Z",
          "status": "completed",
          "updated_at": "2026-02-25T00:04:49.397543Z"
        },
        {
          "config_json": {
            "collection": "cortex"
          },
          "created_at": "2026-02-24T23:01:38.420173Z",
          "error_text": null,
          "finished_at": "2026-02-24T23:01:39.584871Z",
          "id": "3f0503e9-06c6-4c39-8d2d-b38ebe010f67",
          "input_text": "docs/sessions/2026-02-24-web-ui-command-parity.md",
          "result_json": {
            "chunks_embedded": 1,
            "collection": "cortex",
            "docs_embedded": 1,
            "input": "docs/sessions/2026-02-24-web-ui-command-parity.md",
            "source": "rust"
          },
          "started_at": "2026-02-24T23:01:38.713006Z",
          "status": "completed",
          "updated_at": "2026-02-24T23:01:39.584871Z"
        },
        {
          "config_json": {
            "collection": "cortex"
          },
          "created_at": "2026-02-24T21:28:34.385612Z",
          "error_text": null,
          "finished_at": "2026-02-24T21:30:41.756880Z",
          "id": "7cbffcb0-005d-49e3-a963-93661dfa08ac",
          "input_text": ".cache/axon-rust/output/domains/zed.dev/d7c1d3ea-5722-4ed6-a485-ade088103679/markdown",
          "result_json": {
            "chunks_embedded": 6878,
            "collection": "cortex",
            "docs_embedded": 2040,
            "input": ".cache/axon-rust/output/domains/zed.dev/d7c1d3ea-5722-4ed6-a485-ade088103679/markdown",
            "source": "rust"
          },
          "started_at": "2026-02-24T21:28:34.399312Z",
          "status": "completed",
          "updated_at": "2026-02-24T21:30:41.756880Z"
        },
        {
          "config_json": {
            "collection": "cortex"
          },
          "created_at": "2026-02-24T20:58:37.037244Z",
          "error_text": null,
          "finished_at": "2026-02-24T20:58:49.566178Z",
          "id": "d9dcb954-99cb-48e6-8de9-24a29f08eef3",
          "input_text": ".cache/axon-rust/output/domains/geminicli.com/fe1a2756-401e-4c8d-b0c1-2d01157a6296/markdown",
          "result_json": {
            "chunks_embedded": 771,
            "collection": "cortex",
            "docs_embedded": 109,
            "input": ".cache/axon-rust/output/domains/geminicli.com/fe1a2756-401e-4c8d-b0c1-2d01157a6296/markdown",
            "source": "rust"
          },
          "started_at": "2026-02-24T20:58:37.043837Z",
          "status": "completed",
          "updated_at": "2026-02-24T20:58:49.566178Z"
        },
        {
          "config_json": {
            "collection": "cortex"
          },
          "created_at": "2026-02-24T20:55:31.161115Z",
          "error_text": null,
          "finished_at": "2026-02-24T20:55:43.241097Z",
          "id": "bbd495e9-c234-4ba4-b29b-df12411a3959",
          "input_text": ".cache/axon-rust/output/domains/geminicli.com/70118f8a-0005-4ec1-a7a8-5e11ed072b5a/markdown",
          "result_json": {
            "chunks_embedded": 771,
            "collection": "cortex",
            "docs_embedded": 109,
            "input": ".cache/axon-rust/output/domains/geminicli.com/70118f8a-0005-4ec1-a7a8-5e11ed072b5a/markdown",
            "source": "rust"
          },
          "started_at": "2026-02-24T20:55:31.177046Z",
          "status": "completed",
          "updated_at": "2026-02-24T20:55:43.241097Z"
        },
        {
          "config_json": {
            "collection": "cortex"
          },
          "created_at": "2026-02-24T20:26:32.887914Z",
          "error_text": null,
          "finished_at": "2026-02-24T20:26:58.727257Z",
          "id": "88e8060b-8d40-426e-bfe1-2ecd105c9a3d",
          "input_text": ".cache/axon-rust/output/domains/modelcontextprotocol.io/2449918e-ea45-4a6f-aab1-5ee3feea5b8a/markdown",
          "result_json": {
            "chunks_embedded": 1616,
            "collection": "cortex",
            "docs_embedded": 213,
            "input": ".cache/axon-rust/output/domains/modelcontextprotocol.io/2449918e-ea45-4a6f-aab1-5ee3feea5b8a/markdown",
            "source": "rust"
          },
          "started_at": "2026-02-24T20:26:32.898388Z",
          "status": "completed",
          "updated_at": "2026-02-24T20:26:58.727257Z"
        },
        {
          "config_json": {
            "collection": "cortex"
          },
          "created_at": "2026-02-24T19:49:09.493102Z",
          "error_text": null,
          "finished_at": "2026-02-24T19:49:54.644466Z",
          "id": "dafdc51b-ac02-4ca2-a823-96cbcbee6931",
          "input_text": ".cache/axon-rust/output/domains/gofastmcp.com/de8f10d4-9984-4527-a59d-aa83f83840fb/markdown",
          "result_json": {
            "chunks_embedded": 2578,
            "collection": "cortex",
            "docs_embedded": 390,
            "input": ".cache/axon-rust/output/domains/gofastmcp.com/de8f10d4-9984-4527-a59d-aa83f83840fb/markdown",
            "source": "rust"
          },
          "started_at": "2026-02-24T19:49:09.504387Z",
          "status": "completed",
          "updated_at": "2026-02-24T19:49:54.644466Z"
        },
        {
          "config_json": {
            "collection": "cortex"
          },
          "created_at": "2026-02-24T19:39:58.514269Z",
          "error_text": null,
          "finished_at": "2026-02-24T19:40:44.291930Z",
          "id": "2a611d24-b385-461a-aa97-6bf75b3b8344",
          "input_text": ".cache/axon-rust/output/domains/gofastmcp.com/11a40cef-7a73-4371-aeba-463acf65461e/markdown",
          "result_json": {
            "chunks_embedded": 2578,
            "collection": "cortex",
            "docs_embedded": 390,
            "input": ".cache/axon-rust/output/domains/gofastmcp.com/11a40cef-7a73-4371-aeba-463acf65461e/markdown",
            "source": "rust"
          },
          "started_at": "2026-02-24T19:39:58.522534Z",
          "status": "completed",
          "updated_at": "2026-02-24T19:40:44.291930Z"
        },
        {
          "config_json": {
            "collection": "cortex"
          },
          "created_at": "2026-02-24T18:34:33.478695Z",
          "error_text": null,
          "finished_at": "2026-02-24T18:34:33.600933Z",
          "id": "0ae8c8f2-562c-4e8b-ae99-d547036d5586",
          "input_text": "docs/sessions/2026-02-24-nextjs-plate-content-viewer.md",
          "result_json": {
            "chunks_embedded": 1,
            "collection": "cortex",
            "docs_embedded": 1,
            "input": "docs/sessions/2026-02-24-nextjs-plate-content-viewer.md",
            "source": "rust"
          },
          "started_at": "2026-02-24T18:34:33.514075Z",
          "status": "completed",
          "updated_at": "2026-02-24T18:34:33.600933Z"
        },
        {
          "config_json": {
            "collection": "cortex"
          },
          "created_at": "2026-02-24T17:45:48.963594Z",
          "error_text": null,
          "finished_at": "2026-02-24T17:45:49.275014Z",
          "id": "f2f1ac8d-4359-4fa8-b758-64abc01a3966",
          "input_text": ".cache/axon-rust/output/domains/platejs.org/sync/markdown",
          "result_json": {
            "chunks_embedded": 1,
            "collection": "cortex",
            "docs_embedded": 1,
            "input": ".cache/axon-rust/output/domains/platejs.org/sync/markdown",
            "source": "rust"
          },
          "started_at": "2026-02-24T17:45:49.031628Z",
          "status": "completed",
          "updated_at": "2026-02-24T17:45:49.275014Z"
        },
        {
          "config_json": {
            "collection": "cortex"
          },
          "created_at": "2026-02-24T16:29:43.402134Z",
          "error_text": null,
          "finished_at": "2026-02-24T16:29:48.093821Z",
          "id": "500e1d93-89dc-412d-8c46-15b369daf569",
          "input_text": "docs/sessions/2026-02-24-nextjs-dashboard-implementation.md",
          "result_json": {
            "chunks_embedded": 1,
            "collection": "cortex",
            "docs_embedded": 1,
            "input": "docs/sessions/2026-02-24-nextjs-dashboard-implementation.md",
            "source": "rust"
          },
          "started_at": "2026-02-24T16:29:47.988328Z",
          "status": "completed",
          "updated_at": "2026-02-24T16:29:48.093821Z"
        }
      ],
      "local_extract_jobs": [],
      "local_ingest_jobs": [
        {
          "config_json": {
            "collection": "cortex",
            "source": {
              "sessions_claude": false,
              "sessions_codex": false,
              "sessions_gemini": false,
              "sessions_project": null,
              "source_type": "sessions"
            }
          },
          "created_at": "2026-02-25T06:07:55.022517Z",
          "error_text": "watchdog reclaimed stale running ingest job (idle=360s marker=amqp)",
          "finished_at": "2026-02-25T06:13:55.127875Z",
          "id": "2cdc4d78-b0de-43df-ac9f-96c1daedd0bd",
          "result_json": {
            "_watchdog": {
              "first_seen_stale_at": "2026-02-25T06:12:55.112756906+00:00",
              "observed_updated_at": "2026-02-25T06:07:55.084111+00:00"
            }
          },
          "source_type": "sessions",
          "started_at": "2026-02-25T06:07:55.084111Z",
          "status": "failed",
          "target": "all",
          "updated_at": "2026-02-25T06:13:55.127875Z"
        },
        {
          "config_json": {
            "collection": "cortex",
            "source": {
              "sessions_claude": false,
              "sessions_codex": false,
              "sessions_gemini": false,
              "sessions_project": null,
              "source_type": "sessions"
            }
          },
          "created_at": "2026-02-25T00:18:52.822997Z",
          "error_text": "watchdog reclaimed stale running ingest job (idle=360s marker=amqp)",
          "finished_at": "2026-02-25T00:24:52.984995Z",
          "id": "d4e0e3b4-e526-459d-ab20-0ee235a2200f",
          "result_json": {
            "_watchdog": {
              "first_seen_stale_at": "2026-02-25T00:23:52.872580983+00:00",
              "observed_updated_at": "2026-02-25T00:18:52.855962+00:00"
            }
          },
          "source_type": "sessions",
          "started_at": "2026-02-25T00:18:52.855962Z",
          "status": "failed",
          "target": "all",
          "updated_at": "2026-02-25T00:24:52.984995Z"
        },
        {
          "config_json": {
            "collection": "cortex",
            "source": {
              "source_type": "youtube",
              "target": "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
            }
          },
          "created_at": "2026-02-25T00:18:01.883093Z",
          "error_text": null,
          "finished_at": "2026-02-25T00:18:07.492491Z",
          "id": "5734ba4e-b17e-4c38-b8ae-916fa63d4625",
          "result_json": {
            "chunks_embedded": 1
          },
          "source_type": "youtube",
          "started_at": "2026-02-25T00:18:01.919161Z",
          "status": "completed",
          "target": "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
          "updated_at": "2026-02-25T00:18:07.492491Z"
        },
        {
          "config_json": {
            "collection": "cortex",
            "source": {
              "source_type": "reddit",
              "target": "rust"
            }
          },
          "created_at": "2026-02-25T00:17:15.290946Z",
          "error_text": null,
          "finished_at": "2026-02-25T00:17:18.273771Z",
          "id": "9a1e0f8d-1b69-4995-be77-129e8056299f",
          "result_json": {
            "chunks_embedded": 80
          },
          "source_type": "reddit",
          "started_at": "2026-02-25T00:17:15.329804Z",
          "status": "completed",
          "target": "rust",
          "updated_at": "2026-02-25T00:17:18.273771Z"
        },
        {
          "config_json": {
            "collection": "cortex",
            "source": {
              "include_source": false,
              "repo": "openai/openai-cookbook",
              "source_type": "github"
            }
          },
          "created_at": "2026-02-25T00:11:49.678989Z",
          "error_text": "watchdog reclaimed stale running ingest job (idle=361s marker=amqp)",
          "finished_at": "2026-02-25T00:17:50.817765Z",
          "id": "247ead64-992a-47c9-91e7-d5d14efa8032",
          "result_json": {
            "_watchdog": {
              "first_seen_stale_at": "2026-02-25T00:16:50.807170464+00:00",
              "observed_updated_at": "2026-02-25T00:11:49.717290+00:00"
            }
          },
          "source_type": "github",
          "started_at": "2026-02-25T00:11:49.717290Z",
          "status": "failed",
          "target": "openai/openai-cookbook",
          "updated_at": "2026-02-25T00:17:50.817765Z"
        },
        {
          "config_json": {
            "collection": "cortex",
            "source": {
              "include_source": false,
              "repo": "openai/openai-cookbook",
              "source_type": "github"
            }
          },
          "created_at": "2026-02-25T00:10:23.022951Z",
          "error_text": "watchdog reclaimed stale running ingest job (idle=387s marker=amqp)",
          "finished_at": "2026-02-25T00:16:50.804357Z",
          "id": "ccf028f6-3c32-4d6b-a3b6-2ac715014ac3",
          "result_json": {
            "_watchdog": {
              "first_seen_stale_at": "2026-02-25T00:15:50.784657240+00:00",
              "observed_updated_at": "2026-02-25T00:10:23.075175+00:00"
            }
          },
          "source_type": "github",
          "started_at": "2026-02-25T00:10:23.075175Z",
          "status": "failed",
          "target": "openai/openai-cookbook",
          "updated_at": "2026-02-25T00:16:50.804357Z"
        },
        {
          "config_json": {
            "collection": "cortex",
            "source": {
              "source_type": "youtube",
              "target": "https://www.youtube.com/watch?v=lEupiLZIgpE&list=PL6MCtOroZNDCr7TgKEgYDD_5WPV75M2If"
            }
          },
          "created_at": "2026-02-24T04:53:49.108503Z",
          "error_text": null,
          "finished_at": "2026-02-24T04:53:52.104950Z",
          "id": "96a2ce89-d6bc-4bf0-9814-3828b9385017",
          "result_json": {
            "chunks_embedded": 5
          },
          "source_type": "youtube",
          "started_at": "2026-02-24T04:53:49.141503Z",
          "status": "completed",
          "target": "https://www.youtube.com/watch?v=lEupiLZIgpE&list=PL6MCtOroZNDCr7TgKEgYDD_5WPV75M2If",
          "updated_at": "2026-02-24T04:53:52.104950Z"
        },
        {
          "config_json": {
            "collection": "cortex",
            "source": {
              "include_source": false,
              "repo": "modelcontextprotocol/rust-sdk",
              "source_type": "github"
            }
          },
          "created_at": "2026-02-24T04:19:12.202364Z",
          "error_text": null,
          "finished_at": "2026-02-24T04:20:01.783259Z",
          "id": "eff4c1e0-2ccb-405e-8dfd-52c0940c60a3",
          "result_json": {
            "chunks_embedded": 1151
          },
          "source_type": "github",
          "started_at": "2026-02-24T04:19:12.252681Z",
          "status": "completed",
          "target": "modelcontextprotocol/rust-sdk",
          "updated_at": "2026-02-24T04:20:01.783259Z"
        },
        {
          "config_json": {
            "collection": "cortex",
            "source": {
              "source_type": "youtube",
              "target": "https://www.youtube.com/watch?v=lEupiLZIgpE&list=PL6MCtOroZNDCr7TgKEgYDD_5WPV75M2If"
            }
          },
          "created_at": "2026-02-23T19:51:33.185468Z",
          "error_text": "yt-dlp not found or failed to start: No such file or directory (os error 2)",
          "finished_at": "2026-02-23T19:51:33.227240Z",
          "id": "a85390f0-63d4-40a9-bd6a-7d0ce43af467",
          "result_json": null,
          "source_type": "youtube",
          "started_at": "2026-02-23T19:51:33.221681Z",
          "status": "failed",
          "target": "https://www.youtube.com/watch?v=lEupiLZIgpE&list=PL6MCtOroZNDCr7TgKEgYDD_5WPV75M2If",
          "updated_at": "2026-02-23T19:51:33.227240Z"
        },
        {
          "config_json": {
            "collection": "cortex",
            "source": {
              "source_type": "youtube",
              "target": "https://www.youtube.com/watch?v=lEupiLZIgpE&list=PL6MCtOroZNDCr7TgKEgYDD_5WPV75M2If"
            }
          },
          "created_at": "2026-02-23T19:28:34.943625Z",
          "error_text": "yt-dlp not found or failed to start: No such file or directory (os error 2)",
          "finished_at": "2026-02-23T19:28:34.997095Z",
          "id": "0833fa95-b685-484e-ad96-86cb77ebfba5",
          "result_json": null,
          "source_type": "youtube",
          "started_at": "2026-02-23T19:28:34.983253Z",
          "status": "failed",
          "target": "https://www.youtube.com/watch?v=lEupiLZIgpE&list=PL6MCtOroZNDCr7TgKEgYDD_5WPV75M2If",
          "updated_at": "2026-02-23T19:28:34.997095Z"
        }
      ]
    },
    "text": "Job Status\n  Crawl  ✓ 15 ✗ 1 ⚠ 1    Embed  ✓ 19 ✗ 1    Ingest  ✓ 4 ✗ 6    Extract  0\n\n✗ Crawls\n  ✓ https://platejs.org/ | 173/173 pages | 13 filtered | 7.5% thin | (20h ago) | 6906b63a-74b6-43ed-9cfa-5f18aae33971\n  ✗ https://tailscale.com/docs | (18h ago) | 8fe25fd2-88dc-472b-88a6-5b8b7c8e8faa\n       ↳ watchdog reclaimed stale running crawl job (idle=392s marker…\n  ⚠ canceled https://gofastmcp.com/ | (23h ago) | b126da5d-553c-4ac7-9a5f-c71062a86138\n  ✓ https://gofastmco.com/ | 0/0 pages | 1 filtered | 0.0% thin | (23h ago) | 03cbb6f9-76e8-47ec-87fa-32df3e6e8fc1\n  ✓ https://code.claude.com/ | 1070/1070 pages | 320 filtered | 29.9% thin | (23h ago) | a103cad5-fbc3-42b0-9ea0-da98ca796891\n\n✗ Embeds\n  ✓ /home/jmagar/workspace/axon_rust/docs/sessions/2026-02-25-pr-review-lint-debug-session.md | 1 docs | 1 chunks | cortex | (17h ago) | 1b1b7c60-30a3-4e9a-8a03-792e884fc5a5\n  ✓ docs/sessions/2026-02-25-neural-canvas-seam-debug.md | 1 docs | 1 chunks | cortex | (17h ago) | ae00e1f8-d0da-4ae9-83ac-fae5e691d230\n  ✓ docs/sessions/2026-02-25-neural-canvas-presets-settings-cog.md | 1 docs | 1 chunks | cortex | (18h ago) | 34b5dd84-983a-475c-8e8a-b39a324b98f7\n  ✓ docs/sessions/2026-02-25-pulse-mode-phase-1-foundation-session.md | 1 docs | 1 chunks | cortex | (18h ago) | 46adeddd-6c46-4ca1-99d0-03b5e679777c\n  ✓ https://platejs.org/ | 173 docs | 1027 chunks | cortex | (20h ago) | 1dc1ab8d-8d79-4be4-94c8-f5abf55a2c61\n\n✗ Ingests\n  ✗ sessions: all | cortex | (17h ago) | 2cdc4d78-b0de-43df-ac9f-96c1daedd0bd\n       ↳ watchdog reclaimed stale running ingest job (idle=360s marke…\n  ✗ sessions: all | cortex | (23h ago) | d4e0e3b4-e526-459d-ab20-0ee235a2200f\n       ↳ watchdog reclaimed stale running ingest job (idle=360s marke…\n  ✓ youtube: https://www.youtube.com/watch?v=dQw4w9WgXcQ | 1 chunks | cortex | (23h ago) | 5734ba4e-b17e-4c38-b8ae-916fa63d4625\n  ✓ reddit: rust | 80 chunks | cortex | (23h ago) | 9a1e0f8d-1b69-4995-be77-129e8056299f\n  ✗ github: openai/openai-cookbook | cortex | (23h ago) | 247ead64-992a-47c9-91e7-d5d14efa8032\n       ↳ watchdog reclaimed stale running ingest job (idle=361s marke…\n\n✓ Extracts\n  None.\n"
  }
}

[exit_code] 0
```

## help

```bash
mcporter --config config/mcporter.json call axon.axon action:help response_mode:inline --output json
```

```text
{
  "ok": true,
  "action": "help",
  "subaction": "run",
  "data": {
    "artifact": {
      "bytes": 1166,
      "line_count": 83,
      "path": ".cache/axon-mcp/help-actions.json",
      "preview": "{\n  \"actions\": {\n    \"artifacts\": [\n      \"head\",\n      \"grep\",\n      \"wc\",\n      \"read\"\n    ],\n    \"ask\": [\n      \"run\"\n    ],\n    \"crawl\": [\n      \"start\",\n      \"status\",\n      \"cancel\",\n      \"list\",\n      \"cleanup\",\n      \"clear\",\n      \"recover\"\n    ],\n    \"discover\": [\n      \"scrape\",\n      \"map\",\n      \"search\"\n    ],\n    \"embed\": [\n      \"start\",\n      \"status\",\n      \"cancel\",\n      \"list\",\n      \"cleanup\",\n      \"clear\",\n      \"recover\"\n    ],\n    \"extract\": [\n      \"start\",\n      \"status\",\n      \"cancel\",\n      \"list\",\n      \"cleanup\",\n      \"clear\",\n      \"recover\"\n    ],\n    \"hel",
      "preview_truncated": true,
      "sha256": "c09ab22edea48a72bc1bee21eef774ad55951eb16cc6464d0c76ed7b04311b70"
    },
    "inline": {
      "actions": {
        "artifacts": [
          "head",
          "grep",
          "wc",
          "read"
        ],
        "ask": [
          "run"
        ],
        "crawl": [
          "start",
          "status",
          "cancel",
          "list",
          "cleanup",
          "clear",
          "recover"
        ],
        "discover": [
          "scrape",
          "map",
          "search"
        ],
        "embed": [
          "start",
          "status",
          "cancel",
          "list",
          "cleanup",
          "clear",
          "recover"
        ],
        "extract": [
          "start",
          "status",
          "cancel",
          "list",
          "cleanup",
          "clear",
          "recover"
        ],
        "help": [],
        "ingest": [
          "start",
          "status",
          "cancel",
          "list",
          "cleanup",
          "clear",
          "recover"
        ],
        "ops": [
          "doctor",
          "domains",
          "sources",
          "stats"
        ],
        "rag": [
          "query",
          "retrieve"
        ],
        "research": [
          "run"
        ],
        "scrape": [
          "run"
        ],
        "screenshot": [
          "run"
        ],
        "status": []
      },
      "defaults": {
        "artifact_dir": ".cache/axon-mcp",
        "response_mode": "path"
      },
      "resources": [
        "axon://schema/mcp-tool"
      ],
      "tool": "axon"
    },
    "response_mode": "inline",
    "truncated": false
  }
}

[exit_code] 0
```

## crawl.list

```bash
mcporter --config config/mcporter.json call axon.axon action:crawl subaction:list limit:2 offset:0 --output json
```

```text
{
  "ok": true,
  "action": "crawl",
  "subaction": "list",
  "data": {
    "artifact": {
      "bytes": 2456,
      "line_count": 72,
      "path": ".cache/axon-mcp/crawl-list.json",
      "preview": "{\n  \"jobs\": [\n    {\n      \"created_at\": \"2026-02-25T02:38:02.301698Z\",\n      \"error_text\": null,\n      \"finished_at\": \"2026-02-25T02:38:35.913176Z\",\n      \"id\": \"6906b63a-74b6-43ed-9cfa-5f18aae33971\",\n      \"result_json\": {\n        \"audit_diff\": {\n          \"added_count\": 173,\n          \"cache_hit\": false,\n          \"cache_source\": null,\n          \"current_count\": 173,\n          \"previous_count\": 0,\n          \"removed_count\": 0,\n          \"start_url\": \"https://platejs.org/\",\n          \"unchanged_count\": 0\n        },\n        \"audit_report_path\": \".cache/axon-rust/output/domains/platejs.org/6906",
      "preview_truncated": true,
      "sha256": "483b94d9676bdacf4f567ae18c5f01d932777f3c2b61326fa831c9f50dfe5955"
    },
    "response_mode": "path",
    "status": "saved"
  }
}

[exit_code] 0
```

## extract.list

```bash
mcporter --config config/mcporter.json call axon.axon action:extract subaction:list limit:2 offset:0 --output json
```

```text
{
  "ok": true,
  "action": "extract",
  "subaction": "list",
  "data": {
    "jobs": [],
    "limit": 2,
    "offset": 0
  }
}

[exit_code] 0
```

## embed.list

```bash
mcporter --config config/mcporter.json call axon.axon action:embed subaction:list limit:2 offset:0 --output json
```

```text
{
  "ok": true,
  "action": "embed",
  "subaction": "list",
  "data": {
    "jobs": [
      {
        "config_json": {
          "collection": "cortex"
        },
        "created_at": "2026-02-25T06:22:02.549251Z",
        "error_text": null,
        "finished_at": "2026-02-25T06:22:02.706731Z",
        "id": "1b1b7c60-30a3-4e9a-8a03-792e884fc5a5",
        "input_text": "/home/jmagar/workspace/axon_rust/docs/sessions/2026-02-25-pr-review-lint-debug-session.md",
        "result_json": {
          "chunks_embedded": 1,
          "collection": "cortex",
          "docs_embedded": 1,
          "input": "/home/jmagar/workspace/axon_rust/docs/sessions/2026-02-25-pr-review-lint-debug-session.md",
          "source": "rust"
        },
        "started_at": "2026-02-25T06:22:02.585315Z",
        "status": "completed",
        "updated_at": "2026-02-25T06:22:02.706731Z"
      },
      {
        "config_json": {
          "collection": "cortex"
        },
        "created_at": "2026-02-25T06:13:45.248727Z",
        "error_text": null,
        "finished_at": "2026-02-25T06:13:45.409584Z",
        "id": "ae00e1f8-d0da-4ae9-83ac-fae5e691d230",
        "input_text": "docs/sessions/2026-02-25-neural-canvas-seam-debug.md",
        "result_json": {
          "chunks_embedded": 1,
          "collection": "cortex",
          "docs_embedded": 1,
          "input": "docs/sessions/2026-02-25-neural-canvas-seam-debug.md",
          "source": "rust"
        },
        "started_at": "2026-02-25T06:13:45.291148Z",
        "status": "completed",
        "updated_at": "2026-02-25T06:13:45.409584Z"
      }
    ],
    "limit": 2,
    "offset": 0
  }
}

[exit_code] 0
```

## ingest.list

```bash
mcporter --config config/mcporter.json call axon.axon action:ingest subaction:list limit:2 offset:0 --output json
```

```text
{
  "ok": true,
  "action": "ingest",
  "subaction": "list",
  "data": {
    "jobs": [
      {
        "config_json": {
          "collection": "cortex",
          "source": {
            "sessions_claude": false,
            "sessions_codex": false,
            "sessions_gemini": false,
            "sessions_project": null,
            "source_type": "sessions"
          }
        },
        "created_at": "2026-02-25T06:07:55.022517Z",
        "error_text": "watchdog reclaimed stale running ingest job (idle=360s marker=amqp)",
        "finished_at": "2026-02-25T06:13:55.127875Z",
        "id": "2cdc4d78-b0de-43df-ac9f-96c1daedd0bd",
        "result_json": {
          "_watchdog": {
            "first_seen_stale_at": "2026-02-25T06:12:55.112756906+00:00",
            "observed_updated_at": "2026-02-25T06:07:55.084111+00:00"
          }
        },
        "source_type": "sessions",
        "started_at": "2026-02-25T06:07:55.084111Z",
        "status": "failed",
        "target": "all",
        "updated_at": "2026-02-25T06:13:55.127875Z"
      },
      {
        "config_json": {
          "collection": "cortex",
          "source": {
            "sessions_claude": false,
            "sessions_codex": false,
            "sessions_gemini": false,
            "sessions_project": null,
            "source_type": "sessions"
          }
        },
        "created_at": "2026-02-25T00:18:52.822997Z",
        "error_text": "watchdog reclaimed stale running ingest job (idle=360s marker=amqp)",
        "finished_at": "2026-02-25T00:24:52.984995Z",
        "id": "d4e0e3b4-e526-459d-ab20-0ee235a2200f",
        "result_json": {
          "_watchdog": {
            "first_seen_stale_at": "2026-02-25T00:23:52.872580983+00:00",
            "observed_updated_at": "2026-02-25T00:18:52.855962+00:00"
          }
        },
        "source_type": "sessions",
        "started_at": "2026-02-25T00:18:52.855962Z",
        "status": "failed",
        "target": "all",
        "updated_at": "2026-02-25T00:24:52.984995Z"
      }
    ],
    "limit": 2,
    "offset": 0
  }
}

[exit_code] 0
```

## rag.query

```bash
mcporter --config config/mcporter.json call axon.axon action:rag subaction:query query:rust limit:2 offset:0 --output json
```

```text
{
  "ok": true,
  "action": "rag",
  "subaction": "query",
  "data": {
    "artifact": {
      "bytes": 606,
      "line_count": 19,
      "path": ".cache/axon-mcp/rag-query-rust.json",
      "preview": "{\n  \"limit\": 2,\n  \"offset\": 0,\n  \"query\": \"rust\",\n  \"results\": [\n    {\n      \"chunk_index\": 23,\n      \"score\": 0.5686403,\n      \"snippet\": \"![RustWeek](https://zed.dev/_next/image?url=%2F_next%2Fstatic%2Fmedia%2Frustweek-booth.e75ba68e.webp&w=3840&q=75)  RustWeekUtrecht, The Neth\",\n      \"url\": \"https://zed.dev/2025\"\n    },\n    {\n      \"chunk_index\": 31,\n      \"score\": 0.5670138,\n      \"snippet\": \"*   **Rust**       (requires [rustup](https://rustup.rs/)      - uses rust-analyzer from your toolchain)\",\n      \"url\": \"https://oraios.github.io/serena/01-about/020_programming-languages.html\"\n    }",
      "preview_truncated": true,
      "sha256": "c55e1b2303976506c61bcefd669b51cf78700abe902e9d4c32bb6cf90dc4214a"
    },
    "response_mode": "path",
    "status": "saved"
  }
}

[exit_code] 0
```

## discover.search

```bash
mcporter --config config/mcporter.json call axon.axon action:discover subaction:search query:'rust mcp' limit:2 offset:0 --output json
```

```text
{
  "ok": true,
  "action": "discover",
  "subaction": "search",
  "data": {
    "artifact": {
      "bytes": 1879,
      "line_count": 19,
      "path": ".cache/axon-mcp/discover-search-rust-mcp.json",
      "preview": "{\n  \"limit\": 2,\n  \"offset\": 0,\n  \"query\": \"rust mcp\",\n  \"results\": [\n    {\n      \"position\": 1,\n      \"snippet\": \"# Rust MCP Server. By exposing local tools and project context to the LLM, rust-mcp-server allows the model to perform actions on your behalf, such as building, testing, and analyzing your Rust code. * Why use `rust-mcp-server`? ## Why use `rust-mcp-server`? Integrating an LLM with your local development environment via rust-mcp-server can significantly enhance your productivity. * **Apply Rust best practices**: Use `cargo clippy` to lint your code and catch common mistakes, ensuri",
      "preview_truncated": true,
      "sha256": "353d9fbea48d8dc4d21c89dba6f812e350fe443f06ccac1129be74fc5cb33745"
    },
    "response_mode": "path",
    "status": "saved"
  }
}

[exit_code] 0
```

## ops.doctor

```bash
mcporter --config config/mcporter.json call axon.axon action:ops subaction:doctor --output json
```

```text
{
  "ok": true,
  "action": "ops",
  "subaction": "doctor",
  "data": {
    "amqp_configured": true,
    "llm_ok": false,
    "pg_configured": true,
    "qdrant_ok": true,
    "redis_configured": true,
    "tei_ok": true
  }
}

[exit_code] 0
```

## artifacts.head

```bash
mcporter --config config/mcporter.json call axon.axon action:artifacts subaction:head path:config/.cache/axon-mcp/help-actions.json limit:20 --output json
```

```text
{
  "server": "axon",
  "tool": "axon",
  "error": "MCP error -32602: artifact path not found: No such file or directory (os error 2)",
  "issue": {
    "kind": "other",
    "rawMessage": "MCP error -32602: artifact path not found: No such file or directory (os error 2)"
  }
}

[exit_code] 0
```

## discover.scrape

```bash
mcporter --config config/mcporter.json call axon.axon action:discover subaction:scrape url:https://example.com --output json
```

```text
{
  "ok": true,
  "action": "discover",
  "subaction": "scrape",
  "data": {
    "artifact": {
      "bytes": 285,
      "line_count": 6,
      "path": ".cache/axon-mcp/discover-scrape-https-example-com.json",
      "preview": "{\n  \"description\": \"\",\n  \"markdown\": \"Example Domain\\n# Example Domain\\nThis domain is for use in documentation examples without needing permission. Avoid use in operations.\\n[Learn more](https://iana.org/domains/example)\",\n  \"title\": \"Example Domain\",\n  \"url\": \"https://example.com\"\n}",
      "preview_truncated": false,
      "sha256": "298c382cc8ab0e039c328a272daf43c499cdea4b26c68d2ea3aaef6bfa085527"
    },
    "response_mode": "path",
    "status": "saved"
  }
}

[exit_code] 0
```

## discover.map

```bash
mcporter --config config/mcporter.json call axon.axon action:discover subaction:map url:https://example.com limit:5 offset:0 --output json
```

```text
{
  "ok": true,
  "action": "discover",
  "subaction": "map",
  "data": {
    "artifact": {
      "bytes": 166,
      "line_count": 11,
      "path": ".cache/axon-mcp/discover-map-https-example-com.json",
      "preview": "{\n  \"elapsed_ms\": 235,\n  \"limit\": 5,\n  \"offset\": 0,\n  \"pages_seen\": 1,\n  \"total_urls\": 1,\n  \"url\": \"https://example.com\",\n  \"urls\": [\n    \"https://example.com/\"\n  ]\n}",
      "preview_truncated": false,
      "sha256": "08356861aed962fc5d6a0e55c9faab30c99097d839db987099dcdd85f79e89bc"
    },
    "response_mode": "path",
    "status": "saved"
  }
}

[exit_code] 0
```

## rag.retrieve

```bash
mcporter --config config/mcporter.json call axon.axon action:rag subaction:retrieve url:https://example.com limit:2 offset:0 --output json
```

```text
{
  "ok": true,
  "action": "rag",
  "subaction": "retrieve",
  "data": {
    "artifact": {
      "bytes": 249,
      "line_count": 5,
      "path": ".cache/axon-mcp/rag-retrieve-https-example-com.json",
      "preview": "{\n  \"chunks\": 1,\n  \"content\": \"Example Domain\\n# Example Domain\\nThis domain is for use in documentation examples without needing permission. Avoid use in operations.\\n[Learn more](https://iana.org/domains/example)\",\n  \"url\": \"https://example.com\"\n}",
      "preview_truncated": false,
      "sha256": "1107889cbb242ebd8b2ce686b0f0684b77f9aac2f5e5361493f00c845f05c05c"
    },
    "response_mode": "path",
    "status": "saved"
  }
}

[exit_code] 0
```

## scrape

```bash
mcporter --config config/mcporter.json call axon.axon action:scrape url:https://example.com --output json
```

```text
{
  "ok": true,
  "action": "scrape",
  "subaction": "run",
  "data": {
    "artifact": {
      "bytes": 285,
      "line_count": 6,
      "path": ".cache/axon-mcp/scrape-https-example-com.json",
      "preview": "{\n  \"description\": \"\",\n  \"markdown\": \"Example Domain\\n# Example Domain\\nThis domain is for use in documentation examples without needing permission. Avoid use in operations.\\n[Learn more](https://iana.org/domains/example)\",\n  \"title\": \"Example Domain\",\n  \"url\": \"https://example.com\"\n}",
      "preview_truncated": false,
      "sha256": "298c382cc8ab0e039c328a272daf43c499cdea4b26c68d2ea3aaef6bfa085527"
    },
    "response_mode": "path",
    "status": "saved"
  }
}

[exit_code] 0
```

## research

```bash
mcporter --config config/mcporter.json call axon.axon action:research query:'rust mcp' limit:2 offset:0 --output json
```

```text
{
  "ok": true,
  "action": "research",
  "subaction": "run",
  "data": {
    "artifact": {
      "bytes": 6614,
      "line_count": 93,
      "path": ".cache/axon-mcp/research-rust-mcp.json",
      "preview": "{\n  \"extractions\": [\n    {\n      \"extracted\": {\n        \"contextual_note\": \"The provided HTML is a policy notice from the Rust package registry (crates.io) regarding API usage, which is critical for any MCP tool designed to fetch Rust crate metadata.\",\n        \"key_facts\": [\n          {\n            \"detail\": \"All requests to crates.io must include a 'User-Agent' header.\",\n            \"requirement\": \"Mandatory User-Agent Header\"\n          },\n          {\n            \"detail\": \"The User-Agent must identify the specific bot or application, not just the HTTP client library being used (e.g., 'reqwes",
      "preview_truncated": true,
      "sha256": "e31d57a95dd4032df3cf7d758c671247e307df8aa3c54e91dafa81b417f21d62"
    },
    "response_mode": "path",
    "status": "saved"
  }
}

[exit_code] 0
```

## ask

```bash
mcporter --config config/mcporter.json call axon.axon action:ask query:'what is mcp' --output json
```

```text
{
  "ok": true,
  "action": "ask",
  "subaction": "run",
  "data": {
    "artifact": {
      "bytes": 1414,
      "line_count": 9,
      "path": ".cache/axon-mcp/ask-what-is-mcp.json",
      "preview": "{\n  \"answer\": \"MCP stands for **Model Context Protocol** [S7, S10]. It is a protocol used to provide context to AI models and is integrated into various platforms such as Claude Desktop, Claude Code, and GitHub Copilot [S1, S2, S10].\\n\\nKey aspects of MCP include:\\n*   **Local Servers:** Users can set up local MCP servers to extend the capabilities of AI tools like Claude Desktop [S1].\\n*   **Integration:** It is used by various services, such as Exa, to provide additional reference context [S3, S4].\\n*   **Development:** It is available as a software dependency (e.g., via PyPI) for developers",
      "preview_truncated": true,
      "sha256": "f7e4928a3fb8a470118d149c3855697687f65efa8fbb321a68b755c403e79555"
    },
    "response_mode": "path",
    "status": "saved"
  }
}

[exit_code] 0
```

## screenshot

```bash
mcporter --config config/mcporter.json call axon.axon action:screenshot url:https://example.com full_page:false --output json
```

```text
{
  "ok": true,
  "action": "screenshot",
  "subaction": "run",
  "data": {
    "full_page": false,
    "path": ".cache/axon-mcp/screenshots/0001-example-com.png",
    "size_bytes": 22268,
    "url": "https://example.com",
    "viewport": "1920x1080"
  }
}

[exit_code] 0
```

## resource.list

```bash
mcporter --config config/mcporter.json call axon.axon action:help response_mode:inline --output json | jq '.data.inline.resources'
```

```text
[
  "axon://schema/mcp-tool"
]

[exit_code] 0
```

## resource.read

```bash
mcporter --config config/mcporter.json call axon.axon action:help response_mode:inline --output json | jq -r '.data.inline.resources[0]' | xargs -I{} sh -lc 'echo Resource URI: {}; mcporter --config config/mcporter.json list axon --schema'
```

```text
Resource URI: axon://schema/mcp-tool
axon

  /**
   * Unified Axon MCP tool. Use action/subaction routing. Actions: status, help, crawl, extract, embed,
   * ingest, rag, discover, ops, artifacts, scrape, research, ask, screenshot.
   */
  function axon();
      {
        "type": "object",
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "additionalProperties": true,
        "title": "Map_of_AnyValue"
      }

  Examples:
    mcporter call axon.axon()

  1 tool · 16ms · STDIO /home/jmagar/workspace/axon_rust/target/debug/axon-mcp


[exit_code] 0
```
