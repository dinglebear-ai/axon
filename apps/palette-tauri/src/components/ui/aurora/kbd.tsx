import * as React from "react"
import { cn } from "@/lib/utils"

export type KbdProps = React.HTMLAttributes<HTMLElement>

const Kbd = React.forwardRef<HTMLElement, KbdProps>(({ className, style, ...props }, ref) => (
  <kbd
    ref={ref}
    className={cn("inline-flex min-w-5 items-center justify-center rounded-[5px] border px-1.5", className)}
    style={{
      background: "var(--aurora-control-surface)",
      borderColor: "var(--aurora-border-strong)",
      boxShadow: "inset 0 -1px 0 rgba(0,0,0,0.35), var(--aurora-highlight-medium)",
      color: "var(--aurora-text-muted)",
      fontFamily: "var(--aurora-font-mono)",
      fontSize: 11,
      fontWeight: 600,
      height: 20,
      lineHeight: 1,
      ...style,
    }}
    {...props}
  />
))
Kbd.displayName = "Kbd"

export { Kbd }
export default Kbd
