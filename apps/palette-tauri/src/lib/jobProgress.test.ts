import { describe, expect, it } from "vitest";

import {
  isJobPhaseTerminal,
  jobFamilyVerb,
  pendingJobSnapshot,
  summarizeJob,
} from "./jobProgress";

describe("summarizeJob", () => {
  it("maps a pending job to the pending phase with an indeterminate bar", () => {
    const snap = summarizeJob("ingest", { job: { status: "pending" } }, { jobId: "j1", label: "unraid/api" });
    expect(snap.phase).toBe("pending");
    expect(snap.percent).toBeNull();
    expect(snap.label).toBe("unraid/api");
  });

  it("maps a completed job to done at 100%", () => {
    const snap = summarizeJob(
      "embed",
      { job: { status: "completed", result_json: { docs_embedded: 3, chunks_embedded: 42 } } },
      { jobId: "j2", label: "notes" },
    );
    expect(snap.phase).toBe("done");
    expect(snap.percent).toBe(100);
    expect(snap.metrics).toEqual([
      { label: "Docs", value: "3" },
      { label: "Chunks", value: "42" },
    ]);
  });

  it("derives ingest percent from task counts when present", () => {
    const snap = summarizeJob(
      "ingest",
      { job: { status: "running", result_json: { phase: "ingesting", tasks_done: 2, tasks_total: 5 } } },
      { jobId: "j3", label: "owner/repo" },
    );
    expect(snap.phase).toBe("running");
    expect(snap.percent).toBe(40);
    expect(snap.metrics[0]).toEqual({ label: "Phase", value: "ingesting" });
  });

  it("stays indeterminate for a running job without task counts", () => {
    const snap = summarizeJob(
      "extract",
      { job: { status: "running", result_json: { pages_visited: 4, total_items: 9 } } },
      { jobId: "j4", label: "example.com" },
    );
    expect(snap.percent).toBeNull();
    expect(snap.metrics).toEqual([
      { label: "Pages", value: "4" },
      { label: "Items", value: "9" },
    ]);
  });

  it("surfaces error text on a failed job", () => {
    const snap = summarizeJob(
      "ingest",
      { job: { status: "failed", error_text: "github_repo target not found: owner/typo" } },
      { jobId: "j5", label: "owner/typo" },
    );
    expect(snap.phase).toBe("failed");
    expect(snap.errorText).toContain("not found");
  });

  it("accepts a bare job payload (no { job } wrapper)", () => {
    const snap = summarizeJob("embed", { status: "running" }, { jobId: "j6", label: "x" });
    expect(snap.phase).toBe("running");
  });
});

describe("isJobPhaseTerminal", () => {
  it("treats done/failed/canceled as terminal and pending/running as not", () => {
    expect(isJobPhaseTerminal("done")).toBe(true);
    expect(isJobPhaseTerminal("failed")).toBe(true);
    expect(isJobPhaseTerminal("canceled")).toBe(true);
    expect(isJobPhaseTerminal("running")).toBe(false);
    expect(isJobPhaseTerminal("pending")).toBe(false);
  });
});

describe("pendingJobSnapshot", () => {
  it("produces a pending snapshot for the given family + label", () => {
    const snap = pendingJobSnapshot("ingest", "owner/repo");
    expect(snap.phase).toBe("pending");
    expect(snap.family).toBe("ingest");
    expect(snap.label).toBe("owner/repo");
    expect(jobFamilyVerb(snap.family)).toBe("Ingesting");
  });
});
