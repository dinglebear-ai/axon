// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { SourcesView } from "./SourcesView";
import { buildSourcesModel, type SourceSortMode } from "@/lib/sourcesModel";

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
  function renderView(args: {
    filter?: string;
    sort?: SourceSortMode;
    grouped?: boolean;
    onRunAction?: (subcommand: string, argument: string) => void;
    onFilterChange?: (filter: string) => void;
  } = {}) {
    const filter = args.filter ?? "";
    const sort = args.sort ?? "chunks";
    const grouped = args.grouped ?? false;
    return render(
      <SourcesView
        model={buildSourcesModel(payload, filter, sort, grouped)}
        filter={filter}
        sort={sort}
        grouped={grouped}
        onRunAction={args.onRunAction}
        onFilterChange={args.onFilterChange ?? vi.fn()}
        onSortChange={vi.fn()}
        onGroupedChange={vi.fn()}
      />,
    );
  }

  it("renders rows, total chunk count, and URL count", () => {
    renderView();
    expect(screen.getByText("https://docs.rs/tokio")).toBeInTheDocument();
    expect(screen.getByText("165")).toBeInTheDocument(); // 40 + 120 + 5
  });

  it("filters by URL substring", () => {
    const onFilterChange = vi.fn();
    renderView({ filter: "tokio", onFilterChange });
    expect(screen.getByText("https://docs.rs/tokio")).toBeInTheDocument();
    expect(screen.queryByText("https://example.com/page")).not.toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Filter sources by URL"), { target: { value: "serde" } });
    expect(onFilterChange).toHaveBeenCalledWith("serde");
  });

  it("seeds the filter from initialFilter (domain drill)", () => {
    renderView({ filter: "example.com" });
    expect(screen.getByText("https://example.com/page")).toBeInTheDocument();
    expect(screen.queryByText("https://docs.rs/tokio")).not.toBeInTheDocument();
  });

  it("fires onRunAction('retrieve', url) from a row", () => {
    const onRunAction = vi.fn();
    renderView({ onRunAction });
    fireEvent.click(screen.getByLabelText("Retrieve https://docs.rs/tokio"));
    expect(onRunAction).toHaveBeenCalledWith("retrieve", "https://docs.rs/tokio");
  });
});
