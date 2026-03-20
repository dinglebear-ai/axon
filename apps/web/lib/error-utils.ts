/** Extract a human-readable message from an unknown caught value. */
export function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err)
}
