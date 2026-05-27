import { describe, expect, it } from "vitest";

import { formatPayload } from "./format";

describe("formatPayload", () => {
  it("uses nested REST payload summaries for research output", () => {
    expect(
      formatPayload("research", {
        payload: {
          summary: "Hybrid search combines sparse and dense retrieval.",
          results: [{ title: "Raw result" }],
        },
      }),
    ).toBe("Hybrid search combines sparse and dense retrieval.");
  });

  it("formats async job starts as human-readable status lines", () => {
    expect(
      formatPayload("ingest", {
        disposition: "queued",
        execution_mode: "async",
        result: { status: "pending", job_id: "job-123" },
      }),
    ).toBe("ingest queued\nstatus: pending\nmode: async\njob: job-123\nNext: status");
  });

  it("returns ask answers without compacting the whole response", () => {
    expect(formatPayload("ask", { answer: "Use AXON_LLM_BACKEND=openai-compat." })).toBe(
      "Use AXON_LLM_BACKEND=openai-compat.",
    );
  });
});
