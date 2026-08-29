// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { PaletteFooter } from "./PaletteFooter";

afterEach(() => cleanup());

describe("PaletteFooter", () => {
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
        config={null}
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
