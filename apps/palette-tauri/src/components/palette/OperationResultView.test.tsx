// @vitest-environment jsdom
// @ts-expect-error Vitest runs this file in Node; the app tsconfig intentionally omits Node globals.
import { readFileSync } from "node:fs";

import "@testing-library/jest-dom/vitest";
import { act, render, screen } from "@testing-library/react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { buildHelpRun } from "@/lib/actionHelp";
import { ACTIONS } from "@/lib/actions";
import {
  hasStructuredOperationView,
  OperationResultView,
  sanitizeReaderMarkdown,
} from "./OperationResultView";

const mockLoadArtifactObjectUrl = vi.hoisted(() => vi.fn());

vi.mock("@/lib/artifactPreview", () => ({
  loadArtifactObjectUrl: mockLoadArtifactObjectUrl,
}));

function screenshotWithArtifactHandle() {
  return {
    artifact_id: "art_screenshot_123",
    width: 1280,
    height: 720,
    captured_at: "2026-07-16T00:00:00Z",
    warnings: [],
  };
}

function action(subcommand: string) {
  const found = ACTIONS.find((candidate) => candidate.subcommand === subcommand);
  if (!found) throw new Error(`missing action ${subcommand}`);
  return found;
}

describe("OperationResultView routing", () => {
  beforeEach(() => {
    mockLoadArtifactObjectUrl.mockReset();
    mockLoadArtifactObjectUrl.mockResolvedValue("blob:test-shot");
    vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => undefined);
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("claims structured views for JSON-heavy Axon operations", () => {
    for (const subcommand of [
      "query",
      "scrape",
      "search",
      "research",
      "source-site",
      "map",
      "sources",
      "domains",
      "retrieve",
      "doctor",
      "source",
      "extract",
      "endpoints",
      "brand",
      "diff",
      "screenshot",
      "jobs-status",
      "jobs-clear",
      "jobs-list",
      "jobs-recover",
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
    expect(screen.getByText("POST /v1/sources")).toBeInTheDocument();
    expect(screen.getByText("Parameters")).toBeInTheDocument();
  });

  it("falls back to markdown text for historical help entries without payloads", () => {
    render(
      <OperationResultView
        payload={null}
        subcommand="help"
        fallbackText="# Scrape URL\n\nRoute: `POST /v1/sources`"
      />,
    );
    expect(screen.getByText(/# Scrape URL/)).toBeInTheDocument();
  });

  it("renders malformed help payloads as a visible error when no fallback text exists", () => {
    render(
      <OperationResultView
        payload={{ target: { title: "bad" } }}
        subcommand="help"
        fallbackText=""
      />,
    );
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

    expect(sanitizeReaderMarkdown(markdown)).toBe(
      ["- Real item", "```rust", "let ok = true;", "```"].join("\n"),
    );
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

  it("renders screenshot artifact handles without relying on absolute server paths", async () => {
    render(
      <OperationResultView subcommand="screenshot" payload={screenshotWithArtifactHandle()} />,
    );

    const img = await screen.findByRole("img", { name: /captured screenshot/i });
    expect(img).toHaveAttribute("src", "blob:test-shot");
    expect(mockLoadArtifactObjectUrl).toHaveBeenCalledWith("art_screenshot_123");
    expect(
      screen.queryByText("/home/axon/.axon/output/screenshots/example.png"),
    ).not.toBeInTheDocument();
    expect(screen.getByText("art_screenshot_123")).toBeInTheDocument();
  });

  it("shows a compact artifact preview failure state", async () => {
    mockLoadArtifactObjectUrl.mockRejectedValueOnce(new Error("artifact fetch failed with 401"));
    render(
      <OperationResultView subcommand="screenshot" payload={screenshotWithArtifactHandle()} />,
    );

    expect(await screen.findByText(/preview unavailable/i)).toBeInTheDocument();
  });

  it("revokes the object URL and shows an error when the preview image fails to decode", async () => {
    // Use a raw root (not RTL render) so a manually dispatched `error` event
    // reliably reaches React's onError handler under React 19.
    const host = document.createElement("div");
    document.body.appendChild(host);
    const root = createRoot(host);
    try {
      await act(async () => {
        root.render(
          <OperationResultView subcommand="screenshot" payload={screenshotWithArtifactHandle()} />,
        );
      });
      // Flush the mocked loadArtifactObjectUrl promise so the <img> renders.
      await act(async () => {});

      const img = host.querySelector("img");
      expect(img?.getAttribute("src")).toBe("blob:test-shot");

      await act(async () => {
        img?.dispatchEvent(new Event("error"));
      });

      expect(URL.revokeObjectURL).toHaveBeenCalledWith("blob:test-shot");
      expect(host.querySelector("img")).toBeNull();
      expect(host.textContent).toContain("Preview unavailable: image decode failed");
    } finally {
      await act(async () => {
        root.unmount();
      });
      host.remove();
    }
  });

  it("revokes stale artifact object URLs that resolve after unmount", async () => {
    let resolvePreview: (value: string) => void = () => undefined;
    mockLoadArtifactObjectUrl.mockReturnValueOnce(
      new Promise<string>((resolve) => {
        resolvePreview = resolve;
      }),
    );
    const { unmount } = render(
      <OperationResultView subcommand="screenshot" payload={screenshotWithArtifactHandle()} />,
    );

    unmount();
    await act(async () => {
      resolvePreview("blob:late-shot");
    });

    expect(URL.revokeObjectURL).toHaveBeenCalledWith("blob:late-shot");
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
      "source-site",
      "source",
      "extract",
      "endpoints",
      "brand",
      "diff",
      "screenshot",
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
        payload={{
          collection: "axon",
          results: [{ title: "hit one", url: "https://example.com/a", score: 0.91, rank: 1 }],
        }}
      />,
    );
    expect(screen.getByText("hit one")).toBeInTheDocument();
    expect(screen.getByText("0.910")).toBeInTheDocument();
  });

  it("renders web search results with queued source jobs", () => {
    render(
      <OperationResultView
        subcommand="search"
        payload={{
          results: [
            { title: "Result A", url: "https://example.com/a", snippet: "snippet", rank: 1 },
          ],
          source_jobs: [{ job_id: "job-1", status: "queued", url: "https://example.com/a" }],
        }}
      />,
    );
    expect(screen.getByText("Result A")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Queued source jobs" })).toBeInTheDocument();
    expect(document.querySelector(".operation-status-dot")).toBeInTheDocument();
    expect(document.querySelector(".operation-dot")).not.toBeInTheDocument();
  });

  it("renders discovered URLs for map", () => {
    render(
      <OperationResultView
        subcommand="map"
        payload={{ urls: ["https://example.com/x"], count: 1 }}
      />,
    );
    expect(screen.getByText("https://example.com/x")).toBeInTheDocument();
  });

  it("renders retrieved document content", () => {
    render(
      <OperationResultView subcommand="retrieve" payload={{ content: "Stored chunk body" }} />,
    );
    expect(screen.getByText("Stored chunk body")).toBeInTheDocument();
  });

  it("renders suggested URLs", () => {
    render(
      <OperationResultView
        subcommand="suggest"
        payload={{
          suggestions: [{ title: "Docs", url: "https://example.com/docs", reason: "Relevant" }],
        }}
      />,
    );
    expect(screen.getByText("Suggested URLs")).toBeInTheDocument();
    expect(screen.getByText("Docs")).toBeInTheDocument();
  });

  it("renders source and domain lists", () => {
    render(
      <OperationResultView
        subcommand="sources"
        payload={{ urls: ["https://example.com/source"] }}
      />,
    );
    expect(screen.getByText("Indexed sources")).toBeInTheDocument();
    expect(screen.getByText("https://example.com/source")).toBeInTheDocument();

    render(
      <OperationResultView
        subcommand="domains"
        payload={{ domains: [{ domain: "example.com", count: 2 }] }}
      />,
    );
    expect(screen.getByText("Indexed domains")).toBeInTheDocument();
    expect(screen.getAllByText("example.com").length).toBeGreaterThan(0);
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
    render(
      <OperationResultView
        subcommand="source-site"
        payload={{
          execution_mode: "async",
          result: { job_id: "abc123def456ghi", status: "queued" },
        }}
      />,
    );
    expect(screen.getByText("Source job queued")).toBeInTheDocument();
    // shortId truncates ids over 12 chars (canonical, with the ellipsis char).
    expect(screen.getByText("abc123def456…")).toBeInTheDocument();
  });

  it("renders job-start heroes for the remaining async families", () => {
    for (const subcommand of ["source", "extract"]) {
      render(
        <OperationResultView
          subcommand={subcommand}
          payload={{
            execution_mode: "async",
            result: { job_id: `${subcommand}-job-123`, status: "queued" },
          }}
        />,
      );
    }
    expect(screen.getAllByText("Status endpoint")).toHaveLength(2);
    expect(screen.getAllByText(/job queued/i)).toHaveLength(2);
  });

  it("renders a job-lifecycle list view", () => {
    render(
      <OperationResultView
        subcommand="jobs-list"
        payload={{ jobs: [{ job_id: "j1", status: "running", url: "https://example.com" }] }}
      />,
    );
    expect(screen.getByRole("heading", { name: "Jobs" })).toBeInTheDocument();
  });

  it("renders endpoint, brand, and diff detail views", () => {
    render(
      <OperationResultView
        subcommand="endpoints"
        payload={{ total: 1, endpoints: ["https://example.com/api"] }}
      />,
    );
    expect(screen.getByText("Endpoint discovery")).toBeInTheDocument();
    expect(screen.getAllByText("https://example.com/api").length).toBeGreaterThan(0);

    render(
      <OperationResultView
        subcommand="brand"
        payload={{
          name: "Aurora",
          colors: [{ hex: "#29b6f6", usage: "primary" }],
          fonts: ["Manrope"],
        }}
      />,
    );
    expect(screen.getByText("Aurora")).toBeInTheDocument();
    expect(screen.getByText("Manrope")).toBeInTheDocument();

    render(
      <OperationResultView
        subcommand="diff"
        payload={{
          status: "changed",
          url_a: "https://example.com/a",
          url_b: "https://example.com/b",
          metadata_changes: [{ field: "title", old: "A", new: "B" }],
        }}
      />,
    );
    expect(screen.getByText("Diff changed")).toBeInTheDocument();
    expect(screen.getByText("https://example.com/a")).toBeInTheDocument();
  });

  it("renders watch list and detail views", () => {
    render(
      <OperationResultView
        subcommand="watch-list"
        payload={{ watches: [{ name: "Docs watch", id: "watch-1" }] }}
      />,
    );
    expect(screen.getByText("Watch schedules")).toBeInTheDocument();
    expect(screen.getByText("Docs watch")).toBeInTheDocument();

    render(
      <OperationResultView
        subcommand="watch-run"
        payload={{ name: "Docs watch", watch_id: "watch-1", artifacts: [{ id: "artifact-1" }] }}
      />,
    );
    expect(screen.getByText("Watch ID")).toBeInTheDocument();
    expect(screen.getByText("watch-1")).toBeInTheDocument();
  });

  it("falls back to the generic view for unknown subcommands", () => {
    render(<OperationResultView subcommand="totally-unknown" payload={{ field_one: "value" }} />);
    expect(screen.getByRole("alert")).toHaveTextContent("Unknown palette action");
    expect(screen.getByText("totally-unknown")).toBeInTheDocument();
  });
});
