// @vitest-environment jsdom

import { renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { MOBILE_ACTIONS } from "@/lib/actions";
import { usePaletteSelection } from "@/lib/usePaletteSelection";

function selection(query: string) {
  return renderHook(() =>
    usePaletteSelection({
      actions: MOBILE_ACTIONS,
      browseOpen: false,
      browserOpen: false,
      history: [],
      historyOpen: false,
      modeAction: null,
      pendingConfirmation: null,
      query,
      run: { kind: "idle" },
      selected: 0,
      setSelected: vi.fn(),
      settingsOpen: false,
    }),
  );
}

describe("usePaletteSelection mobile catalog", () => {
  it("does not surface desktop-only actions in search results", () => {
    const rendered = selection("terminal");
    expect(
      rendered.result.current.filtered.some((action) => action.subcommand === "terminal"),
    ).toBe(false);
  });

  it("rejects slash invocation of an action removed from the mobile catalog", () => {
    const rendered = selection("/terminal");
    expect(rendered.result.current.filtered).toEqual([]);
    expect(rendered.result.current.active).toBeUndefined();
    expect(rendered.result.current.validation).toBe("No matching action");
  });

  it("keeps remote actions available on mobile", () => {
    const rendered = selection("ask");
    expect(rendered.result.current.filtered.some((action) => action.subcommand === "ask")).toBe(
      true,
    );
  });
});
