// @vitest-environment jsdom
import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));
vi.mock("./invoke", () => ({ invoke: invokeMock }));

import type { RunState } from "./runState";
import { isLiveRefreshablePath, useLiveRefresh } from "./useLiveRefresh";

function successfulRun(path: string): RunState {
  return {
    kind: "success",
    result: { ok: true, status: 200, payload: { version: "initial" }, path },
  } as RunState;
}

describe("isLiveRefreshablePath", () => {
  it("treats stats and status as live-refreshable", () => {
    expect(isLiveRefreshablePath("/v1/stats")).toBe(true);
    expect(isLiveRefreshablePath("/v1/status")).toBe(true);
  });

  it("does NOT auto-refresh slow-growing or one-shot endpoints", () => {
    expect(isLiveRefreshablePath("/v1/sources")).toBe(false);
    expect(isLiveRefreshablePath("/v1/domains")).toBe(false);
    expect(isLiveRefreshablePath("/v1/doctor")).toBe(false);
    expect(isLiveRefreshablePath("/v1/ask")).toBe(false);
  });

  it("is safe on undefined/empty", () => {
    expect(isLiveRefreshablePath(undefined)).toBe(false);
    expect(isLiveRefreshablePath("")).toBe(false);
  });
});

describe("useLiveRefresh request ownership", () => {
  it("does not start a parallel refresh while the prior request is pending", async () => {
    let resolve!: (value: unknown) => void;
    invokeMock.mockReturnValueOnce(
      new Promise((done) => {
        resolve = done;
      }),
    );
    const setRun = vi.fn();
    const { result } = renderHook(() =>
      useLiveRefresh({ run: successfulRun("/v1/stats"), setRun, paused: false }),
    );
    act(() => {
      result.current.refreshNow();
      result.current.refreshNow();
    });
    expect(invokeMock).toHaveBeenCalledTimes(1);
    await act(async () => resolve({ ok: true, status: 200, payload: { version: "one" } }));
  });

  it("invalidates a pending completion when the view changes", async () => {
    let resolve!: (value: unknown) => void;
    invokeMock.mockReturnValueOnce(
      new Promise((done) => {
        resolve = done;
      }),
    );
    const setRun = vi.fn();
    const { result, rerender } = renderHook(
      ({ run }) => useLiveRefresh({ run, setRun, paused: false }),
      { initialProps: { run: successfulRun("/v1/stats") } },
    );
    act(() => result.current.refreshNow());
    rerender({ run: successfulRun("/v1/sources") });
    await act(async () => resolve({ ok: true, status: 200, payload: { version: "stale" } }));
    expect(setRun).not.toHaveBeenCalled();
  });
});
