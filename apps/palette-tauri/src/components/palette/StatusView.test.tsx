// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { StatusView } from "./StatusView";

afterEach(cleanup);

const payload = {
  degraded: false,
  errors: [],
  totals: { crawl: 12, extract: 3, embed: 40, ingest: 7 },
  local_ingest_jobs: [
    { id: "job-abc", status: "running", target: "owner/repo" },
    { id: "job-done", status: "completed", target: "owner/old" },
  ],
};

describe("StatusView", () => {
  it("renders the per-family totals strip", () => {
    render(<StatusView payload={payload} />);
    expect(screen.getByText("12")).toBeInTheDocument();
    expect(screen.getByText("40")).toBeInTheDocument();
  });

  it("opens the live card for a running job, but not a completed one", () => {
    const onOpenJob = vi.fn();
    render(<StatusView payload={payload} onOpenJob={onOpenJob} />);
    fireEvent.click(screen.getByTitle("Open live ingest job"));
    expect(onOpenJob).toHaveBeenCalledWith("ingest", "job-abc", "owner/repo");
    // The completed row is not clickable.
    expect(screen.queryAllByTitle("Open live ingest job")).toHaveLength(1);
  });
});
