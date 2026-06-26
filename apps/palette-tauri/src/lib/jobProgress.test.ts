import { describe, expect, it } from "vitest";

import {
  isJobPhaseTerminal,
  jobFamilyVerb,
  pendingJobSnapshot,
  summarizeJob,
} from "./jobProgress";

describe("summarizeJob", () => {
  it("reads the server-derived progress block (phase / percent / metrics)", () => {
    const snap = summarizeJob(
      "ingest",
      {
        job: { status: "running", updated_at: "2026-06-26T00:00:00Z" },
        progress: {
          family: "ingest",
          phase: "running",
          percent: 40,
          metrics: [
            { label: "Phase", value: "ingesting" },
            { label: "Chunks", value: "1,024" },
          ],
          error: null,
        },
      },
      { jobId: "j1", label: "owner/repo" },
    );
    expect(snap.phase).toBe("running");
    expect(snap.percent).toBe(40);
    expect(snap.metrics).toEqual([
      { label: "Phase", value: "ingesting" },
      { label: "Chunks", value: "1,024" },
    ]);
    expect(snap.label).toBe("owner/repo");
  });

  it("surfaces error from the progress block on a failed job", () => {
    const snap = summarizeJob(
      "ingest",
      {
        job: { status: "failed" },
        progress: { phase: "failed", percent: null, metrics: [], error: "github_repo target not found: owner/typo" },
      },
      { jobId: "j2", label: "owner/typo" },
    );
    expect(snap.phase).toBe("failed");
    expect(snap.errorText).toContain("not found");
  });

  it("falls back to a status-only snapshot when there is no progress (202 accept response)", () => {
    const snap = summarizeJob("embed", { job_id: "j3", status: "pending" }, { jobId: "j3", label: "notes" });
    expect(snap.phase).toBe("pending");
    expect(snap.percent).toBeNull();
    expect(snap.metrics).toEqual([]);
  });

  it("fallback marks a completed status as done at 100%", () => {
    const snap = summarizeJob("embed", { job: { status: "completed" } }, { jobId: "j4", label: "x" });
    expect(snap.phase).toBe("done");
    expect(snap.percent).toBe(100);
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
