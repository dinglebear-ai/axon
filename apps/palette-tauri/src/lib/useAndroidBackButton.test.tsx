// @vitest-environment jsdom

import { renderHook, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { useAndroidBackButton } from "./useAndroidBackButton";

afterEach(() => {
  delete window.__AXON_ANDROID_BACK__;
  vi.clearAllMocks();
});

describe("useAndroidBackButton", () => {
  it("exposes the Android bridge only while enabled and cleans it up", async () => {
    const onBack = vi.fn(() => true);
    const { unmount } = renderHook(() => useAndroidBackButton(true, onBack));

    await waitFor(() => expect(window.__AXON_ANDROID_BACK__).toBeTypeOf("function"));
    expect(window.__AXON_ANDROID_BACK__?.()).toBe(true);
    expect(onBack).toHaveBeenCalledTimes(1);

    unmount();
    expect(window.__AXON_ANDROID_BACK__).toBeUndefined();
  });

  it("returns false to tell Android to finish the root Activity", async () => {
    const onBack = vi.fn(() => false);
    renderHook(() => useAndroidBackButton(true, onBack));

    await waitFor(() => expect(window.__AXON_ANDROID_BACK__).toBeTypeOf("function"));
    expect(window.__AXON_ANDROID_BACK__?.()).toBe(false);
  });

  it("uses the latest callback without replacing the native bridge function", async () => {
    const first = vi.fn(() => true);
    const second = vi.fn(() => false);
    const { rerender } = renderHook(({ callback }) => useAndroidBackButton(true, callback), {
      initialProps: { callback: first },
    });

    await waitFor(() => expect(window.__AXON_ANDROID_BACK__).toBeTypeOf("function"));
    const bridge = window.__AXON_ANDROID_BACK__;

    rerender({ callback: second });
    expect(window.__AXON_ANDROID_BACK__).toBe(bridge);
    expect(window.__AXON_ANDROID_BACK__?.()).toBe(false);
    expect(first).not.toHaveBeenCalled();
    expect(second).toHaveBeenCalledTimes(1);
  });

  it("does not expose the bridge outside Android", () => {
    renderHook(() =>
      useAndroidBackButton(
        false,
        vi.fn(() => true),
      ),
    );
    expect(window.__AXON_ANDROID_BACK__).toBeUndefined();
  });
});
