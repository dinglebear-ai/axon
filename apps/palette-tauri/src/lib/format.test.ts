import { describe, expect, it } from "vitest";

import { formatPayload, MIN_PROGRESS_PCT } from "./format";

describe("MIN_PROGRESS_PCT", () => {
  // The constant exists to floor the rendered bar width so a just-started job still
  // shows a visible sliver: `Math.max(MIN_PROGRESS_PCT, pct)` (App.tsx, JobProgressView).
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
    const output = formatPayload("source", {
      disposition: "queued",
      execution_mode: "async",
      result: { status: "pending", job_id: "job-123" },
    });

    expect(output).not.toMatch(/^\s*\{/);
    expect(output).toContain("source");
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
      artifact_id: "art_screenshot_123",
      width: 1280,
      height: 720,
      captured_at: "2026-07-16T00:00:00Z",
      warnings: [],
    });

    expect(output).toContain("artifact: art_screenshot_123");
    expect(output).not.toContain("path:");
  });

  it("formats representative payloads for every moved formatter", () => {
    const cases: Array<[string, unknown, string[]]> = [
      ["scrape", { markdown: "# Page body" }, ["# Page body"]],
      ["retrieve", { content: "stored chunk" }, ["stored chunk"]],
      ["map", { urls: ["https://example.com/a"] }, ["https://example.com/a"]],
      [
        "query",
        { results: [{ rank: 1, score: 0.925, url: "https://example.com/a", snippet: "hit" }] },
        ["1. score 0.925", "hit"],
      ],
      [
        "search",
        { results: [{ title: "Result A", url: "https://example.com/a", snippet: "snippet" }] },
        ["1. Result A", "snippet"],
      ],
      [
        "suggest",
        { suggestions: [{ url: "https://example.com/docs", reason: "Relevant docs" }] },
        ["https://example.com/docs", "Relevant docs"],
      ],
      [
        "sources",
        { count: 1, urls: ["https://example.com/source"] },
        ["1 indexed sources", "https://example.com/source"],
      ],
      ["domains", { domains: [{ domain: "example.com", count: 2 }] }, ["example.com"]],
      [
        "endpoints",
        { total: 1, endpoints: ["https://example.com/api"] },
        ["Endpoint discovery", "1 candidates", "https://example.com/api"],
      ],
      [
        "brand",
        { name: "Aurora", colors: [{ hex: "#29b6f6", usage: "primary", count: 4 }] },
        ["Aurora", "#29b6f6 primary (4)"],
      ],
      [
        "watch-list",
        { watches: [{ name: "Docs", id: "watch-1", enabled: true, every_seconds: 60 }] },
        ["Docs (watch-1)", "enabled: true"],
      ],
      [
        "watch-create",
        { name: "Docs", id: "watch-1", enabled: true, every_seconds: 60 },
        ["Docs (watch-1)", "every: 60s"],
      ],
      [
        "watch-run",
        { watch_id: "watch-1", artifacts: [{ id: "a1" }] },
        ["watch: watch-1", "artifacts: 1"],
      ],
      [
        "jobs-list",
        { jobs: [{ job_id: "job-1", status: "running", url: "https://example.com" }] },
        ["job-1", "status: running"],
      ],
    ];

    for (const [subcommand, payload, fragments] of cases) {
      const output = formatPayload(subcommand, payload);
      for (const fragment of fragments) {
        expect(output, subcommand).toContain(fragment);
      }
    }
  });

  it("keeps string and non-record payload fallbacks stable", () => {
    expect(formatPayload("status", "plain status")).toBe("plain status");
    expect(formatPayload("status", 42)).toBe("42");
    expect(() => formatPayload("serach", { answer: "typo" })).toThrow(
      "Unknown palette action: serach",
    );
  });
});
