import { describe, expect, it } from 'vitest'
import {
  parseJobsListParams,
  VALID_JOB_FILTER_STATUSES,
  VALID_JOB_FILTER_TYPES,
} from '@/lib/server/jobs-query'

describe('jobs query parsing', () => {
  it('returns defaults for empty search params', () => {
    const result = parseJobsListParams(new URLSearchParams())

    expect(result).toEqual({
      limit: 50,
      offset: 0,
      status: 'all',
      type: 'all',
    })
  })

  it('rejects invalid type filters', () => {
    expect(() => parseJobsListParams(new URLSearchParams('type=foo'))).toThrow(
      'Invalid type filter: foo',
    )
  })

  it('rejects invalid status filters', () => {
    expect(() => parseJobsListParams(new URLSearchParams('status=done'))).toThrow(
      'Invalid status filter: done',
    )
  })

  it('clamps limit and offset values', () => {
    const result = parseJobsListParams(new URLSearchParams('limit=999&offset=-5'))

    expect(result.limit).toBe(200)
    expect(result.offset).toBe(0)
  })

  it('keeps refresh in the allowed type filter set', () => {
    expect(VALID_JOB_FILTER_TYPES.has('refresh')).toBe(true)
  })

  it('keeps failed and canceled in the allowed status filter set', () => {
    expect(VALID_JOB_FILTER_STATUSES.has('failed')).toBe(true)
    expect(VALID_JOB_FILTER_STATUSES.has('canceled')).toBe(true)
  })
})
