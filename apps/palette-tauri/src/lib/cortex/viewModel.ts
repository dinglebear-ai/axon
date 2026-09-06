import type { ProductIdentity } from "../backendProfiles/model";

export type CortexTab = "logs" | "sessions" | "fleet" | "graph" | "correlate";
export const CORTEX_CAPABILITY: Record<CortexTab, string> = {
  logs: "logs.query",
  sessions: "sessions.rendered",
  fleet: "fleet.read",
  graph: "graph.read",
  correlate: "correlation.read",
};
export const CORTEX_ROW_LIMIT = 500;
export const CORTEX_RENDER_WINDOW = 80;
export const CORTEX_RETAINED_BYTES = 512 * 1024;

export function capabilityAvailable(identity: ProductIdentity | null, tab: CortexTab) {
  return identity?.capabilities.includes(CORTEX_CAPABILITY[tab]) ?? false;
}

export function boundedAppend<T>(current: T[], next: T[]) {
  return [...current, ...next].slice(-CORTEX_ROW_LIMIT);
}

export function visibleWindow<T>(rows: T[], scrollTop: number, rowHeight = 52, viewport = 520) {
  if (rows.length <= CORTEX_RENDER_WINDOW) return { rows, start: 0, top: 0, bottom: 0 };
  const start = Math.max(0, Math.floor(scrollTop / rowHeight) - 8);
  const count = Math.min(CORTEX_RENDER_WINDOW, Math.ceil(viewport / rowHeight) + 16);
  const end = Math.min(rows.length, start + count);
  return {
    rows: rows.slice(start, end),
    start,
    top: start * rowHeight,
    bottom: (rows.length - end) * rowHeight,
  };
}

export function safeText(value: unknown, max = 4000) {
  const text = typeof value === "string" ? value : String(value ?? "");
  return text.length > max ? `${text.slice(0, max)}…` : text;
}

export function boundedByItemsAndBytes<T>(current: T[], next: T[], bytes: (item: T) => number) {
  const rows = [...current, ...next].slice(-CORTEX_ROW_LIMIT);
  let total = 0;
  let start = rows.length;
  while (start > 0) {
    const size = bytes(rows[start - 1]);
    if (total + size > CORTEX_RETAINED_BYTES) break;
    total += size;
    start -= 1;
  }
  return rows.slice(start);
}
