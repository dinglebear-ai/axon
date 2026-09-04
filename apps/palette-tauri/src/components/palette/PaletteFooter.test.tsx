// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { PaletteFooter } from "./PaletteFooter";

afterEach(() => cleanup());

describe("PaletteFooter", () => {
  it("removes the desktop footer from layout when hints are disabled", () => {
    const { container } = render(
      <PaletteFooter
        config={{
          serverUrl: "http://127.0.0.1:8001",
          token: null,
          shortcut: "Ctrl+Space",
          collection: "axon",
          resultLimit: 10,
          theme: "dark",
          hideOnBlur: false,
          showFooterHints: false,
        }}
        configError={null}
        onRecent={vi.fn()}
        onSettings={vi.fn()}
        onCodex={vi.fn()}
        onHide={vi.fn()}
      />,
    );

    expect(container).toBeEmptyDOMElement();
  });

  it("hides the desktop-only hide control on mobile", () => {
    render(
      <PaletteFooter
        config={null}
        configError={null}
        onRecent={vi.fn()}
        onSettings={vi.fn()}
        onCodex={vi.fn()}
        onHide={vi.fn()}
        onHome={vi.fn()}
        mobile
        showHide={false}
      />,
    );

    expect(screen.getByRole("navigation", { name: "Palette navigation" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Home" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Recent" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Settings" })).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: "Codex" })).toHaveLength(1);
    expect(screen.queryByRole("button", { name: "Codex app-server" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Hide palette" })).not.toBeInTheDocument();
  });

  it("opens Codex control from the desktop footer", () => {
    const onCodex = vi.fn();
    render(
      <PaletteFooter
        config={{
          serverUrl: "http://127.0.0.1:8001",
          token: null,
          shortcut: "Ctrl+Space",
          collection: "axon",
          resultLimit: 10,
          theme: "dark",
          hideOnBlur: false,
          showFooterHints: true,
        }}
        configError={null}
        onRecent={vi.fn()}
        onSettings={vi.fn()}
        onCodex={onCodex}
        onHide={vi.fn()}
      />,
    );

    screen.getByRole("button", { name: "Codex app-server" }).click();
    expect(onCodex).toHaveBeenCalledOnce();
  });
});
