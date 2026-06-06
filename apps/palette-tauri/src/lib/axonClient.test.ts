import { beforeEach, describe, expect, it, vi } from "vitest";

import { ACTIONS, type PaletteAction } from "./actions";
import { createAxonClient, executeAction, type PaletteConfig } from "./axonClient";
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

function lastRequestBody(): unknown {
  const invokeMock = vi.mocked(invoke);
  const call = invokeMock.mock.calls.at(-1);
  if (!call) throw new Error("invoke was not called");
  const args = call[1] as { request: { body: unknown } };
  return args.request.body;
}

function executeTestAction(subcommand: string, arg: string) {
  return executeAction(createAxonClient(config), action(subcommand), arg, config);
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
});
