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
