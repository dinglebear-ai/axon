// @vitest-environment jsdom
//
// Render tests for JobProgressView — the live status card for embed/extract/
// ingest jobs. Proves the card renders family-aware copy, metrics, terminal
// states, and that Cancel only appears while the job is active. jest-dom
// matchers are registered globally via src/test/setup.ts.

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { JobProgressView } from "./JobProgressView";
import { summarizeJob } from "@/lib/jobProgress";

afterEach(cleanup);

const noop = () => {};

describe("JobProgressView", () => {
  it("renders a running ingest job with family verb, label, and Cancel", () => {
    const snapshot = summarizeJob(
      "ingest",
      { job: { status: "running", result_json: { phase: "ingesting", tasks_done: 1, tasks_total: 4 } } },
      { jobId: "abc123", label: "unraid/api" },
    );
    render(
      <JobProgressView
        snapshot={snapshot}
        nowMs={Date.now()}
        canceling={false}
        onCancel={noop}
        onMinimize={noop}
        onClose={noop}
      />,
    );
    expect(screen.getByText(/Ingesting unraid\/api/)).toBeInTheDocument();
    expect(screen.getByText("job abc123")).toBeInTheDocument();
    expect(screen.getByText("Cancel job")).toBeInTheDocument();
  });

  it("hides Cancel and surfaces the error on a failed job", () => {
    const snapshot = summarizeJob(
      "ingest",
      { job: { status: "failed", error_text: "github_repo target not found: owner/typo" } },
      { jobId: "j9", label: "owner/typo" },
    );
    render(
      <JobProgressView
        snapshot={snapshot}
        nowMs={Date.now()}
        canceling={false}
        onCancel={noop}
        onMinimize={noop}
        onClose={noop}
      />,
    );
    expect(screen.queryByText("Cancel job")).not.toBeInTheDocument();
    expect(screen.getByText(/not found/)).toBeInTheDocument();
  });

  it("fires onCancel when Cancel is clicked", () => {
    const onCancel = vi.fn();
    const snapshot = summarizeJob("embed", { job: { status: "running" } }, { jobId: "e1", label: "notes" });
    render(
      <JobProgressView
        snapshot={snapshot}
        nowMs={Date.now()}
        canceling={false}
        onCancel={onCancel}
        onMinimize={noop}
        onClose={noop}
      />,
    );
    fireEvent.click(screen.getByText("Cancel job"));
    expect(onCancel).toHaveBeenCalledOnce();
  });
});
