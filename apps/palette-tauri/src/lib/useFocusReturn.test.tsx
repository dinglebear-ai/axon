// @vitest-environment jsdom
import "@testing-library/jest-dom/vitest";
import { render, renderHook } from "@testing-library/react";
import type { RefObject } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { RunState } from "@/lib/runState";
import {
  usePaletteHotkeys,
  useFocusReturn,
  type PaletteHotkeyActions,
  type PaletteHotkeyState,
} from "@/lib/useFocusReturn";

const { invokeMock } = vi.hoisted(() => ({
  invokeMock: vi.fn((..._args: unknown[]) => Promise.resolve()),
}));
vi.mock("@/lib/invoke", () => ({ invoke: invokeMock }));
vi.mock("@/lib/paletteView", () => ({ focusInput: vi.fn() }));

const idle: RunState = { kind: "idle" };
const withText: RunState = {
  kind: "success",
  title: "t",
  subtitle: "s",
  text: "answer body",
  outputKind: "markdown",
  result: { ok: true, status: 200, path: "/v1/ask", method: "POST", payload: {} },
};

function baseState(overrides: Partial<PaletteHotkeyState> = {}): PaletteHotkeyState {
  return {
    settingsOpen: false,
    historyOpen: false,
    browseOpen: false,
    query: "",
    modeAction: null,
    run: idle,
    ...overrides,
  };
}

function makeActions(): PaletteHotkeyActions & Record<string, ReturnType<typeof vi.fn>> {
  return {
    closeSettings: vi.fn(),
    toBrowseFromHistory: vi.fn(),
    closeBrowse: vi.fn(),
    clearMode: vi.fn(),
    clearQuery: vi.fn(),
    copyOutput: vi.fn(),
  };
}

function pressEscape() {
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
}

describe("usePaletteHotkeys — Escape precedence ladder (I2)", () => {
  let actions: ReturnType<typeof makeActions>;
  let stateRef: RefObject<PaletteHotkeyState>;

  beforeEach(() => {
    invokeMock.mockClear();
    actions = makeActions();
    stateRef = { current: baseState() };
    renderHook(() => usePaletteHotkeys(stateRef, actions));
  });
  afterEach(() => vi.clearAllMocks());

  it("settings takes precedence over everything", () => {
    stateRef.current = baseState({ settingsOpen: true, historyOpen: true, query: "x" });
    pressEscape();
    expect(actions.closeSettings).toHaveBeenCalledTimes(1);
    expect(actions.toBrowseFromHistory).not.toHaveBeenCalled();
    expect(actions.clearQuery).not.toHaveBeenCalled();
  });

  it("history before browse/query when settings closed", () => {
    stateRef.current = baseState({ historyOpen: true, query: "x" });
    pressEscape();
    expect(actions.toBrowseFromHistory).toHaveBeenCalledTimes(1);
    expect(actions.clearQuery).not.toHaveBeenCalled();
  });

  it("clears mode before query when a mode action is active and query empty", () => {
    stateRef.current = baseState({ modeAction: { kind: "remote", label: "Scrape", subcommand: "scrape" } as never });
    pressEscape();
    expect(actions.clearMode).toHaveBeenCalledTimes(1);
    expect(actions.clearQuery).not.toHaveBeenCalled();
  });

  it("clears the query when only a query is present", () => {
    stateRef.current = baseState({ query: "rust docs" });
    pressEscape();
    expect(actions.clearQuery).toHaveBeenCalledTimes(1);
  });

  it("hides the palette when nothing else is open", () => {
    stateRef.current = baseState();
    pressEscape();
    expect(invokeMock).toHaveBeenCalledWith("hide_palette");
  });

  it("re-binding fix: ladder still resolves after the state object identity changes", () => {
    // App reassigns stateRef.current to a fresh object every render; the listener
    // must keep reading the latest via the stable ref (P-H2 regression guard).
    stateRef.current = baseState({ query: "first" });
    pressEscape();
    stateRef.current = baseState({ settingsOpen: true });
    pressEscape();
    expect(actions.clearQuery).toHaveBeenCalledTimes(1);
    expect(actions.closeSettings).toHaveBeenCalledTimes(1);
  });
});

describe("usePaletteHotkeys — Cmd/Ctrl+C copy guard (I2)", () => {
  it("copies output text, but not when focus is in a text field", () => {
    const actions = makeActions();
    const stateRef: RefObject<PaletteHotkeyState> = { current: baseState({ run: withText }) };
    renderHook(() => usePaletteHotkeys(stateRef, actions));

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "c", ctrlKey: true, bubbles: true }));
    expect(actions.copyOutput).toHaveBeenCalledWith("answer body");

    vi.mocked(actions.copyOutput).mockClear();
    const input = document.createElement("input");
    document.body.appendChild(input);
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "c", ctrlKey: true, bubbles: true }));
    expect(actions.copyOutput).not.toHaveBeenCalled();
    input.remove();
  });
});

describe("useFocusReturn — overlay focus capture/restore (I1)", () => {
  function Overlay({ open }: { open: boolean }) {
    const ref = useFocusReturn<HTMLDivElement>(open);
    return open ? (
      <div ref={ref} tabIndex={-1} data-testid="overlay">
        <button type="button">inside</button>
      </div>
    ) : null;
  }

  it("moves focus into the overlay on open and restores it on close", () => {
    const opener = document.createElement("button");
    document.body.appendChild(opener);
    opener.focus();
    expect(document.activeElement).toBe(opener);

    const { rerender } = render(<Overlay open={false} />);
    rerender(<Overlay open />);
    // Focus landed on a focusable descendant (the button) inside the overlay.
    expect(document.activeElement?.textContent).toBe("inside");

    rerender(<Overlay open={false} />);
    expect(document.activeElement).toBe(opener);
    opener.remove();
  });

  it("does not throw when the opener was removed while the overlay was open", () => {
    const opener = document.createElement("button");
    document.body.appendChild(opener);
    opener.focus();

    const { rerender } = render(<Overlay open={false} />);
    rerender(<Overlay open />);
    opener.remove(); // opener disconnected while overlay is open
    expect(() => rerender(<Overlay open={false} />)).not.toThrow();
  });
});
