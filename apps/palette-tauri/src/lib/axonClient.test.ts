import { beforeEach, describe, expect, it, vi } from "vitest";

import { ACTIONS, type PaletteAction, type RemotePaletteAction } from "./actions";
import { buildActionRequest, createAxonClient, executeAction, type PaletteConfig } from "./axonClient";
import { invoke } from "./invoke";

// executeAction routes through the shared `./invoke` wrapper (not the raw
// @tauri-apps/api/core invoke), so the assertions mock that wrapper directly.
vi.mock("./invoke", () => ({
  invoke: vi.fn(),
}));

const config: PaletteConfig = {
  serverUrl: "127.0.0.1:8001/",
  token: "secret",
  shortcut: "Ctrl+Shift+Space",
  collection: "docs",
  resultLimit: 7,
  theme: "system",
  hideOnBlur: true,
};

function action(subcommand: string): PaletteAction {
  const found = ACTIONS.find((candidate) => candidate.subcommand === subcommand);
  if (!found) throw new Error(`missing action ${subcommand}`);
  return found;
}

function remoteAction(subcommand: string): RemotePaletteAction {
  const found = action(subcommand);
  if (found.kind === "local") throw new Error(`${subcommand} is local`);
  return found;
}

function lastRequestBody(): unknown {
  const invokeMock = vi.mocked(invoke);
  const call = invokeMock.mock.calls.at(-1);
  if (!call) throw new Error("invoke was not called");
  const args = call[1] as { request: { body: unknown } };
  return args.request.body;
}

function executeTestAction(subcommand: string, arg: string) {
  return executeAction(createAxonClient(config), remoteAction(subcommand), arg, config);
}

describe("executeAction", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockResolvedValue({
      ok: true,
      status: 200,
      method: "POST",
      path: "/v1/test",
      payload: {},
    });
  });

  it("posts GitHub ingest targets using the REST target field", async () => {
    await executeTestAction("ingest", "owner/repo");

    expect(lastRequestBody()).toEqual({
      source_type: "github",
      target: "owner/repo",
      include_source: true,
    });
  });

  it("does not attach collection to summarize requests", async () => {
    await executeTestAction("summarize", "https://example.com/doc");

    expect(lastRequestBody()).toEqual({
      urls: ["https://example.com/doc"],
    });
  });

  it("attaches the configured collection to ask requests", async () => {
    await executeTestAction("ask", "what changed?");

    expect(lastRequestBody()).toEqual({
      query: "what changed?",
      explain: false,
      diagnostics: false,
      collection: "docs",
    });
  });

  it("builds chat requests without RAG collection", async () => {
    await executeTestAction("chat", "plain llm chat");

    expect(lastRequestBody()).toEqual({
      message: "plain llm chat",
    });
  });

  it("builds lifecycle routes for async job operations", async () => {
    await executeTestAction("crawl-status", "00000000-0000-4000-8000-000000000000");

    const invokeMock = vi.mocked(invoke);
    const call = invokeMock.mock.calls.at(-1);
    const args = call?.[1] as { request: { method: string; path: string; body: unknown } };
    expect(args.request).toMatchObject({
      method: "GET",
      path: "/v1/crawl/00000000-0000-4000-8000-000000000000",
      body: null,
    });
  });

  it("builds watch create requests from URL and interval", async () => {
    await executeTestAction("watch-create", "https://example.com/docs 120");

    expect(lastRequestBody()).toEqual({
      name: "example.com",
      task_type: "watch",
      task_payload: { urls: ["https://example.com/docs"], ignore_patterns: [] },
      every_seconds: 120,
      enabled: true,
    });
  });

  it("builds brand extraction requests", async () => {
    await executeTestAction("brand", "https://aurora.tootie.tv");

    expect(lastRequestBody()).toEqual({
      url: "https://aurora.tootie.tv",
    });
  });

  it("builds diff requests with two URLs", async () => {
    await executeTestAction("diff", "https://example.com/a https://example.com/b");

    expect(lastRequestBody()).toEqual({
      url_a: "https://example.com/a",
      url_b: "https://example.com/b",
    });
  });

  it("builds full-page screenshot requests", async () => {
    await executeTestAction("screenshot", "https://example.com");

    expect(lastRequestBody()).toEqual({
      url: "https://example.com",
      full_page: true,
    });
  });

  it("rejects local actions before request construction", () => {
    const client = createAxonClient(config);
    expect(() => buildActionRequest(client, action("help") as unknown as RemotePaletteAction, "scrape", config)).toThrow(
      "Local action help cannot be sent to Axon REST",
    );
  });

  it("has REST request mappings for every palette action example", () => {
    const client = createAxonClient(config);

    for (const candidate of ACTIONS) {
      if (candidate.kind === "local") continue;
      const arg = candidate.example.startsWith(candidate.subcommand)
        ? candidate.example.slice(candidate.subcommand.length).trim()
        : "";
      expect(() => buildActionRequest(client, candidate, arg, config), candidate.subcommand).not.toThrow();
    }
  });

  it("uses valid UUID examples for id-based actions", () => {
    const uuidPattern = /[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}/i;

    for (const candidate of ACTIONS) {
      if (!candidate.subcommand.endsWith("-status") && !candidate.subcommand.endsWith("-cancel") && candidate.subcommand !== "watch-run") {
        continue;
      }
      expect(candidate.example, candidate.subcommand).toMatch(uuidPattern);
    }
  });

  // M8: Cross-layer route contract test.
  //
  // Every action subcommand used in buildActionRequest must have a
  // corresponding entry in the Rust ALLOWED_ROUTES set (axon_bridge.rs
  // `is_allowed_route`).  This test encodes the known-good list of
  // subcommands so that adding a new action on the TS side without wiring
  // it in Rust is caught at test time.
  it("all action subcommands are in the Rust ALLOWED_ROUTES allowlist", () => {
    // This list must match the routes accepted by `is_allowed_route` in
    // apps/palette-tauri/src-tauri/src/axon_bridge.rs.
    const RUST_ALLOWED_SUBCOMMANDS = new Set([
      // Static GET routes
      "doctor",
      "status",
      "sources",
      "domains",
      "stats",
      "watch-list",
      // Static POST routes
      "scrape",
      "crawl",
      "map",
      "summarize",
      "ask",
      "chat",
      "query",
      "retrieve",
      "suggest",
      "evaluate",
      "search",
      "research",
      "embed",
      "extract",
      "ingest",
      "endpoints",
      "brand",
      "diff",
      "screenshot",
      "dedupe",
      "watch-create",
      "ingest-sessions-prepared",
      // Job lifecycle (family-operation pairs)
      "crawl-list",
      "crawl-status",
      "crawl-cancel",
      "crawl-cleanup",
      "crawl-clear",
      "crawl-recover",
      "embed-list",
      "embed-status",
      "embed-cancel",
      "embed-cleanup",
      "embed-clear",
      "embed-recover",
      "extract-list",
      "extract-status",
      "extract-cancel",
      "extract-cleanup",
      "extract-clear",
      "extract-recover",
      "ingest-list",
      "ingest-status",
      "ingest-cancel",
      "ingest-cleanup",
      "ingest-clear",
      "ingest-recover",
      // Dynamic watch routes
      "watch-run",
    ]);

    const actionSubcommands = ACTIONS.filter((a) => a.kind !== "local").map((a) => a.subcommand);
    const unlisted = actionSubcommands.filter((s) => !RUST_ALLOWED_SUBCOMMANDS.has(s));

    expect(
      unlisted,
      `Action subcommands not in Rust ALLOWED_ROUTES: ${unlisted.join(", ")}\n` +
        `Add them to is_allowed_route() in axon_bridge.rs and to RUST_ALLOWED_SUBCOMMANDS above.`,
    ).toHaveLength(0);
  });
});
