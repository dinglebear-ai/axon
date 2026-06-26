// Generic async-job progress model for the palette's live job view.
//
// `crawl` has its own richer model (`crawlJob.ts`) because it surfaces a live
// page frontier, depth, and a two-phase crawl→embed handoff. The other async
// job families — `embed`, `extract`, `ingest` — share a much simpler shape:
// poll `GET /v1/{family}/{id}`, read `status` + a handful of `result_json`
// counters, and render a status card until the job reaches a terminal state.
//
// Everything here is derived from REAL data polled from `axon serve`; the
// derivation (phase, percent, metrics) is pure and unit-tested. The view layer
// (`JobProgressView`) renders a `JobSnapshot` and never re-derives.

export type AsyncJobFamily = "embed" | "extract" | "ingest";

export type JobPhase = "pending" | "running" | "done" | "failed" | "canceled";

export interface JobMetric {
  label: string;
  value: string;
}

export interface JobSnapshot {
  family: AsyncJobFamily;
  jobId: string;
  /** Display label for the target (repo slug, URL, embed input). */
  label: string;
  /** Raw backend status string. */
  status: string;
  phase: JobPhase;
  /** 0–100 when determinate; `null` means indeterminate (pulsing bar). */
  percent: number | null;
  /** Key counters pulled from `result_json`, family-specific. */
  metrics: JobMetric[];
  errorText: string | null;
  /** Epoch ms of the last backend write (heartbeat or progress persist). */
  updatedAtMs: number | null;
  startedAtMs: number | null;
}

export function isJobPhaseTerminal(phase: JobPhase): boolean {
  return phase === "done" || phase === "failed" || phase === "canceled";
}

const FAMILY_VERB: Record<AsyncJobFamily, string> = {
  embed: "Embedding",
  extract: "Extracting",
  ingest: "Ingesting",
};

/** Present-tense gerund for a running job of this family ("Ingesting"). */
export function jobFamilyVerb(family: AsyncJobFamily): string {
  return FAMILY_VERB[family];
}

interface JsonRecord {
  [key: string]: unknown;
}

function asRecord(value: unknown): JsonRecord | null {
  return value && typeof value === "object" && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function num(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function str(value: unknown): string | null {
  return typeof value === "string" && value.length > 0 ? value : null;
}

/** Pull the job object out of `{ job: {...} }` or a bare job payload. */
function jobFromPayload(payload: unknown): JsonRecord | null {
  const root = asRecord(payload);
  if (!root) return null;
  return asRecord(root.job) ?? root;
}

function phaseFor(status: string): JobPhase {
  switch (status) {
    case "pending":
      return "pending";
    case "completed":
      return "done";
    case "failed":
      return "failed";
    case "canceled":
    case "cancelled":
      return "canceled";
    case "running":
      return "running";
    default:
      return "running";
  }
}

function parseTimestamp(value: unknown): number | null {
  const s = str(value);
  if (!s) return null;
  const ms = Date.parse(s);
  return Number.isNaN(ms) ? null : ms;
}

/** Append a metric only when its source value is present (non-null number). */
function pushNum(metrics: JobMetric[], label: string, value: unknown): void {
  const n = num(value);
  if (n != null) metrics.push({ label, value: n.toLocaleString() });
}

function embedMetrics(result: JsonRecord): JobMetric[] {
  const metrics: JobMetric[] = [];
  pushNum(metrics, "Docs", result.docs_embedded);
  pushNum(metrics, "Chunks", result.chunks_embedded);
  return metrics;
}

function extractMetrics(result: JsonRecord): JobMetric[] {
  const metrics: JobMetric[] = [];
  pushNum(metrics, "Pages", result.pages_visited);
  pushNum(metrics, "With data", result.pages_with_data);
  pushNum(metrics, "Items", result.total_items);
  return metrics;
}

function ingestMetrics(result: JsonRecord): JobMetric[] {
  const metrics: JobMetric[] = [];
  const phase = str(result.phase);
  if (phase) metrics.push({ label: "Phase", value: phase });
  // Files are split across AST-chunked + prose-fallback counters.
  const filesAst = num(result.files_ast_chunked);
  const filesProse = num(result.files_prose_fallback);
  if (filesAst != null || filesProse != null) {
    metrics.push({ label: "Files", value: ((filesAst ?? 0) + (filesProse ?? 0)).toLocaleString() });
  }
  pushNum(metrics, "Chunks", result.chunks_embedded ?? result.chunks);
  return metrics;
}

function metricsFor(family: AsyncJobFamily, result: JsonRecord): JobMetric[] {
  switch (family) {
    case "embed":
      return embedMetrics(result);
    case "extract":
      return extractMetrics(result);
    case "ingest":
      return ingestMetrics(result);
  }
}

/** Determinate percent for ingest when the reporter exposes task counts. */
function determinatePercent(family: AsyncJobFamily, result: JsonRecord, phase: JobPhase): number | null {
  if (phase === "done") return 100;
  if (phase !== "running" && phase !== "pending") return null;
  if (family === "ingest") {
    const done = num(result.tasks_done);
    const total = num(result.tasks_total);
    if (done != null && total != null && total > 0) {
      return Math.max(0, Math.min(100, (done / total) * 100));
    }
  }
  return null; // indeterminate — view shows a pulsing bar
}

export interface JobSummaryInput {
  jobId: string;
  /** Display label for the target (repo, URL, input). */
  label: string;
}

/**
 * Map a polled job payload into a `JobSnapshot`. Pure: no I/O, no clock reads.
 */
export function summarizeJob(
  family: AsyncJobFamily,
  payload: unknown,
  input: JobSummaryInput,
): JobSnapshot {
  const job = jobFromPayload(payload) ?? {};
  const result = asRecord(job.result_json) ?? {};
  const status = str(job.status) ?? "pending";
  const phase = phaseFor(status);

  return {
    family,
    jobId: input.jobId,
    label: input.label,
    status,
    phase,
    percent: determinatePercent(family, result, phase),
    metrics: metricsFor(family, result),
    errorText: str(job.error_text),
    updatedAtMs: parseTimestamp(job.updated_at),
    startedAtMs: parseTimestamp(job.started_at),
  };
}

/** A pending placeholder snapshot shown immediately on submit. */
export function pendingJobSnapshot(family: AsyncJobFamily, label: string): JobSnapshot {
  return summarizeJob(family, { job: { status: "pending" } }, { jobId: "", label });
}
