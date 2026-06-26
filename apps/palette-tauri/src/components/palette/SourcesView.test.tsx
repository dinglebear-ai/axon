// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { SourcesView } from "./SourcesView";

afterEach(cleanup);

const payload = {
  count: 3,
  urls: [
    ["https://docs.rs/serde", 40],
    ["https://docs.rs/tokio", 120],
    ["https://example.com/page", 5],
  ],
};

describe("SourcesView", () => {
  it("renders rows, total chunk count, and URL count", () => {
    render(<SourcesView payload={payload} />);
    expect(screen.getByText("https://docs.rs/tokio")).toBeInTheDocument();
    expect(screen.getByText("165")).toBeInTheDocument(); // 40 + 120 + 5
  });

  it("filters by URL substring", () => {
    render(<SourcesView payload={payload} />);
    fireEvent.change(screen.getByLabelText("Filter sources by URL"), { target: { value: "tokio" } });
    expect(screen.getByText("https://docs.rs/tokio")).toBeInTheDocument();
    expect(screen.queryByText("https://example.com/page")).not.toBeInTheDocument();
  });

  it("seeds the filter from initialFilter (domain drill)", () => {
    render(<SourcesView payload={payload} initialFilter="example.com" />);
    expect(screen.getByText("https://example.com/page")).toBeInTheDocument();
    expect(screen.queryByText("https://docs.rs/tokio")).not.toBeInTheDocument();
  });

  it("fires onRunAction('retrieve', url) from a row", () => {
    const onRunAction = vi.fn();
    render(<SourcesView payload={payload} onRunAction={onRunAction} />);
    fireEvent.click(screen.getByLabelText("Retrieve https://docs.rs/tokio"));
    expect(onRunAction).toHaveBeenCalledWith("retrieve", "https://docs.rs/tokio");
  });
});
