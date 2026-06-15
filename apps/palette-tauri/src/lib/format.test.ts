import { describe, expect, it } from "vitest";

import { MIN_PROGRESS_PCT, formatPayload } from "./format";

describe("MIN_PROGRESS_PCT", () => {
  it("is the shared minimum visible progress-bar width", () => {
    expect(MIN_PROGRESS_PCT).toBe(2);
  });
});

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

  it("returns chat answers without compacting the whole response", () => {
    expect(formatPayload("chat", { answer: "No retrieval used." })).toBe("No retrieval used.");
  });

  it("truncates large diff text payloads", () => {
    const output = formatPayload("diff", {
      status: "changed",
      text_diff: "x".repeat(12_050),
    });

    expect(output.length).toBeLessThan(12_200);
    expect(output).toContain("[truncated 50 chars from text_diff]");
  });
});
