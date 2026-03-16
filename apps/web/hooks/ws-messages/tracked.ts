import type { Dispatch, MutableRefObject, SetStateAction } from 'react'

export function createTrackedSetter<T>(
  setState: Dispatch<SetStateAction<T>>,
  ref: MutableRefObject<T>,
): Dispatch<SetStateAction<T>> {
  return (action) => {
    if (typeof action === 'function') {
      setState((prev) => {
        const next = (action as (prev: T) => T)(prev)
        ref.current = next
        return next
      })
      return
    }

    ref.current = action
    setState(action)
  }
}
