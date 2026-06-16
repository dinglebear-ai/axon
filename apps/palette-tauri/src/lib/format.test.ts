import { describe, expect, it } from "vitest";

import { MIN_PROGRESS_PCT, formatPayload } from "./format";

describe("MIN_PROGRESS_PCT", () => {
  // The constant exists to floor the rendered bar width so a just-started job still
  // shows a visible sliver: `Math.max(MIN_PROGRESS_PCT, pct)` (App.tsx, CrawlJobView).
  it("floors a 0% bar to a visible width but never inflates a real percentage", () => {
    expect(Math.max(MIN_PROGRESS_PCT, 0)).toBeGreaterThan(0);
    expect(Math.max(MIN_PROGRESS_PCT, 50)).toBe(50);
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

  it("formats screenshot artifact metadata without exposing absolute server paths", () => {
    const output = formatPayload("screenshot", {
      url: "https://example.com",
      path: "/home/axon/.axon/output/screenshots/example.png",
      size_bytes: 1024,
      artifact_handle: {
        relative_path: "screenshots/example.png",
        display_path: "screenshots/example.png",
        kind: "screenshot",
        bytes: 1024,
      },
    });

    expect(output).toContain("artifact: screenshots/example.png");
    expect(output).not.toContain("path:");
    expect(output).not.toContain("/home/axon/.axon/output/screenshots/example.png");
  });
});
