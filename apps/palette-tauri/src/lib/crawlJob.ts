// Live crawl-job model for the palette's running-crawl view.
//
// Everything here is derived from REAL data polled from `axon serve`:
//   - GET /v1/crawl/{id}        → crawl job row (status + result_json progress)
//   - GET /v1/embed/{id}        → embed job row (phase-2: docs/chunks embedded)
//
// The crawl runner persists incremental progress into `result_json` while
// running (pages_crawled, md_created, …) and — once the Tier-2 event stream
// lands — a bounded ring of per-page events plus queued/depth/rate-limit. This
// module maps that raw shape into a `CrawlSnapshot` the view renders, and keeps
// the derivation (percent, ETA, phase) pure and unit-testable.

export type CrawlPhase = "pending" | "crawling" | "embedding" | "done" | "failed" | "canceled";

export interface CrawlLogEvent {
  /** Milliseconds since the crawl started (monotonic-ish, from the backend). */
  t: number;
  kind: "fetch" | "embed" | "info" | "warn";
  url?: string;
  status?: number;
  links?: number;
  chunks?: number;
  batch?: number;
  text?: string;
}

export interface RateLimitHost {
  host: string;
  backoffMs: number;
}

export interface CrawlSnapshot {
  jobId: string;
  host: string;
  url: string;
  status: string;
  phase: CrawlPhase;
  fetched: number;
  queued: number;
  /** Markdown docs written to disk during the crawl (pre-embed). */
  docs: number;
  /** Docs embedded into Qdrant — only non-zero in the embed phase. */
  embedded: number;
  chunks: number;
  depthCurrent: number | null;
  depthMax: number | null;
  /** 0–100, derived from fetched / (fetched + queued) or a page cap. */
  percent: number;
  etaText: string | null;
  events: CrawlLogEvent[];
  rateLimited: RateLimitHost[];
  errorCount: number;
  errorText: string | null;
  embedJobId: string | null;
  /**
   * Epoch ms of the last backend write to this row — either a progress-persister
   * write or the 30s worker heartbeat (`touch_heartbeat`). A recent value proves
   * the job is genuinely alive; a stale one means the worker went quiet. Drives
   * the TAILING-vs-stalled state honestly instead of assuming "running == live".
   */
  updatedAtMs: number | null;
  startedAtMs: number | null;
}

/** True when the heartbeat/progress write is recent enough to call the job live. */
export function isLive(snap: CrawlSnapshot, nowMs: number): boolean {
  if (snap.phase !== "crawling" && snap.phase !== "pending" && snap.phase !== "embedding") {
    return false;
  }
  if (snap.updatedAtMs == null) return true; // no timestamp yet — assume live
  // Heartbeat fires every 30s; allow a generous margin before calling it stalled.
  return nowMs - snap.updatedAtMs < 45_000;
}

interface JsonRecord {
  [key: string]: unknown;
}

function asRecord(value: unknown): JsonRecord | null {
  return value && typeof value === "object" && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function num(value: unknown): number {
  return typeof value === "number" && Number.isFinite(value) ? value : 0;
}

function optNum(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function str(value: unknown): string | null {
  return typeof value === "string" && value.length > 0 ? value : null;
}

/** Pull the job object out of `{ job: {...} }` or a bare job payload. */
export function jobFromPayload(payload: unknown): JsonRecord | null {
  const root = asRecord(payload);
  if (!root) return null;
  return asRecord(root.job) ?? root;
}

export function hostFromUrl(url: string): string {
  try {
    return new URL(url).host.replace(/^www\./, "");
  } catch {
    return url.replace(/^https?:\/\//, "").split("/")[0] || url;
  }
}

/** Human ETA from a remaining-count and a rate (pages/sec). */
export function formatEta(remaining: number, ratePerSec: number): string | null {
  if (remaining <= 0 || ratePerSec <= 0 || !Number.isFinite(ratePerSec)) return null;
  const secs = Math.round(remaining / ratePerSec);
  if (secs < 60) return `est. ${secs}s left`;
  const mins = Math.round(secs / 60);
  if (mins < 60) return `est. ${mins} min left`;
  const hrs = Math.floor(mins / 60);
  const rem = mins % 60;
  return rem ? `est. ${hrs}h ${rem}m left` : `est. ${hrs}h left`;
}

function phaseFor(status: string): CrawlPhase {
  switch (status) {
    case "pending":
      return "pending";
    case "running":
      return "crawling";
    case "completed":
      return "done";
    case "failed":
      return "failed";
    case "canceled":
    case "cancelled":
      return "canceled";
    default:
      return "crawling";
  }
}

function parseEvents(raw: unknown): CrawlLogEvent[] {
  if (!Array.isArray(raw)) return [];
  const events: CrawlLogEvent[] = [];
  for (const item of raw) {
    const rec = asRecord(item);
    if (!rec) continue;
    const kind = str(rec.kind);
    events.push({
      t: num(rec.t),
      kind: kind === "embed" || kind === "warn" || kind === "info" ? kind : "fetch",
      url: str(rec.url) ?? undefined,
      status: optNum(rec.status) ?? undefined,
      links: optNum(rec.links) ?? undefined,
      chunks: optNum(rec.chunks) ?? undefined,
      batch: optNum(rec.batch) ?? undefined,
      text: str(rec.text) ?? undefined,
    });
  }
  return events;
}

function parseRateLimited(raw: unknown): RateLimitHost[] {
  if (!Array.isArray(raw)) return [];
  const hosts: RateLimitHost[] = [];
  for (const item of raw) {
    const rec = asRecord(item);
    if (!rec) continue;
    const host = str(rec.host);
    if (!host) continue;
    hosts.push({ host, backoffMs: num(rec.backoff_ms ?? rec.backoffMs) });
  }
  return hosts;
}

export interface CrawlSummaryInput {
  jobId: string;
  url: string;
  /** Optional configured page cap (max_pages); 0/undefined = uncapped. */
  maxPages?: number;
  maxDepth?: number;
  /** Wall-clock seconds the job has been running (client-derived). */
  elapsedSec?: number;
}

/**
 * Map a polled crawl job payload + the running embed snapshot into a
 * `CrawlSnapshot`. Pure: no I/O, no clock reads — `elapsedSec` is passed in.
 */
export function summarizeCrawl(
  crawlPayload: unknown,
  input: CrawlSummaryInput,
  embedPayload?: unknown,
): CrawlSnapshot {
  const job = jobFromPayload(crawlPayload) ?? {};
  const result = asRecord(job.result_json) ?? {};
  const status = str(job.status) ?? "running";
  let phase = phaseFor(status);

  const fetched = num(result.pages_crawled);
  const docs = num(result.md_created);
  const errorCount = num(result.error_pages);
  const embedJobId = str(result.embed_job_id);

  // `queued` is the backend's live frontier count. Older/alternate payloads may
  // only expose `pages_discovered`, which is a total discovered count, so subtract
  // fetched pages instead of displaying the total as pending work.
  const queued = deriveQueued(result, fetched);

  const events = parseEvents(result.events);
  const rateLimited = parseRateLimited(result.rate_limited);

  const rawDepthMax = optNum(result.depth_max) ?? input.maxDepth ?? null;
  const depthMax = rawDepthMax && rawDepthMax > 0 ? rawDepthMax : null;
  // Spider doesn't surface crawl-hop depth, so derive a real "depth reached" from
  // how far below the seed path the crawled URLs have gone (seed = depth 1).
  const depthCurrent = optNum(result.depth_current) ?? deriveDepthReached(input.url, events);

  // Embed phase: once the crawl is done and an embed job exists, fold its
  // progress in. This is the honest two-phase model — embedded/chunks are only
  // non-zero after the crawl completes.
  let embedded = 0;
  let chunks = 0;
  let embedStatus: string | null = null;
  let embedUpdatedAtMs: number | null = null;
  let embedErrorText: string | null = null;
  const embedJob = jobFromPayload(embedPayload);
  if (embedJob) {
    const embedResult = asRecord(embedJob.result_json) ?? {};
    embedded = num(embedResult.docs_embedded);
    chunks = num(embedResult.chunks_embedded);
    embedStatus = str(embedJob.status);
    embedUpdatedAtMs = parseTimestamp(embedJob.updated_at);
    embedErrorText = str(embedJob.error_text);
  }

  // After the crawl finishes, the embed job is the active phase until it reaches
  // a terminal state. A crawl that produced no docs (no embed job) is just done.
  if (phase === "done" && embedJobId) {
    if (embedStatus === "failed") {
      phase = "failed";
    } else if (embedStatus === "canceled") {
      phase = "canceled";
    } else if (embedStatus === "completed") {
      phase = "done";
    } else {
      phase = "embedding";
    }
  }

  // Percent: crawl progress drives the bar while crawling; embed progress drives
  // it during the embed phase; 100% when fully done.
  const cap = input.maxPages && input.maxPages > 0 ? input.maxPages : 0;
  let percent: number;
  if (phase === "done") {
    percent = 100;
  } else if (phase === "failed" || phase === "canceled") {
    percent = clampPct(ratio(fetched, fetched + queued) * 100);
  } else if (phase === "embedding") {
    percent = docs > 0 ? clampPct((embedded / docs) * 100) : 100;
  } else if (cap > 0) {
    percent = clampPct((fetched / cap) * 100);
  } else {
    percent = clampPct(ratio(fetched, fetched + queued) * 100);
  }

  const elapsedSec = input.elapsedSec ?? 0;
  const ratePerSec = elapsedSec > 0 ? fetched / elapsedSec : 0;
  const etaText =
    phase === "crawling" || phase === "pending" ? formatEta(queued, ratePerSec) : null;

  return {
    jobId: input.jobId,
    host: hostFromUrl(input.url),
    url: input.url,
    status,
    phase,
    fetched,
    queued,
    docs,
    embedded,
    chunks,
    depthCurrent,
    depthMax,
    percent,
    etaText,
    events,
    rateLimited,
    errorCount,
    errorText: phase === "failed" || phase === "canceled" ? (embedErrorText ?? str(job.error_text)) : str(job.error_text),
    embedJobId,
    // During embedding, liveness comes from the embed job's heartbeat, not the
    // (now-frozen) crawl row.
    updatedAtMs: phase === "embedding" && embedUpdatedAtMs != null ? embedUpdatedAtMs : parseTimestamp(job.updated_at),
    startedAtMs: parseTimestamp(job.started_at),
  };
}

function deriveQueued(result: JsonRecord, fetched: number): number {
  const direct = optNum(result.queued ?? result.pending_pages ?? result.frontier_pending ?? result.frontier_count);
  if (direct != null) return Math.max(direct, 0);

  const discovered = optNum(
    result.pages_discovered ?? result.discovered_pages ?? result.urls_discovered ?? result.total_discovered,
  );
  if (discovered != null) return Math.max(discovered - fetched, 0);

  return 0;
}

/** Max path depth below the seed across recent crawled URLs (seed = depth 1). */
function deriveDepthReached(seedUrl: string, events: CrawlLogEvent[]): number | null {
  let seedSegs: string[];
  try {
    seedSegs = new URL(seedUrl).pathname.split("/").filter(Boolean);
  } catch {
    return null;
  }
  let max = 0;
  for (const event of events) {
    if (!event.url) continue;
    let segs: string[];
    try {
      segs = new URL(event.url).pathname.split("/").filter(Boolean);
    } catch {
      continue;
    }
    if (!seedSegs.every((seg, i) => segs[i] === seg)) continue; // only under the seed
    const depth = segs.length - seedSegs.length + 1;
    if (depth > max) max = depth;
  }
  return max > 0 ? max : null;
}

function parseTimestamp(value: unknown): number | null {
  const s = str(value);
  if (!s) return null;
  const ms = Date.parse(s);
  return Number.isNaN(ms) ? null : ms;
}

function ratio(part: number, whole: number): number {
  return whole > 0 ? part / whole : 0;
}

function clampPct(value: number): number {
  if (!Number.isFinite(value)) return 0;
  return Math.max(0, Math.min(100, value));
}
