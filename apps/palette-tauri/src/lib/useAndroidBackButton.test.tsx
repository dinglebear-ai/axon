// @vitest-environment jsdom

import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

const { onBackButtonPressMock, unregisterMock } = vi.hoisted(() => ({
  onBackButtonPressMock: vi.fn(),
  unregisterMock: vi.fn(() => Promise.resolve()),
}));

vi.mock("@tauri-apps/api/app", () => ({
  onBackButtonPress: onBackButtonPressMock,
}));

import { useAndroidBackButton } from "./useAndroidBackButton";

afterEach(() => {
  vi.clearAllMocks();
});

describe("useAndroidBackButton", () => {
  it("registers only when Android handling is enabled and cleans up", async () => {
    let nativeHandler: (() => void) | null = null;
    onBackButtonPressMock.mockImplementation(async (handler: () => void) => {
      nativeHandler = handler;
      return { unregister: unregisterMock };
    });
    const onBack = vi.fn();

    const { unmount } = renderHook(() => useAndroidBackButton(true, onBack));
    await waitFor(() => expect(onBackButtonPressMock).toHaveBeenCalledTimes(1));

    act(() => nativeHandler?.());
    expect(onBack).toHaveBeenCalledTimes(1);

    unmount();
    await waitFor(() => expect(unregisterMock).toHaveBeenCalledTimes(1));
  });

  it("does not register outside Android", () => {
    renderHook(() => useAndroidBackButton(false, vi.fn()));
    expect(onBackButtonPressMock).not.toHaveBeenCalled();
  });
});
