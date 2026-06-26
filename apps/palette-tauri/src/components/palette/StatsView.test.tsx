// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { StatsView } from "./StatsView";

afterEach(cleanup);

describe("StatsView", () => {
  it("renders core collection metrics", () => {
    render(<StatsView payload={{ collection: "axon", indexed_vectors_count: 1000, docs_embedded_estimate: 50 }} />);
    expect(screen.getByText("1,000")).toBeInTheDocument();
    expect(screen.getByText(/Collection · axon/)).toBeInTheDocument();
  });

  it("renders a 7-day growth sparkline + total caption", () => {
    render(<StatsView payload={{ growth_7d: [1, 2, 3, 4] }} />);
    expect(screen.getByLabelText("Indexed-document growth over the last 7 days")).toBeInTheDocument();
    expect(screen.getByText("10 docs / 7d")).toBeInTheDocument();
  });

  it("does not show a delta on first render (baseline == current)", () => {
    render(<StatsView payload={{ indexed_vectors_count: 500 }} />);
    expect(screen.queryByText(/^\+/)).not.toBeInTheDocument();
  });
});
