// @vitest-environment jsdom

import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Client } from "./axonClient";
import { useCodexControl } from "./useCodexControl";

const readCodexSnapshot = vi.fn();
const readCodexEvents = vi.fn();
const readCodexOperations = vi.fn();
vi.mock("./codexControl", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./codexControl")>()),
  readCodexSnapshot: (...args: unknown[]) => readCodexSnapshot(...args),
  readCodexEvents: (...args: unknown[]) => readCodexEvents(...args),
  readCodexOperations: (...args: unknown[]) => readCodexOperations(...args),
}));

const client: Client = { baseUrl: "https://axon.example", headers: {} };
const event = {
  cursor: { boot_id: 1, sequence: 4 },
  event: { kind: "server_request", request_id: 9 },
};
const snapshot = { status: { state: "ready" }, pending_server_requests: [event] };

beforeEach(() => {
  vi.clearAllMocks();
  readCodexSnapshot.mockResolvedValue(snapshot);
  readCodexEvents.mockResolvedValue([event]);
  readCodexOperations.mockResolvedValue([]);
});

describe("useCodexControl", () => {
  it("keeps a successful snapshot when event polling fails", async () => {
    readCodexEvents.mockRejectedValueOnce(new Error("event service unavailable"));
    const { result } = renderHook(() => useCodexControl(client, true));

    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.snapshot).toEqual(snapshot);
    expect(result.current.events).toEqual([event]);
    expect(result.current.error).toContain("event service unavailable");
  });

  it("resets boot-scoped events and retries without a cursor after a cursor error", async () => {
    const { result } = renderHook(() => useCodexControl(client, true));
    await waitFor(() => expect(result.current.events).toEqual([event]));
    readCodexEvents
      .mockRejectedValueOnce(new Error("cursor boot id no longer matches"))
      .mockResolvedValueOnce([]);

    await act(async () => result.current.refresh());

    expect(readCodexEvents).toHaveBeenLastCalledWith(client, undefined);
    expect(result.current.events).toEqual([event]);
    expect(result.current.error).toBeNull();
  });

  it("does not recreate answered requests from replay history after remount", async () => {
    readCodexSnapshot.mockResolvedValue({
      status: { state: "ready" },
      pending_server_requests: [],
    });
    const first = renderHook(() => useCodexControl(client, true));
    await waitFor(() => expect(first.result.current.loading).toBe(false));
    expect(first.result.current.events).toEqual([]);
    first.unmount();

    const second = renderHook(() => useCodexControl(client, true));
    await waitFor(() => expect(second.result.current.loading).toBe(false));
    expect(second.result.current.events).toEqual([]);
  });
});
