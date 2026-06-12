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

  it("removes empty markdown bullets and dash-only fenced blocks", () => {
    const markdown = [
      "- Real item",
      "-",
      "* ",
      "•",
      "",
      "```txt",
      "-",
      "```",
      "",
      "```rust",
      "let ok = true;",
      "```",
    ].join("\n");

    expect(sanitizeReaderMarkdown(markdown)).toBe(["- Real item", "```rust", "let ok = true;", "```"].join("\n"));
  });

  it("cleans common scrape chrome without touching fenced code", () => {
    const markdown = [
      "Claude Code by Anthropic | AI Coding Agent, Terminal, IDESkip to main content Debugging...",
      "Slack curl -fsSL https://claude.ai/install.sh | bash Or read the documentation Try Claude Code (opens in new tab)Developer docs (opens in new tab)",
      "",
      "```bash",
      "echo 'Skip to main content should stay in code'",
      "```",
      "-",
    ].join("\n");

    expect(sanitizeReaderMarkdown(markdown)).toBe(
      [
        "Claude Code by Anthropic | AI Coding Agent, Terminal, IDE",
        "Slack curl -fsSL https://claude.ai/install.sh | bash Or read the documentation Try Claude Code Developer docs",
        "```bash",
        "echo 'Skip to main content should stay in code'",
        "```",
      ].join("\n"),
    );
  });
});
