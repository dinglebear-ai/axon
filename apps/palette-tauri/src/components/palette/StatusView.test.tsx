// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { StatusView } from "./StatusView";

afterEach(cleanup);

const payload = {
  degraded: false,
  errors: [],
  totals: { source: 12, extract: 3, watch: 4, prune: 1 },
  source_jobs: [
    { id: "job-abc", status: "running", target: "owner/repo" },
    { id: "job-done", status: "completed", target: "owner/old" },
  ],
};

describe("StatusView", () => {
  it("renders the per-family totals strip", () => {
    render(<StatusView payload={payload} />);
    expect(screen.getByText("12")).toBeInTheDocument();
    expect(screen.getByText("4")).toBeInTheDocument();
  });

  it("opens the live card for a running job, but not a completed one", () => {
    const onOpenJob = vi.fn();
    render(<StatusView payload={payload} onOpenJob={onOpenJob} />);
    fireEvent.click(screen.getByTitle("Open live source job"));
    expect(onOpenJob).toHaveBeenCalledWith("source", "job-abc", "owner/repo");
    // The completed row is not clickable.
    expect(screen.queryAllByTitle("Open live source job")).toHaveLength(1);
  });
});
