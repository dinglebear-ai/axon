// @vitest-environment jsdom
//
// TerminalView renders two very different modes depending on the invoke seam:
// the browser-dev fallback (no Tauri runtime — no real shell to spawn) shows a
// clear unavailable message, while the Tauri runtime drives real
// `terminal_cwd`/`terminal_run` invokes. Both paths are covered here by
// mocking `@/lib/invoke` per-test via `vi.doMock` + dynamic import, since
// `isTauriRuntime` is a module-level constant baked in at import time.

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

afterEach(() => {
  cleanup();
  vi.resetModules();
  vi.clearAllMocks();
});

describe("TerminalView — browser dev fallback", () => {
  beforeEach(() => {
    vi.doMock("@/lib/invoke", () => ({
      isTauriRuntime: false,
      invoke: vi.fn(() => Promise.reject(new Error("terminal_run is only available in the Tauri runtime"))),
      appWindow: { listen: async () => () => undefined },
    }));
  });

  it("shows a clear unavailable message instead of faking output", async () => {
    const { TerminalView } = await import("./TerminalView");
    render(<TerminalView />);
    expect(screen.getByText("Terminal requires the desktop app.")).toBeInTheDocument();
    expect(screen.queryByLabelText("Terminal command input")).not.toBeInTheDocument();
  });
});

describe("TerminalView — Tauri runtime", () => {
  const invokeMock = vi.fn();

  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockImplementation((command: string, args?: Record<string, unknown>) => {
      if (command === "terminal_cwd") return Promise.resolve("/home/operator");
      if (command === "terminal_run") {
        const cmd = String(args?.command ?? "");
        if (cmd === "pwd") {
          return Promise.resolve({ stdout: "/home/operator\n", stderr: "", exitCode: 0, cwd: "/home/operator" });
        }
        if (cmd === "false") {
          return Promise.resolve({ stdout: "", stderr: "", exitCode: 1, cwd: "/home/operator" });
        }
        return Promise.resolve({ stdout: "", stderr: "", exitCode: 0, cwd: "/home/operator" });
      }
      return Promise.reject(new Error(`unexpected invoke: ${command}`));
    });
    vi.doMock("@/lib/invoke", () => ({
      isTauriRuntime: true,
      invoke: invokeMock,
      appWindow: { listen: async () => () => undefined },
    }));
  });

  it("seeds the prompt from terminal_cwd on mount", async () => {
    const { TerminalView } = await import("./TerminalView");
    render(<TerminalView />);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("terminal_cwd"));
    await waitFor(() => expect(screen.getByText("~$")).toBeInTheDocument());
  });

  it("submits a command via terminal_run and renders real stdout", async () => {
    const { TerminalView } = await import("./TerminalView");
    render(<TerminalView />);
    const input = await screen.findByLabelText("Terminal command input");
    fireEvent.change(input, { target: { value: "pwd" } });
    fireEvent.keyDown(input, { key: "Enter" });

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("terminal_run", { command: "pwd" }),
    );
    expect(await screen.findByText("/home/operator")).toBeInTheDocument();
    // The submitted line is echoed as an "in" line before its output.
    expect(screen.getAllByText("pwd").length).toBeGreaterThan(0);
  });

  it("surfaces a nonzero exit code", async () => {
    const { TerminalView } = await import("./TerminalView");
    render(<TerminalView />);
    const input = await screen.findByLabelText("Terminal command input");
    fireEvent.change(input, { target: { value: "false" } });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(await screen.findByText("exit 1")).toBeInTheDocument();
  });

  it("clears scrollback on the local `clear` command without invoking the shell", async () => {
    const { TerminalView } = await import("./TerminalView");
    render(<TerminalView />);
    const input = await screen.findByLabelText("Terminal command input");
    fireEvent.change(input, { target: { value: "pwd" } });
    fireEvent.keyDown(input, { key: "Enter" });
    await screen.findByText("/home/operator");

    invokeMock.mockClear();
    fireEvent.change(input, { target: { value: "clear" } });
    fireEvent.keyDown(input, { key: "Enter" });

    await waitFor(() => expect(screen.queryByText("/home/operator")).not.toBeInTheDocument());
    expect(invokeMock).not.toHaveBeenCalledWith("terminal_run", expect.anything());
  });

  it("recalls previous commands with ArrowUp/ArrowDown history", async () => {
    const { TerminalView } = await import("./TerminalView");
    render(<TerminalView />);
    const input = (await screen.findByLabelText("Terminal command input")) as HTMLInputElement;

    fireEvent.change(input, { target: { value: "pwd" } });
    fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => expect(input.value).toBe(""));

    fireEvent.change(input, { target: { value: "echo two" } });
    fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => expect(input.value).toBe(""));

    fireEvent.keyDown(input, { key: "ArrowUp" });
    expect(input.value).toBe("echo two");
    fireEvent.keyDown(input, { key: "ArrowUp" });
    expect(input.value).toBe("pwd");
    fireEvent.keyDown(input, { key: "ArrowDown" });
    expect(input.value).toBe("echo two");
    fireEvent.keyDown(input, { key: "ArrowDown" });
    expect(input.value).toBe("");
  });
});
