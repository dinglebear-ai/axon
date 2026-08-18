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
        onHide={vi.fn()}
        showHide={false}
      />,
    );

    expect(screen.getByRole("button", { name: "Settings" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Hide palette" })).not.toBeInTheDocument();
  });
});
