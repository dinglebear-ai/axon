import { describe, expect, it } from "vitest";

import { ACTIONS } from "@/lib/actions";
import { chatToolMessage } from "@/lib/chatToolActions";

describe("chatToolActions", () => {
  it("formats one-shot tool results for chat", () => {
    const scrape = ACTIONS.find((action) => action.subcommand === "scrape");
    expect(scrape).toBeDefined();

    const message = chatToolMessage(scrape!, "https://example.com", {
      ok: true,
      status: 200,
      method: "POST",
      path: "/v1/scrape",
      payload: { markdown: "# Example\n\nHello from the page.", url: "https://example.com" },
    });

    expect(message).toContain("Scrape completed");
    expect(message).toContain("POST /v1/scrape");
    expect(message).toContain("# Example");
  });

  it("formats queued jobs with the job id", () => {
    const crawl = ACTIONS.find((action) => action.subcommand === "crawl");
    expect(crawl).toBeDefined();

    const message = chatToolMessage(crawl!, "https://example.com/docs", {
      ok: true,
      status: 202,
      method: "POST",
      path: "/v1/crawl",
      payload: { job_id: "job-123", status: "queued" },
    });

    expect(message).toContain("Crawl queued");
    expect(message).toContain("Job id: `job-123`");
    expect(message).toContain("Status: queued");
  });
});
