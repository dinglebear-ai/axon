import { arrField, unwrapPayload } from "@/lib/payload";

export interface SourceRow {
  url: string;
  chunks: number;
}

export interface SourceGroup {
  domain: string;
  list: SourceRow[];
  chunks: number;
}

export type SourceSortMode = "chunks" | "url";

export interface SourcesModel {
  rows: SourceRow[];
  filtered: SourceRow[];
  groups: SourceGroup[] | null;
  totalChunks: number;
}

/**
 * Parse the server `sources` payload into display rows.
 *
 * @param payload - Raw `sources` response payload.
 * @returns Source rows with URL and chunk counts.
 */
export function parseSourceRows(payload: unknown): SourceRow[] {
  const data = unwrapPayload(payload);
  return arrField(data, "urls").flatMap((entry) => {
    if (!Array.isArray(entry)) return [];
    const url = typeof entry[0] === "string" ? entry[0] : "";
    const chunks = typeof entry[1] === "number" ? entry[1] : 0;
    return url ? [{ url, chunks }] : [];
  });
}

/**
 * Extract the display domain for a source URL.
 *
 * @param url - Source URL or URL-like string.
 * @returns Host/domain label used for grouping.
 */
export function sourceDomain(url: string): string {
  try {
    return new URL(url).host.replace(/^www\./, "");
  } catch {
    return url.replace(/^https?:\/\//, "").split("/")[0] || url;
  }
}

/**
 * Build the filtered, sorted, optionally grouped model consumed by SourcesView.
 *
 * @param payload - Raw `sources` response payload.
 * @param filter - Case-insensitive URL substring filter.
 * @param sort - Active source row sort mode.
 * @param grouped - Whether rows should be grouped by domain.
 * @returns Derived model for the presentational sources view.
 */
export function buildSourcesModel(
  payload: unknown,
  filter: string,
  sort: SourceSortMode,
  grouped: boolean,
): SourcesModel {
  const rows = parseSourceRows(payload);
  const needle = filter.trim().toLowerCase();
  const matched = needle ? rows.filter((row) => row.url.toLowerCase().includes(needle)) : rows;
  const filtered = [...matched].sort((a, b) =>
    sort === "chunks" ? b.chunks - a.chunks : a.url.localeCompare(b.url),
  );
  const totalChunks = filtered.reduce((sum, row) => sum + row.chunks, 0);
  if (!grouped) return { rows, filtered, groups: null, totalChunks };

  const byDomain = new Map<string, SourceRow[]>();
  for (const row of filtered) {
    const key = sourceDomain(row.url);
    const list = byDomain.get(key);
    if (list) list.push(row);
    else byDomain.set(key, [row]);
  }

  const groups = [...byDomain.entries()]
    .map(([domain, list]) => ({ domain, list, chunks: list.reduce((sum, row) => sum + row.chunks, 0) }))
    .sort((a, b) => b.chunks - a.chunks);
  return { rows, filtered, groups, totalChunks };
}
