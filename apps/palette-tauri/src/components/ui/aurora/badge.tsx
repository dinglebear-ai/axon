// BREAKING: badgeVariants export removed in this version. See badgeVariants deprecation shim below.

import * as React from "react"
import { cn, devWarn } from "@/lib/utils"

export type BadgeTone = "info" | "success" | "warn" | "error" | "neutral" | "rose" | "violet"

type ToneTokens = { text: string; border: string; bg: string; dot: string; dotShadow: string }

const badgeToneMap: Record<BadgeTone, ToneTokens> = {
  info: {
    text:      "var(--aurora-info-foreground)",
    border:    "var(--aurora-info-border)",
    bg:        "var(--aurora-info-surface)",
    dot:       "var(--aurora-info)",
    dotShadow: "0 0 4px var(--aurora-info)",
  },
  success: {
    text:      "var(--aurora-success-foreground)",
    border:    "var(--aurora-success-border)",
    bg:        "var(--aurora-success-surface)",
    dot:       "var(--aurora-success)",
    dotShadow: "0 0 4px var(--aurora-success)",
  },
  warn: {
    text:      "var(--aurora-warn-foreground)",
    border:    "var(--aurora-warn-border)",
    bg:        "var(--aurora-warn-surface)",
    dot:       "var(--aurora-warn)",
    dotShadow: "0 0 4px var(--aurora-warn)",
  },
  error: {
    text:      "var(--aurora-error-foreground)",
    border:    "var(--aurora-error-border)",
    bg:        "var(--aurora-error-surface)",
    dot:       "var(--aurora-error)",
    dotShadow: "0 0 4px var(--aurora-error)",
  },
  neutral: {
    text:      "var(--aurora-neutral-foreground)",
    border:    "var(--aurora-neutral-border)",
    bg:        "var(--aurora-neutral-surface)",
    dot:       "var(--aurora-neutral)",
    dotShadow: "0 0 4px var(--aurora-neutral)",
  },
  rose: {
    text:      "var(--aurora-accent-pink-strong)",
    border:    "var(--aurora-accent-pink-border)",
    bg:        "var(--aurora-accent-pink-surface)",
    dot:       "var(--aurora-accent-pink)",
    dotShadow: "0 0 4px var(--aurora-accent-pink)",
  },
  violet: {
    text:      "var(--aurora-accent-violet-strong)",
    border:    "var(--aurora-accent-violet-border)",
    bg:        "var(--aurora-accent-violet-surface)",
    dot:       "var(--aurora-accent-violet)",
    dotShadow: "0 0 4px var(--aurora-accent-violet)",
  },
}

// ---------------------------------------------------------------------------
// Pulse keyframe injection
// ---------------------------------------------------------------------------

const PULSE_ID = "aurora-badge-pulse"

function injectPulse() {
  if (typeof document === "undefined") return
  if (document.getElementById(PULSE_ID)) return
  const style = document.createElement("style")
  style.id = PULSE_ID
  style.textContent = `
    @keyframes aurora-badge-pulse {
      0%, 100% {
        box-shadow:
          0 0 0 0 color-mix(in srgb, var(--badge-dot-color) 40%, transparent),
          0 0 4px var(--badge-dot-color);
      }
      50% {
        box-shadow:
          0 0 0 4px transparent,
          0 0 6px var(--badge-dot-color);
      }
    }
    .aurora-badge-dot--pulse {
      animation: aurora-badge-pulse 1.6s ease-in-out infinite;
    }
    @media (prefers-reduced-motion: reduce) {
      .aurora-badge-dot--pulse {
        animation: none;
      }
    }
  `
  document.head.appendChild(style)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function resolveTone(variant: BadgeTone | "default" | undefined): BadgeTone {
  if (!variant) return "neutral"
  if (variant === "default") {
    devWarn('[Aurora Badge] variant="default" is deprecated. Use variant="neutral" instead.')
    return "neutral"
  }
  if (!Object.hasOwn(badgeToneMap, variant)) {
    devWarn(
      `[Aurora Badge] Unknown variant "${variant}". Valid values: ${Object.keys(badgeToneMap).join(", ")}. Falling back to "neutral".`
    )
    return "neutral"
  }
  return variant
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface BadgeProps extends React.HTMLAttributes<HTMLSpanElement> {
  /** Semantic or expressive tone. "default" is a deprecated alias for "neutral". */
  variant?: BadgeTone | "default"
  /** Alias for variant — preferred in skill/docs usage: <Badge tone="info"> */
  tone?: BadgeTone | "default"
  /** Render a status dot to the left of the label. */
  dot?: boolean
  /**
   * Animate the dot with a pulse ring — use for "live", "recording", or
   * "connected" indicators. Has no effect when `dot` is false.
   */
  pulse?: boolean
  /** Visual size. Defaults to "default". */
  size?: "sm" | "default"
  /**
   * Typography and radius shape:
   * - "label"  (default) — uppercase mono, 4px radius. Status codes, tech labels.
   * - "tag"    — sentence-case sans, 4px radius. Content tags, user labels.
   * - "pill"   — sentence-case sans, 999px radius. Fully rounded chips.
   */
  shape?: "label" | "tag" | "pill"
  /**
   * Render as a clickable chip (filter tags, toggleable labels).
   * Adds cursor-pointer, focus ring, and keyboard activation (Enter/Space).
   * When `onClick` is also provided, `role="button"` is applied automatically.
   */
  interactive?: boolean
}

// ---------------------------------------------------------------------------
// Badge
// ---------------------------------------------------------------------------

const Badge = React.forwardRef<HTMLSpanElement, BadgeProps>(
  (
    {
      className,
      variant,
      tone: toneProp,
      dot = false,
      pulse = false,
      size = "default",
      shape = "label",
      interactive = false,
      style,
      children,
      onClick,
      onKeyDown,
      ...props
    },
    ref
  ) => {
    const tone = resolveTone(toneProp ?? variant)
    const { text, border, bg, dot: dotColor, dotShadow } = badgeToneMap[tone]

    // Inject pulse keyframes lazily — only when the feature is first used.
    React.useEffect(() => {
      if (pulse && dot) injectPulse()
    }, [pulse, dot])

    // -----------------------------------------------------------------------
    // Size tokens
    // -----------------------------------------------------------------------
    const isSm = size === "sm"
    const dotSize = isSm ? "4px" : "5px"
    const badgeRadius =
      shape === "pill" ? "999px" : "4px"
    const badgeFontSize = isSm
      ? "var(--aurora-type-caption)"
      : "var(--aurora-type-micro)"

    // -----------------------------------------------------------------------
    // Shape tokens
    // -----------------------------------------------------------------------
    const isLabel = shape === "label"
    const fontFamily = isLabel
      ? "var(--aurora-font-mono, 'JetBrains Mono', monospace)"
      : "var(--aurora-font-sans, Inter, sans-serif)"
    const letterSpacing = isLabel ? "0.075em" : "0.01em"

    // -----------------------------------------------------------------------
    // Interactive keyboard handler
    // -----------------------------------------------------------------------
    const handleKeyDown = React.useCallback(
      (e: React.KeyboardEvent<HTMLSpanElement>) => {
        onKeyDown?.(e)
        if (interactive && onClick && (e.key === "Enter" || e.key === " ")) {
          e.preventDefault()
          onClick(e as unknown as React.MouseEvent<HTMLSpanElement>)
        }
      },
      [interactive, onClick, onKeyDown]
    )

    // -----------------------------------------------------------------------
    // Derived accessibility attributes
    // -----------------------------------------------------------------------
    const interactiveProps = interactive
      ? {
          tabIndex: 0,
          role: onClick ? ("button" as const) : undefined,
          onKeyDown: handleKeyDown,
          onClick,
        }
      : { onClick }

    return (
      <span
        ref={ref}
        className={cn(
          "inline-flex items-center gap-1.5 leading-none border whitespace-nowrap",
          // Size
          isSm ? "px-1.5 py-0" : "px-2 py-0.5",
          // Shape: uppercase only for "label"
          isLabel && "uppercase",
          // Interactive
          interactive && [
            "cursor-pointer",
            "transition-[box-shadow,filter,transform] duration-150",
            "hover:brightness-125",
            "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--aurora-focus-ring)]",
          ],
          className
        )}
        style={{
          borderRadius: badgeRadius,
          background: bg,
          borderColor: border,
          color: text,
          fontFamily,
          fontSize: badgeFontSize,
          fontWeight: 650,
          letterSpacing,
          ...style,
        }}
        {...interactiveProps}
        {...props}
      >
        {dot && (
          <span
            aria-hidden="true"
            className={cn(pulse && "aurora-badge-dot--pulse")}
            style={{
              display: "inline-block",
              width: dotSize,
              height: dotSize,
              borderRadius: "50%",
              backgroundColor: dotColor,
              flexShrink: 0,
              // Static glow when not pulsing; animation handles it when pulsing.
              boxShadow: pulse ? undefined : dotShadow,
              // CSS custom property consumed by the keyframe so one rule
              // works across all 7 tones.
              ["--badge-dot-color" as string]: dotColor,
            }}
          />
        )}
        {children}
      </span>
    )
  }
)
Badge.displayName = "Badge"

export { Badge }
export default Badge

/** @deprecated badgeVariants has been removed. Use the Badge component directly. */
export const badgeVariants = undefined
