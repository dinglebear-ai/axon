export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

// REST responses are enveloped as `{ payload: <actual>, degraded?, errors? }`.
// Unwrap one level to the real data, tolerating already-unwrapped payloads.
export function unwrapPayload(value: unknown): Record<string, unknown> {
  if (!isRecord(value)) return {};
  if (isRecord(value.payload)) return value.payload;
  return value;
}

export function numField(value: Record<string, unknown>, key: string): number | undefined {
  const field = value[key];
  return typeof field === "number" && Number.isFinite(field) ? field : undefined;
}

export function strField(value: Record<string, unknown>, key: string): string | undefined {
  const field = value[key];
  return typeof field === "string" ? field : undefined;
}

export function boolField(value: Record<string, unknown>, key: string): boolean | undefined {
  const field = value[key];
  return typeof field === "boolean" ? field : undefined;
}

export function arrField(value: Record<string, unknown>, key: string): unknown[] {
  const field = value[key];
  return Array.isArray(field) ? field : [];
}

/**
 * First array found among a record's values, or `null` when none exist (or the
 * input is not a record). Preserves the "find the result array regardless of its
 * key" semantics that OperationResultViewShared relied on.
 */
export function firstArray(v: unknown): unknown[] | null {
  if (!isRecord(v)) return null;
  for (const value of Object.values(v)) {
    if (Array.isArray(value)) return value;
  }
  return null;
}

/**
 * Truncate a long identifier for display: values over 12 chars become the first
 * 12 chars plus an ellipsis. Pure truncation — callers guard empty/undefined
 * values themselves (e.g. `id ? shortId(id) : "—"`).
 */
export function shortId(value: string): string {
  return value.length > 12 ? `${value.slice(0, 12)}…` : value;
}

/**
 * Capitalize the first letter of every word, treating whitespace, `/`, and `-`
 * as word boundaries (e.g. `jobs-list` → `Jobs-List`, `a/b` → `A/B`).
 */
export function titleCase(s: string): string {
  return s.replace(/(^|[\s/-])(\w)/g, (_match, sep: string, ch: string) => sep + ch.toUpperCase());
}
