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

  it("rejects an out-of-range progress.phase and recomputes from status", () => {
    // A bogus phase string must NOT pass through — it falls back to the phase
    // derived from the raw status (here: running).
    const snap = summarizeJob(
      "extract",
      {
        job: { status: "running" },
        progress: { phase: "reticulating", percent: 10, metrics: [], error: null },
      },
      { jobId: "j5", label: "x" },
    );
    expect(snap.phase).toBe("running");
  });

  it("ignores a non-numeric progress.percent (renders indeterminate)", () => {
    const snap = summarizeJob(
      "embed",
      {
        job: { status: "running" },
        progress: { phase: "running", percent: "lots", metrics: [], error: null },
      },
      { jobId: "j6", label: "x" },
    );
    expect(snap.percent).toBeNull();
  });

  it("drops malformed metric entries (non-object, missing/empty label or value)", () => {
    const snap = summarizeJob(
      "ingest",
      {
        job: { status: "running" },
        progress: {
          phase: "running",
          percent: null,
          metrics: [
            { label: "Docs", value: "3" }, // kept
            { label: "", value: "9" }, // dropped: empty label
            { label: "Chunks", value: "" }, // dropped: empty value
            { label: "NoValue" }, // dropped: missing value
            "not-an-object", // dropped: not a record
            42, // dropped: not a record
          ],
          error: null,
        },
      },
      { jobId: "j7", label: "x" },
    );
    expect(snap.metrics).toEqual([{ label: "Docs", value: "3" }]);
  });

  it("treats a non-array progress.metrics as empty", () => {
    const snap = summarizeJob(
      "ingest",
      {
        job: { status: "running" },
        progress: { phase: "running", percent: null, metrics: { label: "x", value: "1" }, error: null },
      },
      { jobId: "j8", label: "x" },
    );
    expect(snap.metrics).toEqual([]);
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
