"use client"

import * as React from "react"
import { cn } from "@/lib/utils"

export interface SeparatorProps extends React.HTMLAttributes<HTMLDivElement> {
  orientation?: "horizontal" | "vertical"
  decorative?: boolean
}

const Separator = React.forwardRef<HTMLDivElement, SeparatorProps>(
  ({ className, orientation = "horizontal", decorative = true, style, ...props }, ref) => (
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
)
Separator.displayName = "Separator"

export { Separator }
export default Separator
