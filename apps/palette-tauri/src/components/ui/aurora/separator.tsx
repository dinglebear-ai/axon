"use client"

import type * as React from "react"
import { cn } from "@/lib/utils"

export interface SeparatorProps extends React.HTMLAttributes<HTMLDivElement> {
  orientation?: "horizontal" | "vertical"
  decorative?: boolean
  ref?: React.Ref<HTMLDivElement>
}

function Separator({
  className,
  orientation = "horizontal",
  decorative = true,
  style,
  ref,
  ...props
}: SeparatorProps) {
  return (
    // biome-ignore lint/a11y/useAriaPropsSupportedByRole: aria-orientation is only set on the "separator" role path; the decorative "none" path leaves it undefined
    <div
      ref={ref}
      role={decorative ? "none" : "separator"}
      aria-orientation={decorative ? undefined : orientation}
      className={cn(orientation === "vertical" ? "h-full min-h-5 w-px" : "h-px w-full", className)}
      style={{
        background: "var(--aurora-border-default)",
        ...style,
      }}
      {...props}
    />
  )
}
Separator.displayName = "Separator"

export { Separator }
export default Separator
