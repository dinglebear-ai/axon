import * as React from "react"
import { cn, devWarn } from "@/lib/utils"

export type StatusTone = "online" | "syncing" | "queued" | "degraded" | "offline" | "error" | "automating"

export const toneColor: Record<StatusTone, { color: string; shadow: string }> = {
  online:     { color: "var(--aurora-success)",        shadow: "0 0 10px var(--aurora-success)" },
  syncing:    { color: "var(--aurora-info)",           shadow: "0 0 10px var(--aurora-info)" },
  queued:     { color: "var(--aurora-neutral)",        shadow: "0 0 10px var(--aurora-neutral)" },
  degraded:   { color: "var(--aurora-warn)",           shadow: "0 0 10px var(--aurora-warn)" },
  offline:    { color: "var(--aurora-neutral)",        shadow: "0 0 10px var(--aurora-neutral)" },
  error:      { color: "var(--aurora-error)",          shadow: "0 0 10px var(--aurora-error)" },
  automating: { color: "var(--aurora-accent-violet)",  shadow: "0 0 10px var(--aurora-accent-violet)" },
}

// queued and offline use --aurora-neutral-foreground so labels don't compete visually with the dot
const dimTones = new Set<StatusTone>(["queued", "offline"])

const pulseTones = new Set<StatusTone>(["syncing", "automating"])

export interface StatusIndicatorProps extends React.HTMLAttributes<HTMLSpanElement> {
  tone?: StatusTone
  label?: React.ReactNode
  pulse?: boolean
}

function StatusIndicator({ className, tone = "online", label, pulse, style, ...props }: StatusIndicatorProps) {
  const safeTone = Object.hasOwn(toneColor, tone) ? tone : "online"
  if (tone !== safeTone) {
    devWarn(`[Aurora StatusIndicator] Unknown tone "${tone}". Valid values: ${Object.keys(toneColor).join(", ")}. Falling back to "online".`)
  }

  const resolvedPulse = pulse ?? pulseTones.has(safeTone)
  const { color, shadow } = toneColor[safeTone]
  const labelColor = dimTones.has(safeTone)
    ? "var(--aurora-neutral-foreground)"
    : "var(--aurora-text-primary)"

  return (
    <span
      className={cn("inline-flex items-center gap-2", className)}
      style={{
        color: labelColor,
        fontSize: "var(--aurora-type-body-sm)",
        fontWeight: "var(--aurora-weight-ui)",
        lineHeight: "var(--aurora-line-ui)",
        ...style,
      }}
      {...props}
    >
      <span
        aria-hidden="true"
        className={cn("size-2 rounded-full", resolvedPulse && "animate-pulse")}
        style={{ background: color, boxShadow: shadow }}
      />
      {label ?? safeTone}
    </span>
  )
}

export { StatusIndicator }
export default StatusIndicator
