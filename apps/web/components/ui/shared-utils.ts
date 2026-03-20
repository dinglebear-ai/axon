/**
 * Shared utilities for UI components.
 * Extracted to eliminate duplication across interactive/static component pairs.
 */

import { cva } from 'class-variance-authority'

import { cn } from '@/lib/utils'

/**
 * Safe URL protocols allowed in user-provided hrefs.
 * Blocks javascript:, data:, vbscript:, and other dangerous schemes.
 */
const SAFE_URL_PROTOCOLS = new Set(['http:', 'https:', 'blob:', 'mailto:', 'tel:'])

/**
 * Sanitize a user-provided URL for use in href attributes.
 * Returns the original URL if safe, or '#' if blocked/invalid.
 */
export function getSafeHref(url: string | undefined): string {
  if (!url) return '#'

  const trimmed = url.trim()
  if (!trimmed) return '#'

  // Allow relative paths (but not protocol-relative URLs like //evil.com)
  if (
    (trimmed.startsWith('/') && !trimmed.startsWith('//')) ||
    trimmed.startsWith('./') ||
    trimmed.startsWith('../')
  ) {
    return trimmed
  }

  try {
    const parsed = new URL(trimmed)
    if (SAFE_URL_PROTOCOLS.has(parsed.protocol)) return trimmed
  } catch {
    // URL parsing failed — if there's no colon it's a bare relative path (e.g. "attachments/report.pdf")
    return trimmed.includes(':') ? '#' : trimmed
  }

  return '#'
}

/**
 * Shared heading CVA variants used by both interactive and static heading nodes.
 */
export const headingVariants = cva('relative mb-1', {
  variants: {
    variant: {
      h1: 'mt-[1.6em] pb-1 font-bold font-heading text-4xl',
      h2: 'mt-[1.4em] pb-px font-heading font-semibold text-2xl tracking-tight',
      h3: 'mt-[1em] pb-px font-heading font-semibold text-xl tracking-tight',
      h4: 'mt-[0.75em] font-heading font-semibold text-lg tracking-tight',
      h5: 'mt-[0.75em] font-semibold text-lg tracking-tight',
      h6: 'mt-[0.75em] font-semibold text-base tracking-tight',
    },
  },
})

/**
 * Shared kbd shadow classes for light and dark mode.
 * Used by both interactive and static kbd leaf nodes.
 */
export const KBD_SHADOW_CLASSES =
  'shadow-[rgba(255,_255,_255,_0.1)_0px_0.5px_0px_0px_inset,_rgb(248,_249,_250)_0px_1px_5px_0px_inset,_rgb(193,_200,_205)_0px_0px_0px_0.5px,_rgb(193,_200,_205)_0px_2px_1px_-1px,_rgb(193,_200,_205)_0px_1px_0px_0px] dark:shadow-[rgba(255,_255,_255,_0.1)_0px_0.5px_0px_0px_inset,_rgb(26,_29,_30)_0px_1px_5px_0px_inset,_rgb(76,_81,_85)_0px_0px_0px_0.5px,_rgb(76,_81,_85)_0px_2px_1px_-1px,_rgb(76,_81,_85)_0px_1px_0px_0px]'

/** Shared heading-item CVA for table-of-contents nodes (interactive + static). */
export const headingItemVariants = cva(
  'block h-auto w-full cursor-pointer truncate rounded-none px-0.5 py-1.5 text-left font-medium text-muted-foreground underline decoration-[0.5px] underline-offset-4 hover:bg-accent hover:text-muted-foreground',
  {
    variants: {
      depth: {
        1: 'pl-0.5',
        2: 'pl-[26px]',
        3: 'pl-[50px]',
        4: 'pl-[74px]',
        5: 'pl-[98px]',
        6: 'pl-[122px]',
      },
    },
  },
)

/** Emoji font-family stack used by callout nodes and emoji picker buttons. */
export const EMOJI_FONT_FAMILY =
  '"Apple Color Emoji", "Segoe UI Emoji", NotoColorEmoji, "Noto Color Emoji", "Segoe UI Symbol", "Android Emoji", EmojiSymbols'

/** Shared KaTeX rendering options for equation components. */
export const KATEX_OPTIONS = {
  displayMode: true,
  errorColor: '#cc0000',
  fleqn: false,
  leqno: false,
  macros: { '\\f': '#1f(#2)' },
  output: 'htmlAndMathml' as const,
  strict: 'warn' as const,
  throwOnError: false,
  trust: false,
}

/**
 * Shared base classes for the editor CVA definition.
 * Used by both interactive (editor.tsx) and static (editor-static.tsx) components.
 * Each file extends these with its own base-class overrides and additional variants.
 */
export const EDITOR_BASE_CLASSES = cn(
  'group/editor',
  'relative w-full cursor-text select-text overflow-x-hidden whitespace-pre-wrap break-words',
  'rounded-md ring-offset-background focus-visible:outline-none',
  '[&_strong]:font-bold',
)

/**
 * Shared editor variant values consumed by both interactive and static editor CVA.
 * Intentional divergences (e.g. `aiChat` padding, `comment` variant) stay local to each file.
 */
export const EDITOR_SHARED_VARIANTS = {
  disabled: {
    true: 'cursor-not-allowed opacity-50' as const,
  },
  focused: {
    true: 'ring-2 ring-ring ring-offset-2' as const,
  },
  variant: {
    ai: 'w-full px-0 text-base md:text-sm' as const,
    default: 'size-full px-16 pt-4 pb-72 text-base sm:px-[max(64px,calc(50%-350px))]' as const,
    demo: 'size-full px-16 pt-4 pb-72 text-base sm:px-[max(64px,calc(50%-350px))]' as const,
    fullWidth: 'size-full px-16 pt-4 pb-72 text-base sm:px-24' as const,
    none: '' as const,
    select: 'px-3 py-2 text-base data-readonly:w-fit' as const,
  },
}

/**
 * Format a Date as a human-readable label: "Today", "Yesterday", "Tomorrow",
 * or a locale-formatted long date string.
 */
export function formatElementDate(date: Date): string {
  const today = new Date()
  const isToday = date.toDateString() === today.toDateString()

  const yesterday = new Date(today)
  yesterday.setDate(today.getDate() - 1)
  const isYesterday = date.toDateString() === yesterday.toDateString()

  const tomorrow = new Date(today)
  tomorrow.setDate(today.getDate() + 1)
  const isTomorrow = date.toDateString() === tomorrow.toDateString()

  if (isToday) return 'Today'
  if (isYesterday) return 'Yesterday'
  if (isTomorrow) return 'Tomorrow'

  return date.toLocaleDateString(undefined, {
    day: 'numeric',
    month: 'long',
    year: 'numeric',
  })
}
