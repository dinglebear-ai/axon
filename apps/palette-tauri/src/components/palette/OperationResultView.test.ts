import { describe, expect, it } from "vitest";

import { hasStructuredOperationView, sanitizeReaderMarkdown } from "./OperationResultView";

describe("OperationResultView routing", () => {
  it("claims structured views for JSON-heavy Axon operations", () => {
    for (const subcommand of [
      "query",
      "scrape",
      "search",
      "research",
      "crawl",
      "map",
      "sources",
      "domains",
      "retrieve",
      "doctor",
      "embed",
      "extract",
      "ingest",
      "endpoints",
      "brand",
      "diff",
      "screenshot",
      "dedupe",
      "crawl-status",
      "crawl-clear",
      "embed-list",
      "ingest-recover",
    ]) {
      expect(hasStructuredOperationView(subcommand), subcommand).toBe(true);
    }
  });

  it("leaves content-first markdown operations on the existing Streamdown path", () => {
    for (const subcommand of ["ask", "chat", "summarize"]) {
      expect(hasStructuredOperationView(subcommand), subcommand).toBe(false);
    }
  });

  it("removes empty markdown bullets without changing fenced code", () => {
    const markdown = [
      "- Real item",
      "-",
      "* ",
      "•",
      "",
      "```txt",
      "-",
      "```",
    ].join("\n");

    expect(sanitizeReaderMarkdown(markdown)).toBe(["- Real item", "", "```txt", "-", "```"].join("\n"));
  });
});
