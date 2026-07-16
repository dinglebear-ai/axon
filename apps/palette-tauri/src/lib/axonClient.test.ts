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

function requestFor(subcommand: string, arg: string) {
  return buildActionRequest(createAxonClient(config), remoteAction(subcommand), arg, config);
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

  it("sends source targets raw with no source_type", async () => {
    // The palette must NOT classify client-side — that classifier drifted from the
    // canonical backend `classify_target`. It ships the bare target; the server
    // routes github/gitlab/gitea/git/reddit/youtube/rss from it.
    await executeTestAction("source", "owner/repo");
    expect(lastRequestBody()).toEqual({ source: "owner/repo", collection: "docs" });

    await executeTestAction("source", "https://www.youtube.com/watch?v=abc123");
    expect(lastRequestBody()).toEqual({ source: "https://www.youtube.com/watch?v=abc123", collection: "docs" });

    await executeTestAction("source", "r/rust");
    expect(lastRequestBody()).toEqual({ source: "r/rust", collection: "docs" });
  });

  it("sends GitLab/Gitea targets raw (previously misclassified as github by the client)", async () => {
    await executeTestAction("source", "https://gitlab.com/group/project");
    expect(lastRequestBody()).toEqual({ source: "https://gitlab.com/group/project", collection: "docs" });
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
    await executeTestAction("jobs-status", "00000000-0000-4000-8000-000000000000");

    const invokeMock = vi.mocked(invoke);
    const call = invokeMock.mock.calls.at(-1);
    const args = call?.[1] as { request: { method: string; path: string; body: unknown } };
    expect(args.request).toMatchObject({
      method: "GET",
      path: "/v1/jobs/00000000-0000-4000-8000-000000000000",
      body: null,
    });
  });

  it("builds watch create requests from URL and interval", async () => {
    await executeTestAction("watch-create", "https://example.com/docs 120");

    expect(lastRequestBody()).toEqual({
      source: "https://example.com/docs",
      schedule: { every_seconds: 120 },
      embed: true,
      options: { values: {} },
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

  it("pins exact route and body contracts for static actions", () => {
    const cases: Array<[string, string, string, string, unknown]> = [
      ["doctor", "", "GET", "/v1/doctor", null],
      ["status", "", "GET", "/v1/status", null],
      ["sources", "", "GET", "/v1/sources", null],
      ["domains", "", "GET", "/v1/domains", null],
      ["stats", "", "GET", "/v1/stats", null],
      ["watch-list", "", "GET", "/v1/watches", null],
      ["scrape", "https://example.com/doc", "POST", "/v1/sources", { source: "https://example.com/doc", scope: "page", collection: "docs" }],
      ["source-site", "https://example.com/docs", "POST", "/v1/sources", { source: "https://example.com/docs", scope: "site", collection: "docs" }],
      ["map", "https://example.com", "POST", "/v1/map", { url: "https://example.com" }],
      ["summarize", "https://example.com/doc", "POST", "/v1/summarize", { urls: ["https://example.com/doc"] }],
      ["ask", "what changed?", "POST", "/v1/ask", { query: "what changed?", explain: false, diagnostics: false, collection: "docs" }],
      ["chat", "plain llm chat", "POST", "/v1/chat", { message: "plain llm chat" }],
      ["query", "palette routes", "POST", "/v1/query", { query: "palette routes", limit: 7, collection: "docs" }],
      ["retrieve", "https://example.com/doc", "POST", "/v1/retrieve", { url: "https://example.com/doc", collection: "docs" }],
      ["suggest", "tauri", "POST", "/v1/suggest", { focus: "tauri" }],
      ["evaluate", "is RAG better?", "POST", "/v1/evaluate", { question: "is RAG better?" }],
      ["search", "tauri v2", "POST", "/v1/search", { query: "tauri v2", limit: 7 }],
      ["research", "qdrant hybrid", "POST", "/v1/research", { query: "qdrant hybrid", limit: 7 }],
      ["source", "owner/repo", "POST", "/v1/sources", { source: "owner/repo", collection: "docs" }],
      ["extract", "https://example.com/pricing", "POST", "/v1/extract", { urls: ["https://example.com/pricing"], collection: "docs" }],
      ["endpoints", "https://example.com", "POST", "/v1/endpoints", { url: "https://example.com" }],
      ["brand", "https://example.com", "POST", "/v1/brand", { url: "https://example.com" }],
      ["diff", "https://example.com/a https://example.com/b", "POST", "/v1/diff", { url_a: "https://example.com/a", url_b: "https://example.com/b" }],
      ["screenshot", "https://example.com", "POST", "/v1/screenshot", { url: "https://example.com", full_page: true }],
      [
        "watch-create",
        "https://example.com/docs 120",
        "POST",
        "/v1/watches",
        { source: "https://example.com/docs", schedule: { every_seconds: 120 }, embed: true, options: { values: {} }, enabled: true },
      ],
      ["watch-run", "00000000-0000-4000-8000-000000000000", "POST", "/v1/watches/00000000-0000-4000-8000-000000000000/exec", null],
    ];

    for (const [subcommand, arg, method, path, body] of cases) {
      expect(requestFor(subcommand, arg), subcommand).toMatchObject({ method, path, body });
    }
  });

  it("pins exact route and body contracts for every job lifecycle operation", () => {
    const id = "00000000-0000-4000-8000-000000000000";
    expect(requestFor("jobs-list", "")).toMatchObject({ method: "GET", path: "/v1/jobs", body: null });
    expect(requestFor("jobs-status", id)).toMatchObject({ method: "GET", path: `/v1/jobs/${id}`, body: null });
    expect(requestFor("jobs-cancel", id)).toMatchObject({ method: "POST", path: `/v1/jobs/${id}/cancel`, body: {} });
    expect(requestFor("jobs-cleanup", "")).toMatchObject({
      method: "POST",
      path: "/v1/jobs/cleanup",
      body: { dry_run: false },
    });
    expect(requestFor("jobs-clear", "")).toMatchObject({
      method: "DELETE",
      path: "/v1/jobs",
      body: { confirm: true },
    });
    expect(requestFor("jobs-recover", "")).toMatchObject({
      method: "POST",
      path: "/v1/jobs/recover",
      body: {},
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
      "source-site",
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
      "source",
      "extract",
      "endpoints",
      "brand",
      "diff",
      "screenshot",
      "watch-create",
      // Unified job lifecycle
      "jobs-list",
      "jobs-status",
      "jobs-cancel",
      "jobs-cleanup",
      "jobs-clear",
      "jobs-recover",
      // Dynamic watch routes
      "watch-run",
      // NOT an Axon REST route: `github` is special-cased in `executeAction`
      // (axonClient.ts) to call the dedicated `github_browse` Tauri command
      // (src-tauri/src/github_bridge.rs) instead of `axon_http_request`, so it
      // has no entry in axon_bridge.rs's `is_allowed_route`. Listed here so
      // this cross-layer contract test doesn't flag it as unwired.
      "github",
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
