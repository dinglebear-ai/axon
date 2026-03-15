'use client'

import { useEffect, useState } from 'react'

/**
 * Returns true when the viewport is below the `lg` breakpoint (1024px),
 * matching the Tailwind `lg:` prefix used throughout the shell layout.
 *
 * Initialises to `false` (server-safe) and updates after the first paint,
 * eliminating a flash of wrong layout on hydration.
 */
export function useIsMobile(): boolean {
  const [isMobile, setIsMobile] = useState(false)

  useEffect(() => {
    const media = window.matchMedia('(max-width: 1023px)')
    const update = () => setIsMobile(media.matches)
    update()
    media.addEventListener('change', update)
    return () => media.removeEventListener('change', update)
  }, [])

  return isMobile
}

/** Returns true only for phone-sized viewports (< 640px). */
export function useIsPhone(): boolean {
  const [isPhone, setIsPhone] = useState(false)

  useEffect(() => {
    const media = window.matchMedia('(max-width: 639px)')
    const update = () => setIsPhone(media.matches)
    update()
    media.addEventListener('change', update)
    return () => media.removeEventListener('change', update)
  }, [])

  return isPhone
}

/** Returns true for tablet-sized viewports (640px - 1023px). */
export function useIsTablet(): boolean {
  const [isTablet, setIsTablet] = useState(false)

  useEffect(() => {
    const media = window.matchMedia('(min-width: 640px) and (max-width: 1023px)')
    const update = () => setIsTablet(media.matches)
    update()
    media.addEventListener('change', update)
    return () => media.removeEventListener('change', update)
  }, [])

  return isTablet
}

/** Returns true for desktop-sized viewports (>= 1024px). */
export function useIsDesktop(): boolean {
  const [isDesktop, setIsDesktop] = useState(false)

  useEffect(() => {
    const media = window.matchMedia('(min-width: 1024px)')
    const update = () => setIsDesktop(media.matches)
    update()
    media.addEventListener('change', update)
    return () => media.removeEventListener('change', update)
  }, [])

  return isDesktop
}
