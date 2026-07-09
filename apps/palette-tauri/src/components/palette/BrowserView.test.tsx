// @vitest-environment jsdom
//
// Render tests for BrowserView. Covers the dev-mode fallback (no Tauri
// runtime — must show the "requires desktop app" message and never call
// `invoke`), the Tauri-runtime path (opens the browser window on mount and
// closes it on unmount), address-bar navigation, and tab management.

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.fn((_command: string, _args?: Record<string, unknown>) =>
  Promise.resolve(undefined),
);
const runtimeState = { isTauriRuntime: false };

vi.mock("@/lib/invoke", () => ({
  get isTauriRuntime() {
    return runtimeState.isTauriRuntime;
  },
  invoke: (command: string, args?: Record<string, unknown>) => invokeMock(command, args),
  appWindow: { listen: () => Promise.resolve(() => {}) },
}));

import { BrowserView } from "./BrowserView";

beforeEach(() => {
  invokeMock.mockClear();
  runtimeState.isTauriRuntime = false;
});

afterEach(() => {
  cleanup();
});

describe("BrowserView — dev-mode fallback", () => {
  it("shows the desktop-app-required message and never calls invoke", () => {
    render(<BrowserView initialTarget={null} onClose={() => {}} />);

    expect(screen.getByText(/Browser tool requires the desktop app/i)).toBeInTheDocument();
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("still renders the address bar and tab strip chrome", () => {
    render(<BrowserView initialTarget={null} onClose={() => {}} />);

    expect(screen.getByLabelText(/address bar/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/^new tab$/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /close browser/i })).toBeInTheDocument();
  });

  it("calls onClose when the close-browser button is clicked", () => {
    const onClose = vi.fn();
    render(<BrowserView initialTarget={null} onClose={onClose} />);

    fireEvent.click(screen.getByRole("button", { name: /close browser/i }));

    expect(onClose).toHaveBeenCalledTimes(1);
  });
});

describe("BrowserView — Tauri runtime", () => {
  beforeEach(() => {
    runtimeState.isTauriRuntime = true;
  });

  it("opens the browser window on mount with the normalized initial target", () => {
    render(<BrowserView initialTarget="example.com" onClose={() => {}} />);

    expect(invokeMock).toHaveBeenCalledWith("browser_open", { url: "https://example.com" });
  });

  it("opens to the home sentinel when no initial target is given", () => {
    render(<BrowserView initialTarget={null} onClose={() => {}} />);

    expect(invokeMock).toHaveBeenCalledWith("browser_open", { url: "about:blank" });
  });

  it("closes the browser window on unmount", () => {
    const { unmount } = render(<BrowserView initialTarget={null} onClose={() => {}} />);
    invokeMock.mockClear();

    unmount();

    expect(invokeMock).toHaveBeenCalledWith("browser_close", undefined);
  });

  it("navigates via the address bar on submit", () => {
    render(<BrowserView initialTarget={null} onClose={() => {}} />);
    invokeMock.mockClear();

    const input = screen.getByLabelText(/address bar/i);
    fireEvent.change(input, { target: { value: "docs.rs/serde" } });
    fireEvent.submit(input.closest("form") as HTMLFormElement);

    expect(invokeMock).toHaveBeenCalledWith("browser_navigate", { url: "https://docs.rs/serde" });
  });

  it("opens a new tab and navigates the browser window to its home state", () => {
    render(<BrowserView initialTarget="example.com" onClose={() => {}} />);
    invokeMock.mockClear();

    fireEvent.click(screen.getByLabelText(/^new tab$/i));

    expect(invokeMock).toHaveBeenCalledWith("browser_navigate", { url: "about:blank" });
    expect(screen.getAllByText("New Tab").length).toBeGreaterThan(0);
  });

  it("drives back/forward/reload through their respective commands", () => {
    render(<BrowserView initialTarget={null} onClose={() => {}} />);
    invokeMock.mockClear();

    fireEvent.click(screen.getByRole("button", { name: /^back$/i }));
    fireEvent.click(screen.getByRole("button", { name: /^forward$/i }));
    fireEvent.click(screen.getByRole("button", { name: /^reload$/i }));

    expect(invokeMock).toHaveBeenCalledWith("browser_back", undefined);
    expect(invokeMock).toHaveBeenCalledWith("browser_forward", undefined);
    expect(invokeMock).toHaveBeenCalledWith("browser_reload", undefined);
  });
});
