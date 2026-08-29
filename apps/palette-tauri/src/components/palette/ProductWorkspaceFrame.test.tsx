// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { ProductWorkspaceFrame } from "./ProductWorkspaceFrame";

afterEach(cleanup);

describe("ProductWorkspaceFrame", () => {
  it("renders children directly without product-workspace-shell for axon workspace", () => {
    render(
      <ProductWorkspaceFrame
        workspace="axon"
        available={new Set(["axon", "labby", "cortex"])}
        profiles={[]}
        activeProfileIds={{}}
        labbyProfile={null}
        cortexProfile={null}
        onSelect={() => {}}
        onSelectProfile={() => {}}
      >
        <div data-testid="palette-shell">Axon Palette Content</div>
      </ProductWorkspaceFrame>,
    );

    expect(screen.getByTestId("palette-shell")).toBeInTheDocument();
    expect(screen.queryByRole("navigation", { name: "Product workspaces" })).not.toBeInTheDocument();
    expect(document.querySelector(".product-workspace-shell")).not.toBeInTheDocument();
  });

  it("renders product navigation and missing profile message for unconfigured labby workspace", () => {
    render(
      <ProductWorkspaceFrame
        workspace="labby"
        available={new Set(["axon"])}
        profiles={[]}
        activeProfileIds={{}}
        labbyProfile={null}
        cortexProfile={null}
        onSelect={() => {}}
        onSelectProfile={() => {}}
      >
        <div data-testid="palette-shell">Axon Palette Content</div>
      </ProductWorkspaceFrame>,
    );

    expect(screen.queryByTestId("palette-shell")).not.toBeInTheDocument();
    expect(screen.getByRole("navigation", { name: "Product workspaces" })).toBeInTheDocument();
    expect(screen.getByText("Labby needs a backend profile")).toBeInTheDocument();
    expect(document.querySelector(".product-workspace-shell")).toBeInTheDocument();
  });
});
