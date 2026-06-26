// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { DomainsView } from "./DomainsView";

afterEach(cleanup);

const payload = {
  domains: [
    { domain: "docs.rs", vectors: 200 },
    { domain: "example.com", vectors: 50 },
  ],
};

describe("DomainsView", () => {
  it("renders domains sorted by vectors with totals", () => {
    render(<DomainsView payload={payload} />);
    expect(screen.getByText("docs.rs")).toBeInTheDocument();
    expect(screen.getByText("250")).toBeInTheDocument(); // total vectors
  });

  it("drills into a domain on click", () => {
    const onDrillDomain = vi.fn();
    render(<DomainsView payload={payload} onDrillDomain={onDrillDomain} />);
    fireEvent.click(screen.getByTitle("Show sources for docs.rs"));
    expect(onDrillDomain).toHaveBeenCalledWith("docs.rs");
  });
});
