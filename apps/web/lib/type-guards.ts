/** Type guard: value is a non-null, non-array object */
export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

/** Check if a record has all specified keys */
export function hasKeys<K extends string>(
  obj: Record<string, unknown>,
  ...keys: K[]
): obj is Record<K, unknown> {
  return keys.every((k) => k in obj)
}
