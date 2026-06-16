// @vitest-environment jsdom

import { act, renderHook } from "@testing-library/react";
import { useState, type Dispatch, type SetStateAction } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { PaletteAction } from "@/lib/actions";
import { summarizeCrawl } from "@/lib/crawlJob";
import type { RunState } from "@/lib/runState";
import { useCrawlJob } from "@/lib/useCrawlJob";

function runningJob(jobId: string): RunState {
  return {
    kind: "job",
    family: "crawl",
    title: "Crawling example.com",
    subtitle: `job ${jobId}`,
    jobId,
    statusUrl: `/v1/crawl/${jobId}`,
    url: "https://example.com",
    startedAtMs: Date.now(),
    maxPages: 0,
    maxDepth: 0,
    snapshot: summarizeCrawl({ job: { status: "running" } }, { jobId, url: "https://example.com" }),
    minimized: false,
  };
}

function crawlResponse(status: string, pages: number) {
  return new Response(JSON.stringify({ job: { status, pages_crawled: pages } }), {
    status: 200,
    headers: { "content-type": "application/json" },
  });
}

function setup(initial: RunState) {
  return renderHook(() => {
    const [run, setRun] = useState<RunState>(initial);
    // Only `setRun` matters for the poll; the rest are inert in these tests.
    const noop = (_value?: unknown) => undefined;
    const job = useCrawlJob({
      run,
      setRun,
      setSettingsOpen: noop as Dispatch<SetStateAction<boolean>>,
      setHistoryOpen: noop as Dispatch<SetStateAction<boolean>>,
      setBrowseOpen: noop as Dispatch<SetStateAction<boolean>>,
      setQuery: noop as Dispatch<SetStateAction<string>>,
      setModeAction: noop as Dispatch<SetStateAction<PaletteAction | null>>,
    });
    return { ...job, run };
  });
}

describe("useCrawlJob 1Hz poll (T-L1)", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.runOnlyPendingTimers();
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it("polls the crawl status roughly once per second while non-terminal", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockResolvedValue(crawlResponse("running", 3));
    setup(runningJob("job-1"));

    // The effect fires an immediate `tick()` on mount.
    await act(async () => {
      await Promise.resolve();
    });
    const afterMount = fetchSpy.mock.calls.length;
    expect(afterMount).toBeGreaterThanOrEqual(1);

    // Each 1000ms interval should issue one more poll.
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1000);
    });
    expect(fetchSpy.mock.calls.length).toBe(afterMount + 1);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(2000);
    });
    expect(fetchSpy.mock.calls.length).toBe(afterMount + 3);

    // Every poll targets the crawl status route.
    expect(fetchSpy).toHaveBeenCalledWith("/v1/crawl/job-1", expect.objectContaining({ method: "GET" }));
  });

  it("stops polling once the job reaches a terminal phase", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockResolvedValue(crawlResponse("completed", 5));
    setup(runningJob("job-2"));

    await act(async () => {
      await Promise.resolve();
      await vi.advanceTimersByTimeAsync(1500);
    });
    const callsAfterTerminal = fetchSpy.mock.calls.length;

    // The snapshot is now terminal (done); advancing further must not poll again.
    await act(async () => {
      await vi.advanceTimersByTimeAsync(3000);
    });
    expect(fetchSpy.mock.calls.length).toBe(callsAfterTerminal);
  });
});
