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

  // L4: the allowlist and the dispatch map are now derived from one source, so
  // every claimed subcommand must actually route to a structured (non-generic) view.
  it("keeps the allowlist and the renderer map in sync", () => {
    const structured = [
      "query",
      "scrape",
      "search",
      "research",
      "map",
      "suggest",
      "sources",
      "domains",
      "doctor",
      "crawl",
      "embed",
      "extract",
      "ingest",
      "endpoints",
      "brand",
      "diff",
      "screenshot",
      "dedupe",
      "watch-list",
      "watch-create",
      "watch-run",
    ];
    for (const subcommand of structured) {
      expect(hasStructuredOperationView(subcommand), subcommand).toBe(true);
    }
  });
});

// T-M2: render each structured view from a representative payload and assert it
// produced its distinctive structured chrome (a hero/summary metric, a row, a
// figure, …) rather than falling through to the generic JSON dump.
describe("OperationResultView structured rendering", () => {
  it("renders ranked query matches", () => {
    render(
      <OperationResultView
        subcommand="query"
        payload={{ collection: "axon", results: [{ title: "hit one", url: "https://example.com/a", score: 0.91, rank: 1 }] }}
      />,
    );
    expect(screen.getByText("hit one")).toBeInTheDocument();
    expect(screen.getByText("0.910")).toBeInTheDocument();
  });

  it("renders web search results with queued crawl jobs", () => {
    render(
      <OperationResultView
        subcommand="search"
        payload={{
          results: [{ title: "Result A", url: "https://example.com/a", snippet: "snippet", rank: 1 }],
          crawl_jobs: [{ job_id: "job-1", status: "queued", url: "https://example.com/a" }],
        }}
      />,
    );
    expect(screen.getByText("Result A")).toBeInTheDocument();
    expect(screen.getByText("Queued crawl jobs")).toBeInTheDocument();
  });

  it("renders discovered URLs for map", () => {
    render(<OperationResultView subcommand="map" payload={{ urls: ["https://example.com/x"], count: 1 }} />);
    expect(screen.getByText("https://example.com/x")).toBeInTheDocument();
  });

  it("renders a degraded doctor report", () => {
    render(
      <OperationResultView
        subcommand="doctor"
        payload={{ degraded: true, checks: [{ name: "TEI", status: "warn", message: "slow" }] }}
      />,
    );
    expect(screen.getByText("Doctor found issues")).toBeInTheDocument();
    expect(screen.getByText("TEI")).toBeInTheDocument();
  });

  it("renders a job-start hero for async families", () => {
    render(<OperationResultView subcommand="crawl" payload={{ execution_mode: "async", result: { job_id: "abc123def456ghi", status: "queued" } }} />);
    expect(screen.getByText("Crawl job queued")).toBeInTheDocument();
    // shortId truncates ids over 12 chars (canonical, with the ellipsis char).
    expect(screen.getByText("abc123def456…")).toBeInTheDocument();
  });

  it("renders a job-lifecycle list view", () => {
    render(<OperationResultView subcommand="crawl-list" payload={{ jobs: [{ job_id: "j1", status: "running", url: "https://example.com" }] }} />);
    expect(screen.getByText("Crawl List")).toBeInTheDocument();
  });

  it("renders dedupe metrics", () => {
    render(<OperationResultView subcommand="dedupe" payload={{ removed: 4, scanned: 100, collection: "axon" }} />);
    expect(screen.getByText("Dedupe complete")).toBeInTheDocument();
  });

  it("falls back to the generic view for unknown subcommands", () => {
    render(<OperationResultView subcommand="totally-unknown" payload={{ field_one: "value" }} />);
    expect(screen.getByText("Field One")).toBeInTheDocument();
  });
});
