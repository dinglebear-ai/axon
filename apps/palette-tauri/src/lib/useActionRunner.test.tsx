// @vitest-environment jsdom

import { act, renderHook } from "@testing-library/react";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { HistoryItem } from "@/components/palette/HistoryPanel";
import { ACTIONS, type PaletteAction } from "@/lib/actions";
import type { Client, PaletteConfig } from "@/lib/axonClient";
import { parseCommand } from "@/lib/paletteView";
import type { RunState } from "@/lib/runState";
import { reduceStreamEvent, useActionRunner } from "@/lib/useActionRunner";

function action(subcommand: string) {
  const found = ACTIONS.find((candidate) => candidate.subcommand === subcommand);
  if (!found) throw new Error(`missing action ${subcommand}`);
  return found;
}

const config: PaletteConfig = {
  serverUrl: "http://127.0.0.1:9999",
  token: null,
  shortcut: "Ctrl+Space",
  collection: "axon",
  resultLimit: 10,
  theme: "dark",
  hideOnBlur: false,
};

const client: Client = { baseUrl: "http://127.0.0.1:9999", headers: {} };

afterEach(() => vi.restoreAllMocks());

function setup(
  query: string,
  overrides: { client?: Client | null; config?: PaletteConfig | null; modeAction?: PaletteAction | null } = {},
) {
  const parsed = parseCommand(query);
  return renderHook(() => {
    const [run, setRun] = useState<RunState>({ kind: "idle" });
    const [history, setHistory] = useState<HistoryItem[]>([]);
    const [modeAction, setModeAction] = useState<PaletteAction | null>(overrides.modeAction ?? null);
    const [input, setQuery] = useState(query);
    const [, setBrowseOpen] = useState(false);
    const runner = useActionRunner({
      client: overrides.client === undefined ? client : overrides.client,
      config: overrides.config === undefined ? config : overrides.config,
      run,
      setRun,
      setHistory,
      setModeAction,
      setQuery,
      setBrowseOpen,
      modeAction,
      parsed,
      query: input,
    });
    return { ...runner, run, history, parsed };
  });
}

describe("useActionRunner local help", () => {
  it.each([
    ["help", "help", undefined],
    ["help scrape", "help", "scrape"],
    ["scrape help", "help", "scrape"],
    ["fetch help", "help", "scrape"],
    ["crawl --help", "help", "crawl"],
    ["?", "help", undefined],
  ])("handles %s without requiring a backend client", async (query, subcommand, target) => {
    const rendered = setup(query, { client: null, config: null });
    await act(async () => {
      await rendered.result.current.submit(action(subcommand));
    });
    expect(rendered.result.current.run.kind).toBe("success");
    expect("result" in rendered.result.current.run ? rendered.result.current.run.result.path : "").toBe("palette://help");
    const payload = "result" in rendered.result.current.run ? rendered.result.current.run.result.payload : null;
    if (target) expect(payload).toMatchObject({ target: { subcommand: target } });
    else expect(payload).toMatchObject({ catalog: expect.any(Array) });
  });

  it.each([
    ["help", "help"],
    ["help scrape", "help"],
    ["scrape help", "help"],
    ["crawl --help", "help"],
  ])("does not call REST for %s when a backend client exists", async (query, subcommand) => {
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockRejectedValue(new Error("REST should not be called"));
    const rendered = setup(query);
    await act(async () => {
      await rendered.result.current.submit(action(subcommand));
    });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it("keeps help-like text as backend input when already in action mode", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify({ query: "help" }), {
        status: 200,
        headers: { "content-type": "application/json" },
      }),
    );
    const rendered = setup("help", { modeAction: action("ask") });
    await act(async () => {
      await rendered.result.current.submit(action("ask"));
    });
    // The one-shot path now dispatches through useActionState (R-H1); flush the
    // transition so the terminal RunState has settled before asserting.
    await act(async () => {});

    expect(fetchSpy).toHaveBeenCalledWith(
      "/v1/ask",
      expect.objectContaining({
        body: JSON.stringify({ query: "help", explain: false, diagnostics: false, collection: "axon" }),
      }),
    );
    expect(rendered.result.current.history[0]?.action.subcommand).toBe("ask");
    expect(rendered.result.current.history[0]?.target).toBe("help");
    expect(rendered.result.current.run.kind).toBe("success");
    expect("result" in rendered.result.current.run ? rendered.result.current.run.result.path : "").toBe("/v1/ask");
    expect("result" in rendered.result.current.run ? rendered.result.current.run.result.payload : null).toMatchObject({
      query: "help",
    });
  });
});

// ── R-H1: one-shot (useActionState) request/response path ────────────────────
describe("useActionRunner one-shot useActionState path", () => {
  it("transitions running → success and records history on a 2xx response", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify({ collection: "axon", points: 42 }), {
        status: 200,
        headers: { "content-type": "application/json" },
      }),
    );
    const rendered = setup("stats");
    await act(async () => {
      await rendered.result.current.submit(action("stats"));
    });
    await act(async () => {});
    expect(rendered.result.current.run.kind).toBe("success");
    const run = rendered.result.current.run;
    expect("result" in run ? run.result.status : 0).toBe(200);
    expect(rendered.result.current.history[0]?.action.subcommand).toBe("stats");
  });

  it("transitions running → error on a non-2xx response", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify({ error: "boom" }), {
        status: 500,
        headers: { "content-type": "application/json" },
      }),
    );
    const rendered = setup("stats");
    await act(async () => {
      await rendered.result.current.submit(action("stats"));
    });
    await act(async () => {});
    expect(rendered.result.current.run.kind).toBe("error");
  });

  it("surfaces a fetch rejection as an error RunState with status 0", async () => {
    vi.spyOn(globalThis, "fetch").mockRejectedValue(new Error("network down"));
    const rendered = setup("stats");
    await act(async () => {
      await rendered.result.current.submit(action("stats"));
    });
    await act(async () => {});
    const run = rendered.result.current.run;
    expect(run.kind).toBe("error");
    expect("result" in run ? run.result.status : -1).toBe(0);
    expect("text" in run ? run.text : "").toContain("network down");
  });
});

// ── A-M5: pressing Enter is never a silent no-op ─────────────────────────────
describe("useActionRunner A-M5 transient errors", () => {
  it("surfaces an error RunState when no client/config is configured", async () => {
    const rendered = setup("stats", { client: null, config: null });
    await act(async () => {
      await rendered.result.current.submit(action("stats"));
    });
    const run = rendered.result.current.run;
    expect(run.kind).toBe("error");
    expect("title" in run ? run.title : "").toContain("unavailable");
    // No REST attempt is made when the client is missing.
  });

  it("surfaces a validation error instead of returning silently", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch");
    // `ask` requires an argument; submitting empty should surface a needs-input error.
    const rendered = setup("ask");
    await act(async () => {
      await rendered.result.current.submit(action("ask"), "");
    });
    const run = rendered.result.current.run;
    expect(run.kind).toBe("error");
    expect("text" in run ? run.text : "").toMatch(/required/i);
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});

// ── A-M4 / streaming reducer: no fabricated { status: 200 } ──────────────────
describe("reduceStreamEvent", () => {
  const streaming: RunState = {
    kind: "streaming",
    title: "Streaming Ask",
    subtitle: "POST /v1/ask/stream",
    text: "partial",
    outputKind: "markdown",
    requestId: "req-1",
    path: "/v1/ask/stream",
    actionLabel: "Ask",
    prompt: "why",
  };

  it("appends delta text only for the matching requestId", () => {
    const next = reduceStreamEvent(streaming, { type: "delta", requestId: "req-1", text: " more" });
    expect("text" in next ? next.text : "").toBe("partial more");
    const ignored = reduceStreamEvent(streaming, { type: "delta", requestId: "other", text: "x" });
    expect(ignored).toBe(streaming);
  });

  it("produces a success terminal state with status 0 (not a fabricated 200)", () => {
    const next = reduceStreamEvent(streaming, { type: "done", requestId: "req-1", answer: "final answer" });
    expect(next.kind).toBe("success");
    expect("result" in next ? next.result.status : -1).toBe(0);
    expect("result" in next ? next.result.payload : null).toMatchObject({ answer: "final answer" });
    expect("text" in next ? next.text : "").toBe("final answer");
  });

  it("produces an honest error terminal state on a stream error event", () => {
    const next = reduceStreamEvent(streaming, { type: "error", requestId: "req-1", message: "stream broke" });
    expect(next.kind).toBe("error");
    expect("result" in next ? next.result.status : -1).toBe(0);
    expect("text" in next ? next.text : "").toBe("stream broke");
  });

  it("leaves non-streaming states untouched", () => {
    const idle: RunState = { kind: "idle" };
    expect(reduceStreamEvent(idle, { type: "done", requestId: "req-1" })).toBe(idle);
  });
});
