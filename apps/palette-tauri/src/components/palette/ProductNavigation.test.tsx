// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ProductNavigation } from "./ProductNavigation";

afterEach(cleanup);

describe("ProductNavigation", () => {
  it("keeps all product identities discoverable and marks unavailable profiles", () => {
    const onSelect = vi.fn();
    render(
      <ProductNavigation
        active="axon"
        available={new Set(["axon", "cortex"])}
        onSelect={onSelect}
      />,
    );

    expect(screen.getByRole("navigation", { name: "Product workspaces" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Axon\s*Knowledge & work/ })).toHaveAttribute(
      "aria-current",
      "page",
    );
    expect(
      screen.getByRole("button", { name: /Labby\s*Gateway & capabilities · setup required/ }),
    ).toBeEnabled();
    fireEvent.click(screen.getByRole("button", { name: /Cortex\s*Observability/ }));
    expect(onSelect).toHaveBeenCalledWith("cortex");
  });
});
