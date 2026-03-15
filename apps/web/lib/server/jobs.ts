import type { JobStatus } from './job-types'

export type StatusFilter =
  | 'all'
  | 'active'
  | 'pending'
  | 'running'
  | 'completed'
  | 'failed'
  | 'canceled'

export type JsonRecord = Record<string, unknown>

export function statusValues(filter: StatusFilter): string[] | null {
  switch (filter) {
    case 'active':
      return ['pending', 'running']
    case 'pending':
      return ['pending']
    case 'running':
      return ['running']
    case 'completed':
      return ['completed']
    case 'failed':
      return ['failed', 'canceled']
    case 'canceled':
      return ['canceled']
    default:
      return null
  }
}

export function statusClause(
  filter: StatusFilter,
  startAt: number,
): { clause: string; params: unknown[] } {
  const vals = statusValues(filter)
  if (!vals) return { clause: '1=1', params: [] }
  return { clause: `status = ANY($${startAt}::text[])`, params: [vals] }
}

export function asJsonRecord(value: unknown): JsonRecord {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as JsonRecord) : {}
}

export function stringOrNull(value: unknown): string | null {
  return typeof value === 'string' ? value : null
}

export function boolOrNull(value: unknown): boolean | null {
  return typeof value === 'boolean' ? value : null
}

export function numberOrNull(value: unknown): number | null {
  return typeof value === 'number' && Number.isFinite(value) ? value : null
}

export function stringArray(value: unknown): string[] {
  if (!Array.isArray(value)) return []
  return value.filter((v): v is string => typeof v === 'string')
}

export function isoDateOrNull(value: unknown): string | null {
  return value instanceof Date ? value.toISOString() : null
}

export function jobSuccessFromStatus(status: unknown): boolean | null {
  if (status === 'completed') return true
  if (status === 'failed' || status === 'canceled') return false
  return null
}

export function truncateJobTarget(value: string | null | undefined, max = 120): string {
  if (!value) return '—'
  return value.length > max ? `${value.slice(0, max)}…` : value
}

export function summarizeUrls(value: unknown): string {
  const urls = stringArray(value)
  const first = urls[0] ?? '—'
  return urls.length > 1 ? `${first} (+${urls.length - 1})` : first
}

export function safeJobStatus(status: unknown, fallback: JobStatus): JobStatus {
  return typeof status === 'string' ? (status as JobStatus) : fallback
}
