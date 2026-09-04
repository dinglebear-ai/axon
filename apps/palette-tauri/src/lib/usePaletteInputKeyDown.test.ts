import type { KeyboardEvent } from "react";
import { describe, expect, it, vi } from "vitest";

import { ACTIONS, type PaletteAction } from "@/lib/actions";
import { usePaletteInputKeyDown } from "@/lib/usePaletteInputKeyDown";

const ask = ACTIONS.find((action) => action.subcommand === "ask") as PaletteAction;

describe("usePaletteInputKeyDown", () => {
  it("executes the highlighted action when Enter is pressed", () => {
    const enterActionMode = vi.fn();
    const requestSubmit = vi.fn();
    const preventDefault = vi.fn();
    const onKeyDown = usePaletteInputKeyDown({
      active: ask,
      askFallback: false,
      askSessionsLength: 0,
      dispatchView: vi.fn(),
      enterActionMode,
      filteredLength: 1,
      modeAction: null,
      parsed: { search: "what are Claude Code hooks?", arg: "" },
      requestSubmit,
      setAskSessionsOpen: vi.fn(),
      setSelected: vi.fn(),
    });

    onKeyDown({
      key: "Enter",
      preventDefault,
      currentTarget: { value: "what are Claude Code hooks?" },
    } as unknown as KeyboardEvent<HTMLInputElement>);

    expect(preventDefault).toHaveBeenCalledOnce();
    expect(requestSubmit).toHaveBeenCalledWith(ask, "what are Claude Code hooks?");
    expect(enterActionMode).not.toHaveBeenCalled();
  });

  it("submits the live value on the first Enter in action mode", () => {
    const map = ACTIONS.find((action) => action.subcommand === "map") as PaletteAction;
    const requestSubmit = vi.fn();
    const onKeyDown = usePaletteInputKeyDown({
      active: map,
      askFallback: false,
      askSessionsLength: 0,
      dispatchView: vi.fn(),
      enterActionMode: vi.fn(),
      filteredLength: 1,
      modeAction: map,
      parsed: { search: "code.claude", arg: "" },
      requestSubmit,
      setAskSessionsOpen: vi.fn(),
      setSelected: vi.fn(),
    });

    onKeyDown({
      key: "Enter",
      preventDefault: vi.fn(),
      currentTarget: { value: "code.claude.com" },
    } as unknown as KeyboardEvent<HTMLInputElement>);

    expect(requestSubmit).toHaveBeenCalledWith(map, "code.claude.com");
  });

  it("uses the live command value when React's parsed snapshot is stale", () => {
    const map = ACTIONS.find((action) => action.subcommand === "map") as PaletteAction;
    const requestSubmit = vi.fn();
    const onKeyDown = usePaletteInputKeyDown({
      active: map,
      askFallback: false,
      askSessionsLength: 0,
      dispatchView: vi.fn(),
      enterActionMode: vi.fn(),
      filteredLength: 1,
      modeAction: null,
      parsed: { invoked: map, search: "map", arg: "" },
      requestSubmit,
      setAskSessionsOpen: vi.fn(),
      setSelected: vi.fn(),
    });

    onKeyDown({
      key: "Enter",
      preventDefault: vi.fn(),
      currentTarget: { value: "map code.claude.com" },
    } as unknown as KeyboardEvent<HTMLInputElement>);

    expect(requestSubmit).toHaveBeenCalledWith(map, "code.claude.com");
  });

  it("does not launch Terminal from type-ahead Enter before explicit Tab selection", () => {
    const terminal = ACTIONS.find((action) => action.subcommand === "terminal") as PaletteAction;
    const requestSubmit = vi.fn();
    const onKeyDown = usePaletteInputKeyDown({
      active: terminal,
      askFallback: false,
      askSessionsLength: 0,
      dispatchView: vi.fn(),
      enterActionMode: vi.fn(),
      filteredLength: 1,
      modeAction: null,
      parsed: { search: "t", arg: "" },
      requestSubmit,
      setAskSessionsOpen: vi.fn(),
      setSelected: vi.fn(),
    });

    onKeyDown({
      key: "Enter",
      preventDefault: vi.fn(),
      currentTarget: { value: "t" },
    } as unknown as KeyboardEvent<HTMLInputElement>);

    expect(requestSubmit).not.toHaveBeenCalled();
  });
});
