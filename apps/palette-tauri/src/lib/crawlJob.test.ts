import { describe, expect, it } from "vitest";

import { formatEta, hostFromUrl, summarizeCrawl } from "./crawlJob";

describe("hostFromUrl", () => {
  it("extracts and de-wwws the host", () => {
    expect(hostFromUrl("https://docs.rs/spider/latest/")).toBe("docs.rs");
    expect(hostFromUrl("https://www.example.com/a")).toBe("example.com");
  });
  it("falls back gracefully on garbage", () => {
    expect(hostFromUrl("not a url")).toBe("not a url");
  });
});

describe("formatEta", () => {
  it("returns minutes for the mock's rate", () => {
    // 187 queued at ~0.24 pages/sec ≈ 13 minutes — matches the mock.
    expect(formatEta(187, 187 / (13 * 60))).toBe("est. 13 min left");
  });
  it("returns seconds under a minute", () => {
    expect(formatEta(10, 1)).toBe("est. 10s left");
  });
  it("returns null when nothing is queued", () => {
    expect(formatEta(0, 1)).toBeNull();
  });
});

describe("summarizeCrawl", () => {
  const url = "https://docs.rs/spider/latest/spider/";

  it("derives the mock's headline numbers from real result_json", () => {
    const snap = summarizeCrawl(
      {
        job: {
          status: "running",
          result_json: {
            pages_crawled: 39,
            md_created: 38,
            queued: 187,
            error_pages: 0,
            depth_current: 2,
            depth_max: 4,
          },
        },
      },
      { jobId: "job-1", url, maxDepth: 4, elapsedSec: 162 },
    );

    expect(snap.fetched).toBe(39);
    expect(snap.queued).toBe(187);
    expect(snap.docs).toBe(38);
    expect(snap.depthCurrent).toBe(2);
    expect(snap.depthMax).toBe(4);
    expect(Math.round(snap.percent)).toBe(17); // 39 / (39 + 187)
    expect(snap.phase).toBe("crawling");
    expect(snap.etaText).toMatch(/min left$/);
  });

  it("maps pending status before any progress", () => {
    const snap = summarizeCrawl({ job: { status: "pending" } }, { jobId: "j", url });
    expect(snap.phase).toBe("pending");
    expect(snap.fetched).toBe(0);
    expect(snap.percent).toBe(0);
  });

  it("folds the embed job in as the second phase", () => {
    const crawl = {
      job: { status: "completed", result_json: { pages_crawled: 40, md_created: 40, embed_job_id: "e1" } },
    };
    const embed = { job: { status: "running", result_json: { docs_embedded: 12, chunks_embedded: 280 } } };
    const snap = summarizeCrawl(crawl, { jobId: "j", url }, embed);
    expect(snap.phase).toBe("embedding");
    expect(snap.embedded).toBe(12);
    expect(snap.chunks).toBe(280);
    expect(snap.embedJobId).toBe("e1");
  });

  it("completes at 100% when crawl done and embed finished", () => {
    const crawl = { job: { status: "completed", result_json: { pages_crawled: 40, md_created: 40, embed_job_id: "e1" } } };
    const embed = { job: { status: "completed", result_json: { docs_embedded: 40, chunks_embedded: 920 } } };
    const snap = summarizeCrawl(crawl, { jobId: "j", url }, embed);
    expect(snap.phase).toBe("done");
    expect(snap.percent).toBe(100);
    expect(snap.embedded).toBe(40);
  });

  it("parses per-page events and rate-limit hosts from the event stream", () => {
    const snap = summarizeCrawl(
      {
        job: {
          status: "running",
          result_json: {
            pages_crawled: 2,
            events: [
              { t: 610, kind: "fetch", url: "https://docs.rs/serde", status: 200, links: 9 },
              { t: 1240, kind: "warn", url: "https://docs.rs/releases", status: 429, text: "backing off 2s" },
              { t: 1660, kind: "embed", batch: 1, chunks: 18 },
            ],
            rate_limited: [{ host: "docs.rs", backoff_ms: 2000 }],
          },
        },
      },
      { jobId: "j", url },
    );
    expect(snap.events).toHaveLength(3);
    expect(snap.events[0]).toMatchObject({ kind: "fetch", status: 200, links: 9 });
    expect(snap.events[1]).toMatchObject({ kind: "warn", status: 429 });
    expect(snap.events[2]).toMatchObject({ kind: "embed", batch: 1, chunks: 18 });
    expect(snap.rateLimited).toEqual([{ host: "docs.rs", backoffMs: 2000 }]);
  });
});
