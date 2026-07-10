// @vitest-environment jsdom

import { act, renderHook } from "@testing-library/react";
import { useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { pendingJobSnapshot } from "@/lib/jobProgress";
import type { RunState } from "@/lib/runState";
import { useJobPoll } from "@/lib/useJobPoll";

function runningAsyncJob(jobId: string): RunState {
  return {
    kind: "asyncJob",
    family: "ingest",
    title: "Ingesting owner/repo",
    subtitle: `job ${jobId}`,
    jobId,
    statusUrl: `/v1/ingest/${jobId}`,
    target: "owner/repo",
    startedAtMs: Date.now(),
    snapshot: { ...pendingJobSnapshot("ingest", "owner/repo"), jobId, phase: "running", status: "running" },
    minimized: false,
  };
}

// Unified `GET /v1/jobs/{id}` returns a flat `JobSummary`, not the legacy
// `{ job, progress }` envelope — see `summarizeUnifiedJob` (bead
// axon_rust-ruzox.9) in `jobProgress.ts`.
function okResponse(status: string) {
  return new Response(JSON.stringify({ status }), {
    status: 200,
    headers: { "content-type": "application/json" },
  });
}

function setup(initial: RunState) {
  return renderHook(() => {
    const [run, setRun] = useState<RunState>(initial);
    const noop = () => undefined;
    const job = useJobPoll({ run, setRun, onMinimizeJob: noop, onExpandJob: noop, onCloseJob: noop });
    return { ...job, run };
  });
}

describe("useJobPoll stall detection (C2)", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.runOnlyPendingTimers();
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it("surfaces a visible failed state with errorText after 10 consecutive poll failures", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockRejectedValue(new Error("network down"));
    const view = setup(runningAsyncJob("job-stall"));

    // Mount tick + 10 interval ticks = 11 failures; the 10th trips the stall.
    await act(async () => {
      await Promise.resolve();
      await vi.advanceTimersByTimeAsync(11_000);
    });

    expect(fetchSpy.mock.calls.length).toBeGreaterThanOrEqual(10);
    const run = view.result.current.run;
    expect(run.kind).toBe("asyncJob");
    if (run.kind !== "asyncJob") throw new Error("expected asyncJob");
    expect(run.snapshot.phase).toBe("failed");
    expect(run.snapshot.errorText).toContain("Lost contact with the server");
    expect(run.subtitle).toBe("lost contact with server");
  });

  it("counts a non-ok HTTP status toward the stall, not just thrown errors", async () => {
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValue(new Response("boom", { status: 503 }));
    const view = setup(runningAsyncJob("job-503"));

    await act(async () => {
      await Promise.resolve();
      await vi.advanceTimersByTimeAsync(11_000);
    });

    expect(fetchSpy.mock.calls.length).toBeGreaterThanOrEqual(10);
    const run = view.result.current.run;
    if (run.kind !== "asyncJob") throw new Error("expected asyncJob");
    expect(run.snapshot.phase).toBe("failed");
    expect(run.snapshot.errorText).toContain("Lost contact with the server");
  });

  it("recovers (no stall) when a transient failure is followed by a success", async () => {
    let calls = 0;
    vi.spyOn(globalThis, "fetch").mockImplementation(() => {
      calls += 1;
      // Fail the first 3 ticks, then succeed forever.
      return calls <= 3 ? Promise.reject(new Error("blip")) : Promise.resolve(okResponse("running"));
    });
    const view = setup(runningAsyncJob("job-recover"));

    await act(async () => {
      await Promise.resolve();
      await vi.advanceTimersByTimeAsync(11_000);
    });

    const run = view.result.current.run;
    if (run.kind !== "asyncJob") throw new Error("expected asyncJob");
    // 3 failures < threshold and the counter reset on success → still running.
    expect(run.snapshot.phase).toBe("running");
    expect(run.snapshot.errorText).toBeNull();
  });
});
