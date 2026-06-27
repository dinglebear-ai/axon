// Pure, side-effect-free helpers extracted from App.tsx. Kept framework-free so
// they stay trivially unit-testable and reusable by the palette hooks.
import { coercesArgumentToUrl, type PaletteAction } from "@/lib/actions";
import type { RunState } from "@/lib/runState";

export function currentOutputTarget(
  run: RunState,
  active: PaletteAction | null | undefined,
  query: string,
): string {
  if (run.kind === "idle") return query.trim() || active?.subcommand || "";
  if ("result" in run) {
    const payload = run.result.payload;
    if (payload && typeof payload === "object") {
      const record = payload as Record<string, unknown>;
      const url = record.url ?? record.requested_url ?? record.target;
      if (typeof url === "string" && url) return url;
    }
  }
  if ("text" in run) return firstUrlFromText(run.text) ?? run.subtitle;
  return run.subtitle;
}

export function firstUrlFromText(value: string): string | null {
  return value.match(/https?:\/\/[^\s"')\]}]+/i)?.[0] ?? null;
}

export function newRequestId(): string {
  return globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

export function normalizeSubmitArgument(action: PaletteAction, argument: string): string {
  const trimmed = argument.trim();
  if (coercesArgumentToUrl(action) && trimmed && !/^https?:\/\//i.test(trimmed)) {
    return `https://${trimmed}`;
  }
  return trimmed;
}

export function crawlSeedUrl(argument: string): string {
  const first = argument.trim().split(/\s+/)[0] ?? "";
  if (!first) return "";
  return /^https?:\/\//i.test(first) ? first : `https://${first}`;
}

export function extractEmbedJobId(payload: unknown): string | null {
  if (!payload || typeof payload !== "object") return null;
  const root = payload as Record<string, unknown>;
  const job = (typeof root.job === "object" && root.job ? root.job : root) as Record<string, unknown>;
  const result = job.result_json;
  if (result && typeof result === "object") {
    const id = (result as Record<string, unknown>).embed_job_id;
    if (typeof id === "string" && id) return id;
  }
  return null;
}
