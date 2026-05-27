import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { ACTIONS, type PaletteAction } from "./actions";
import { executeAction, type PaletteConfig } from "./axonClient";

vi.mock("@tauri-apps/api/core", () => ({
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

  it("posts GitHub ingest targets using the REST repo field", async () => {
    await executeAction(action("ingest"), "owner/repo", config);

    expect(lastRequestBody()).toEqual({
      source_type: "github",
      repo: "owner/repo",
      include_source: true,
    });
  });

  it("does not attach collection to summarize requests", async () => {
    await executeAction(action("summarize"), "https://example.com/doc", config);

    expect(lastRequestBody()).toEqual({
      urls: ["https://example.com/doc"],
    });
  });

  it("attaches the configured collection to ask requests", async () => {
    await executeAction(action("ask"), "what changed?", config);

    expect(lastRequestBody()).toEqual({
      query: "what changed?",
      explain: false,
      diagnostics: false,
      collection: "docs",
    });
  });
});
