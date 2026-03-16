import { describe, expect, it } from 'vitest'
import { createTrackedSetter } from '@/hooks/ws-messages/tracked'

describe('createTrackedSetter', () => {
  it('updates ref and state for direct assignments', () => {
    let state = 'initial'
    const ref = { current: state }
    const setState = (action: string | ((prev: string) => string)) => {
      state = typeof action === 'function' ? action(state) : action
    }

    const setTracked = createTrackedSetter(setState, ref)
    setTracked('next')

    expect(state).toBe('next')
    expect(ref.current).toBe('next')
  })

  it('updates ref and state for functional updaters', () => {
    let state = [1]
    const ref = { current: state }
    const setState = (action: number[] | ((prev: number[]) => number[])) => {
      state = typeof action === 'function' ? action(state) : action
    }

    const setTracked = createTrackedSetter(setState, ref)
    setTracked((prev) => [...prev, 2, 3])

    expect(state).toEqual([1, 2, 3])
    expect(ref.current).toEqual([1, 2, 3])
  })
})
