// @vitest-environment jsdom
// @ts-expect-error Vitest runs this file in Node; the app tsconfig intentionally omits Node globals.
import { readFileSync } from "node:fs";

import "@testing-library/jest-dom/vitest";
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { ACTIONS } from "@/lib/actions";
import { buildHelpRun } from "@/lib/actionHelp";
import { hasStructuredOperationView, OperationResultView, sanitizeReaderMarkdown } from "./OperationResultView";

function action(subcommand: string) {
  const found = ACTIONS.find((candidate) => candidate.subcommand === subcommand);
  if (!found) throw new Error(`missing action ${subcommand}`);
  return found;
}

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

  it("renders action help as a structured help view", () => {
    const run = buildHelpRun(action("scrape"));
    render(<OperationResultView payload={run.result.payload} subcommand="help" />);
    expect(screen.getByRole("heading", { name: "Scrape URL" })).toBeInTheDocument();
    expect(screen.getByText("POST /v1/scrape")).toBeInTheDocument();
    expect(screen.getByText("Parameters")).toBeInTheDocument();
  });

  it("falls back to markdown text for historical help entries without payloads", () => {
    render(<OperationResultView payload={null} subcommand="help" fallbackText="# Scrape URL\n\nRoute: `POST /v1/scrape`" />);
    expect(screen.getByText(/# Scrape URL/)).toBeInTheDocument();
  });

  it("renders malformed help payloads as a visible error when no fallback text exists", () => {
    render(<OperationResultView payload={{ target: { title: "bad" } }} subcommand="help" fallbackText="" />);
    expect(screen.getByRole("alert")).toHaveTextContent("Help payload is malformed");
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

  it("lets scraped document readers use the full output panel height", () => {
    const styles = readFileSync("src/styles.css", "utf8");

    expect(styles).toContain(".operation-reader-view");
    expect(styles).toContain(".operation-reader-section");
    expect(styles).toContain("max-height: none");
    expect(styles).not.toContain("max-height: min(48vh, 560px)");
  });
});
