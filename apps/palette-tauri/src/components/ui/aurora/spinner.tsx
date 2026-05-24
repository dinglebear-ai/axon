import * as React from "react"
import { cn } from "@/lib/utils"

export interface SpinnerProps extends React.HTMLAttributes<HTMLSpanElement> {
  size?: "sm" | "default" | "lg"
  tone?: "cyan" | "rose" | "muted"
}

const sizeMap = {
  sm: 14,
  default: 18,
  lg: 24,
}

const toneMap = {
  cyan: "var(--aurora-accent-primary)",
  rose: "var(--aurora-accent-pink)",
  muted: "var(--aurora-text-muted)",
}

const Spinner = React.forwardRef<HTMLSpanElement, SpinnerProps>(
  ({ className, size = "default", tone = "cyan", style, ...props }, ref) => {
    const px = sizeMap[size]
    const color = toneMap[tone]

    return (
      <span
        ref={ref}
        aria-label="Loading"
        className={cn("inline-block animate-spin rounded-full", className)}
        style={{
          width: px,
          height: px,
          border: `2px solid color-mix(in srgb, ${color} 22%, transparent)`,
          borderTopColor: color,
          boxShadow: `0 0 10px color-mix(in srgb, ${color} 20%, transparent)`,
          ...style,
        }}
        {...props}
      />
    )
  }
)
Spinner.displayName = "Spinner"

export { Spinner }
export default Spinner
