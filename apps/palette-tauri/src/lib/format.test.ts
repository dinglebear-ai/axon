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
    const output = formatPayload("ingest", {
      disposition: "queued",
      execution_mode: "async",
      result: { status: "pending", job_id: "job-123" },
    });

    expect(output).not.toMatch(/^\s*\{/);
    expect(output).toContain("ingest");
    expect(output).toContain("queued");
    expect(output).toContain("pending");
    expect(output).toContain("async");
    expect(output).toContain("job-123");
  });

  it("returns ask answers without compacting the whole response", () => {
    expect(formatPayload("ask", { answer: "Use AXON_LLM_BACKEND=openai-compat." })).toBe(
      "Use AXON_LLM_BACKEND=openai-compat.",
    );
  });
});
